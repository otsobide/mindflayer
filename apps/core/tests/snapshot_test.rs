//! The snapshot every front end reads, against a real workspace on disk:
//! what it serializes to, name by name and in order, and what reading it
//! leaves behind, which is nothing.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::slice;

use mindflayer_core::assign;
use mindflayer_core::install::{self, Edits};
use mindflayer_core::ledger::{Action, InstallRecord, Outcome, Target};
use mindflayer_core::reapply::{self, Change};
use mindflayer_core::resolve::{Inputs, Intent, Reason};
use mindflayer_core::view::{Health, Held, Listing, LoadedWorkspace, Project, Snapshot};
use mindflayer_core::{
    create, hash, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, Reference,
    Share, SourceKind, State, FLAYER_DIR,
};
use tempfile::TempDir;

use common::skill;

/// Seconds since the epoch every time in the fixture is pinned to, so what
/// the ledger says reads the same on every run.
const AT: i64 = 1_790_000_000;

/// A workspace `hub` in a temporary folder, as a front end finds one:
///
/// - its library holds `review` and the rule `commits`, which it offers;
/// - it links `api`, marked by its settings for Claude and Codex and shared,
///   which also name the folders its skills go in, the ones its agents
///   read; `api` authors `deploy`, received `review` from the library, both
///   copies there as they were written, and received the rule `commits`
///   (D-23) as an install writes it;
/// - it links `gone`, whose directory has gone since;
/// - it loads `team`, beside it, which offers `notes`;
/// - its shelf holds `lint`, gathered from one source, and its log the
///   install of `commits` and one gather.
struct Fixture {
    dir: TempDir,
    hub: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    let (mut workspace, _) = FlayerWorkspace::init(&hub).unwrap();
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    common::write_files(
        &library,
        &[("review/SKILL.md", &skill("review", "Read it"))],
    );

    let directories = Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules");
    let (api, _) = MindProject::init_settings(
        hub.join("api"),
        &[Agent::Claude, Agent::Codex],
        Share::Shared,
        &directories,
    )
    .unwrap();
    workspace.link(&api).unwrap();
    // The folders the standard table sends skills to, in an order of its
    // own: settings that say where, which change nothing else.
    reapply::apply(
        Some(&workspace),
        &api,
        &Change::Targets {
            kind: Kind::Skill,
            folders: Some(vec![
                PathBuf::from(".claude/skills"),
                PathBuf::from(".agents/skills"),
            ]),
        },
        &AgentHomes::default(),
    )
    .unwrap();
    let api = MindProject::open(api.root()).unwrap();
    common::write_files(
        api.root(),
        &[
            ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
            (
                ".agents/skills/review/SKILL.md",
                &skill("review", "Read it"),
            ),
            (
                ".claude/skills/review/SKILL.md",
                &skill("review", "Read it"),
            ),
        ],
    );
    create(&workspace.own(), Kind::Rule, "commits", "Write them well").unwrap();
    let parts = assign::install(
        &workspace,
        slice::from_ref(&api),
        &Reference::parse("rule:commits"),
        None,
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap()
    .parts;
    for part in parts {
        assert!(
            part.done
                .as_ref()
                .is_ok_and(|offered| offered.installed.is_ok()),
            "{:?}",
            part.done
        );
    }
    let (gone, _) = MindProject::init(hub.join("gone")).unwrap();
    workspace.link(&gone).unwrap();
    fs::remove_dir_all(gone.root()).unwrap();

    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    let notes = team.own().directory_for(Kind::Skill).unwrap();
    common::write_files(&notes, &[("notes/SKILL.md", &skill("notes", "Take them"))]);
    workspace.load(&team).unwrap();

    let ledger = workspace.ledger().unwrap();
    ledger
        .installed(
            "api",
            &InstallRecord {
                kind: Kind::Skill,
                name: String::from("review"),
                source_id: None,
                origin: Some(String::from("workspace")),
                content_hash: Some(hash::tree(&library.join("review")).unwrap()),
                targets: [".agents/skills/review", ".claude/skills/review"]
                    .into_iter()
                    .map(|path| Target {
                        agent: None,
                        path: path.to_owned(),
                        content_hash: None,
                    })
                    .collect(),
            },
        )
        .unwrap();
    let url = "https://github.com/acme/skills";
    let source = ledger
        .source_for(SourceKind::Git, url, None, "skills")
        .unwrap();
    let revision = "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3";
    ledger.saw_source(source.id, Some(revision)).unwrap();
    let shelf = format!(".mindflayer/skills/{}/lint", source.directory);
    common::write_files(
        &hub.join(&shelf),
        &[("SKILL.md", &skill("lint", "Tidy it"))],
    );
    ledger
        .record(
            &source,
            Kind::Skill,
            "lint",
            &shelf,
            Some("Tidy it"),
            Some(revision),
        )
        .unwrap();
    ledger
        .log(
            Action::Gather,
            Some(url),
            Outcome::Ok,
            Some("1 added, 0 updated, 0 unchanged, 0 failed"),
        )
        .unwrap();
    drop(ledger);
    at_a_pinned_time(&workspace);
    Fixture { dir, hub }
}

/// Every time the ledger keeps, set to [`AT`].
fn at_a_pinned_time(workspace: &FlayerWorkspace) {
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE artifacts SET gathered_at = {AT}; UPDATE actions SET at = {AT};"
        ))
        .unwrap();
}

/// Inputs naming the workspace at `root`, with the default one in `home`.
fn naming<'a>(root: &'a Path, home: Option<&'a Path>) -> Inputs<'a> {
    Inputs {
        flag: Some(root),
        activated: None,
        start: root,
        home,
        intent: Intent::Read,
    }
}

/// `json` with every spelling of `root` in it as `<root>`, and `id` as
/// `<id>`, so it reads the same wherever the fixture was made.
fn placed(json: &str, root: &Path, id: &str) -> String {
    let escaped = serde_json::to_string(&root.to_string_lossy()).unwrap();
    let escaped = escaped.trim_matches('"');
    let json = json.replace(escaped, "<root>").replace(id, "<id>");
    if cfg!(windows) {
        json.replace("\\\\", "/")
    } else {
        json
    }
}

/// What [`fixture`] serializes to, with its folder as `<root>` and `api`'s
/// id as `<id>`: every name a front end reads, in the order it is written,
/// pinned whole, so a field that goes, moves or changes what it holds shows
/// here before it reaches the panel.
const EXPECTED: &str = r#"{
  "format": 1,
  "workspace": {
    "name": "hub",
    "path": "<root>/hub",
    "reason": "explicit",
    "made": true
  },
  "kinds": [
    "skill",
    "rule"
  ],
  "agents": [
    "claude",
    "codex",
    "opencode"
  ],
  "states": [
    "current",
    "outdated",
    "edited",
    "missing",
    "orphaned",
    "foreign",
    "authored"
  ],
  "healths": [
    "ok",
    "attention",
    "broken",
    "stale"
  ],
  "summary": {
    "projects": 1,
    "stale": 1,
    "held": {
      "total": 3,
      "kinds": [
        {
          "kind": "skill",
          "count": 2
        },
        {
          "kind": "rule",
          "count": 1
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 2
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 1
        }
      ]
    },
    "library": {
      "total": 2,
      "kinds": [
        {
          "kind": "skill",
          "count": 1
        },
        {
          "kind": "rule",
          "count": 1
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 0
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 0
        }
      ]
    },
    "shelf": {
      "total": 1,
      "kinds": [
        {
          "kind": "skill",
          "count": 1
        },
        {
          "kind": "rule",
          "count": 0
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 0
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 0
        }
      ]
    }
  },
  "library": {
    "origin": "workspace",
    "directories": [
      {
        "kind": "skill",
        "path": ".mindflayer/library/skills"
      },
      {
        "kind": "rule",
        "path": ".mindflayer/library/rules"
      }
    ],
    "artifacts": [
      {
        "kind": "skill",
        "name": "review",
        "summary": "Read it",
        "state": null,
        "agents": [],
        "origin": null,
        "targets": [
          ".mindflayer/library/skills/review"
        ],
        "issues": [],
        "unread": null
      },
      {
        "kind": "rule",
        "name": "commits",
        "summary": "Write them well",
        "state": null,
        "agents": [],
        "origin": null,
        "targets": [
          ".mindflayer/library/rules/commits.md"
        ],
        "issues": [],
        "unread": null
      }
    ],
    "templates": [
      {
        "name": "full",
        "kind": "skill",
        "origin": null
      }
    ],
    "failures": []
  },
  "projects": [
    {
      "entry": "api",
      "path": "<root>/hub/api",
      "name": "api",
      "marker": "settings.toml",
      "id": "<id>",
      "agents": [
        "claude",
        "codex"
      ],
      "settings": {
        "agents": [
          "claude",
          "codex"
        ],
        "share": "shared",
        "edit_tracked": false,
        "targets": [
          {
            "kind": "skill",
            "folders": [
              ".claude/skills",
              ".agents/skills"
            ]
          }
        ]
      },
      "stale": null,
      "health": "ok",
      "directories": [
        {
          "kind": "skill",
          "path": "skills"
        },
        {
          "kind": "rule",
          "path": "rules"
        }
      ],
      "artifacts": [
        {
          "kind": "skill",
          "name": "deploy",
          "summary": "Ship it",
          "state": "authored",
          "agents": [],
          "origin": null,
          "targets": [
            "skills/deploy"
          ],
          "issues": [],
          "unread": null
        },
        {
          "kind": "skill",
          "name": "review",
          "summary": "Read it",
          "state": "current",
          "agents": [
            "claude",
            "codex"
          ],
          "origin": "workspace",
          "targets": [
            ".agents/skills/review",
            ".claude/skills/review"
          ],
          "issues": [],
          "unread": null
        },
        {
          "kind": "rule",
          "name": "commits",
          "summary": null,
          "state": "current",
          "agents": [
            "claude",
            "codex"
          ],
          "origin": "workspace",
          "targets": [
            "AGENTS.md"
          ],
          "issues": [],
          "unread": null
        }
      ],
      "tally": {
        "total": 3,
        "kinds": [
          {
            "kind": "skill",
            "count": 2
          },
          {
            "kind": "rule",
            "count": 1
          }
        ],
        "states": [
          {
            "state": "current",
            "count": 2
          },
          {
            "state": "outdated",
            "count": 0
          },
          {
            "state": "edited",
            "count": 0
          },
          {
            "state": "missing",
            "count": 0
          },
          {
            "state": "orphaned",
            "count": 0
          },
          {
            "state": "foreign",
            "count": 0
          },
          {
            "state": "authored",
            "count": 1
          }
        ]
      },
      "templates": [
        {
          "name": "full",
          "kind": "skill",
          "origin": null
        }
      ],
      "unreached": [],
      "missed": [],
      "misplaced": [],
      "failures": []
    },
    {
      "entry": "gone",
      "path": "<root>/hub/gone",
      "name": null,
      "marker": null,
      "id": null,
      "agents": [],
      "settings": null,
      "stale": {
        "code": "not_a_project",
        "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
      },
      "health": "stale",
      "directories": [],
      "artifacts": [],
      "tally": {
        "total": 0,
        "kinds": [
          {
            "kind": "skill",
            "count": 0
          },
          {
            "kind": "rule",
            "count": 0
          }
        ],
        "states": [
          {
            "state": "current",
            "count": 0
          },
          {
            "state": "outdated",
            "count": 0
          },
          {
            "state": "edited",
            "count": 0
          },
          {
            "state": "missing",
            "count": 0
          },
          {
            "state": "orphaned",
            "count": 0
          },
          {
            "state": "foreign",
            "count": 0
          },
          {
            "state": "authored",
            "count": 0
          }
        ]
      },
      "templates": [],
      "unreached": [],
      "missed": [],
      "misplaced": [],
      "failures": []
    }
  ],
  "loaded": [
    {
      "entry": "../team",
      "path": "<root>/team",
      "name": "team",
      "offers": 1,
      "origin": "load:../team",
      "error": null
    }
  ],
  "shelf": [
    {
      "kind": "skill",
      "name": "lint",
      "path": ".mindflayer/skills/github.com-acme-skills/lint",
      "summary": "Tidy it",
      "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
      "gathered_at": 1790000000,
      "source": {
        "kind": "git",
        "url": "https://github.com/acme/skills",
        "reference": null,
        "subdirectory": "skills",
        "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
        "directory": "github.com-acme-skills"
      },
      "origin": "https://github.com/acme/skills"
    }
  ],
  "sources": [
    {
      "kind": "git",
      "url": "https://github.com/acme/skills",
      "reference": null,
      "subdirectory": "skills",
      "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
      "directory": "github.com-acme-skills",
      "origin": "https://github.com/acme/skills"
    }
  ],
  "known": [
    {
      "name": "hub",
      "path": "<root>/hub",
      "default": false,
      "remembered": false,
      "in_use": true,
      "projects": 2,
      "stale_projects": 1,
      "stale": null
    }
  ],
  "history": [
    {
      "at": 1790000000,
      "action": "gather",
      "target": "https://github.com/acme/skills",
      "outcome": "ok",
      "detail": "1 added, 0 updated, 0 unchanged, 0 failed"
    },
    {
      "at": 1790000000,
      "action": "install",
      "target": "api",
      "outcome": "ok",
      "detail": "commits into api"
    }
  ],
  "warnings": []
}"#;

/// A snapshot's names are a contract, as a report's are: every key, and the
/// order each is written in.
#[test]
fn a_snapshot_serializes_every_name_in_order() {
    let fixture = fixture();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();
    let id = api.settings().unwrap().id.to_string();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let json = serde_json::to_string_pretty(&snapshot).unwrap();

    assert_eq!(placed(&json, fixture.dir.path(), &id), EXPECTED);
}

/// Every path under `root`, hidden ones included, with what each file holds.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
                found.insert(path, None);
            } else {
                let bytes = fs::read(&path).unwrap();
                found.insert(path, Some(bytes));
            }
        }
    }
    found
}

/// A front end reads a snapshot at every update, so reading one must leave
/// the disk as it was: no ledger made or rewritten, no scrub, and no default
/// workspace made in a home that has none.
#[test]
fn building_a_snapshot_creates_no_path_and_never_the_home_workspace() {
    let fixture = fixture();
    let home = fixture.dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let before = tree(fixture.dir.path());

    let named = Snapshot::read(&naming(&fixture.hub, Some(&home)))
        .unwrap()
        .unwrap();
    // Nothing named and nothing above: the default one, as it would be,
    // even asked by inputs that mean to write, which a read is not.
    let defaulted = Snapshot::read(&Inputs {
        flag: None,
        activated: None,
        start: &home,
        home: Some(&home),
        intent: Intent::Write,
    })
    .unwrap()
    .unwrap();

    assert_eq!(named.workspace.reason, Reason::Explicit);
    assert_eq!(defaulted.workspace.reason, Reason::Home);
    assert!(!defaulted.workspace.made);
    assert!(!home.join(FLAYER_DIR).exists());
    assert_eq!(tree(fixture.dir.path()), before);
}

/// A row an older build wrote before credentials were refused still holds
/// them until a command that writes opens the ledger; a snapshot is a read,
/// which scrubs nothing, and prints none of them all the same.
#[test]
fn no_url_in_a_snapshot_carries_userinfo_even_from_rows_written_before_the_scrub() {
    let fixture = fixture();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();
    let tainted = "https://me:ghp_t0ken@github.com/acme/skills";
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE sources SET url = '{tainted}';
             UPDATE installations SET origin = '{tainted}';
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES ({AT}, 'gather', '{tainted}', 'failed', '{tainted}: cannot be fetched');"
        ))
        .unwrap();
    let ledger = fs::read(workspace.ledger_path()).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();

    assert!(!json.contains("ghp_t0ken"), "{json}");
    assert!(!json.contains("me:"), "{json}");
    assert!(json.contains("https://github.com/acme/skills"), "{json}");
    assert_eq!(snapshot.history.len(), 3, "{:?}", snapshot.history);
    assert_eq!(
        fs::read(workspace.ledger_path()).unwrap(),
        ledger,
        "a read scrubbed the ledger"
    );
}

/// The project `name` in [`fixture`], as a snapshot shows it.
fn read(fixture: &Fixture, name: &str) -> Project {
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();
    let project = MindProject::open(fixture.hub.join(name)).unwrap();
    Project::read(&project, Some(&workspace))
}

/// The one row `api` in [`fixture`] lists as `deploy`.
fn deploy(fixture: &Fixture) -> Held {
    let project = read(fixture, "api");
    assert!(project.failures.is_empty(), "{:?}", project.failures);
    let deploy: Vec<&Held> = project
        .artifacts
        .iter()
        .filter(|held| held.name == "deploy")
        .collect();
    assert_eq!(deploy.len(), 1, "{deploy:?}");
    deploy[0].clone()
}

/// A file that does not read is listed where it would have been, by the
/// name its place gives it and in place of a copy of that name in an
/// agent's folder, with why it does not read: the one most in need of
/// opening. Where it is is said on its own, so the copy that reads, whose
/// standing the row keeps, is never taken for it. What no artifact can be
/// named by is a failure of its holder.
#[test]
fn a_file_that_does_not_read_is_listed_by_its_place_with_why() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::write(api.join("skills/deploy/SKILL.md"), "no front matter\n").unwrap();
    common::write_files(
        &api,
        &[(
            ".claude/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        )],
    );

    let beside = deploy(&fixture);
    fs::remove_dir_all(api.join(".claude/skills/deploy")).unwrap();
    let alone = deploy(&fixture);

    // Where the copy is, by its own name, and the reason alone.
    assert_eq!(
        serde_json::to_value(&beside.unread).unwrap(),
        serde_json::json!({
            "path": "skills/deploy",
            "code": "front_matter_missing",
            "message": "the file does not start with a `---` front matter fence",
        })
    );
    assert_eq!(beside.summary, None);
    // How the name stands is the copy's that reads, somebody's in Claude's
    // folder, and both are where it is held.
    assert_eq!(beside.state, Some(State::Foreign));
    assert_eq!(beside.agents, [Agent::Claude]);
    assert_eq!(beside.targets, [".claude/skills/deploy", "skills/deploy"]);
    // With no copy that reads, nothing stands, and it is held where it is.
    assert_eq!(
        alone.unread.as_ref().map(|unread| unread.path.as_str()),
        Some("skills/deploy")
    );
    assert_eq!(alone.state, None);
    assert_eq!(alone.agents, []);
    assert_eq!(alone.targets, ["skills/deploy"]);
}

/// An install every copy of which has gone is still listed, missing, with
/// where its copies were put, so a front end can say what to put back.
#[test]
fn an_install_nothing_of_which_is_found_says_where_it_went() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::remove_dir_all(api.join(".agents")).unwrap();
    fs::remove_dir_all(api.join(".claude")).unwrap();

    let project = read(&fixture, "api");

    let review = project
        .artifacts
        .iter()
        .find(|held| held.name == "review")
        .expect("listed");
    assert_eq!(review.state, Some(State::Missing));
    assert_eq!(review.agents, []);
    assert_eq!(review.origin.as_deref(), Some("workspace"));
    assert_eq!(
        review.targets,
        [".agents/skills/review", ".claude/skills/review"]
    );
}

/// A project whose installs are as they were put, beside its own work,
/// asks for nothing, and an entry that opens no project is stale: one word
/// each, which a front end sorts and filters by.
#[test]
fn a_project_with_nothing_to_do_is_ok_and_one_that_opens_none_is_stale() {
    let fixture = fixture();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();

    let healths: Vec<Health> = snapshot
        .projects
        .iter()
        .map(|project| project.health)
        .collect();
    assert_eq!(healths, [Health::Ok, Health::Stale]);
}

/// An edited copy asks for attention, a forced update or an uninstall,
/// and so does what `validate` only warns of; somebody's copy asks for
/// nothing, since it is never Mindflayer's to change.
#[test]
fn an_edited_copy_asks_for_attention_and_somebody_s_asks_for_nothing() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    common::write_files(
        &api,
        &[(
            ".claude/skills/legacy/SKILL.md",
            &skill("legacy", "Kept by hand"),
        )],
    );
    let foreign = read(&fixture, "api");
    fs::write(
        api.join(".claude/skills/review/SKILL.md"),
        skill("review", "Read it twice"),
    )
    .unwrap();
    let edited = read(&fixture, "api");

    assert!(foreign
        .artifacts
        .iter()
        .any(|held| held.name == "legacy" && held.state == Some(State::Foreign)));
    assert_eq!(foreign.health, Health::Ok);
    assert_eq!(edited.health, Health::Attention);
}

/// A file that does not read breaks its project, over anything else it
/// asks for: what to do cannot be told of a file nobody can read.
#[test]
fn a_file_that_does_not_read_breaks_its_project_over_an_edited_copy() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::write(
        api.join(".claude/skills/review/SKILL.md"),
        skill("review", "Read it twice"),
    )
    .unwrap();
    fs::write(api.join("skills/deploy/SKILL.md"), "no front matter\n").unwrap();

    let project = read(&fixture, "api");

    assert_eq!(project.health, Health::Broken);
}

/// The overview's counts are of directories, as `known` counts them: a
/// second spelling of one is an entry of its own, which `unlink` is given,
/// and one project all the same, whose artifacts count once.
#[test]
fn the_summary_counts_each_directory_once_however_many_entries_spell_it() {
    let fixture = fixture();
    let marker = fixture.hub.join(FLAYER_DIR).join("flayer.toml");
    let text = fs::read_to_string(&marker).unwrap();
    assert!(text.contains("\"gone\""), "{text}");
    fs::write(&marker, text.replacen("\"gone\"", "\"gone\", \"./api\"", 1)).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.projects.len(), 3);
    assert_eq!(snapshot.summary.projects, 1);
    assert_eq!(snapshot.summary.stale, 1);
    assert_eq!(snapshot.summary.held.total, 3);
    assert_eq!(
        snapshot.summary.held.total,
        snapshot.projects[0].tally.total
    );
}

/// What a workspace loads is listed on its own as the snapshot lists it,
/// for a front end showing a project that workspace does not link.
#[test]
fn what_a_workspace_loads_is_listed_on_its_own_as_the_snapshot_lists_it() {
    let fixture = fixture();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();

    assert_eq!(LoadedWorkspace::of(&workspace), snapshot.loaded);
    assert_eq!(snapshot.loaded[0].offers, 1);
}

/// Each origin a snapshot lists beside what is offered is what `--from`
/// takes for it: given back to an install, it names that one offer, which
/// says it with the same words. So a front end that prints it beside a
/// command prints a line that runs.
#[test]
fn every_origin_a_snapshot_lists_names_its_offer_to_from() {
    let fixture = fixture();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();
    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let offered = |name: &str, origin: &str| {
        install::offered(&workspace, &ledger, &api, Kind::Skill, name, Some(origin))
            .unwrap_or_else(|error| panic!("{name} from {origin}: {error}"))
            .origin()
    };

    let [shelved] = snapshot.shelf.as_slice() else {
        panic!("{:?}", snapshot.shelf)
    };
    assert_eq!(
        offered(&shelved.gathered.name, &shelved.origin),
        shelved.origin
    );
    assert_eq!(snapshot.sources[0].origin, shelved.origin);
    assert_eq!(
        offered("review", snapshot.library.origin),
        snapshot.library.origin
    );
    let [team] = snapshot.loaded.as_slice() else {
        panic!("{:?}", snapshot.loaded)
    };
    assert_eq!(offered("notes", &team.origin), team.origin);
}

/// A second spelling of one loaded workspace offers what the first does,
/// under the first's origin, so it lists that origin too: its own would
/// name nothing an install could find.
#[test]
fn a_second_spelling_of_a_load_lists_the_origin_its_offers_go_by() {
    let fixture = fixture();
    let marker = fixture.hub.join(FLAYER_DIR).join("flayer.toml");
    let text = fs::read_to_string(&marker).unwrap();
    assert!(text.contains("\"../team\""), "{text}");
    fs::write(
        &marker,
        text.replacen("\"../team\"", "\"../team\", \"./../team\"", 1),
    )
    .unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();

    let origins: Vec<(&str, &str, usize)> = snapshot
        .loaded
        .iter()
        .map(|loaded| (loaded.entry.as_str(), loaded.origin.as_str(), loaded.offers))
        .collect();
    assert_eq!(
        origins,
        [
            ("../team", "load:../team", 1),
            ("./../team", "load:../team", 1)
        ]
    );
}

/// A listing of every kind is the snapshot's holders, as they are: what a
/// script prints of a project is what a front end draws of it.
#[test]
fn a_listing_of_every_kind_holds_what_the_snapshot_holds() {
    let fixture = fixture();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let listing = Listing::of_workspace(&workspace, &Kind::ALL);
    let one = Listing::of_project(&api, Some(&workspace), &Kind::ALL);

    assert_eq!(listing.library.as_ref(), Some(&snapshot.library));
    assert_eq!(listing.projects, snapshot.projects);
    assert_eq!(one.library, None);
    assert_eq!(one.projects, [read(&fixture, "api")]);
}

/// A listing of one kind reads that kind alone: each holder's rows of it,
/// as the snapshot has them, and its folders and templates of it.
#[test]
fn a_listing_of_one_kind_holds_only_that_kind() {
    let fixture = fixture();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let rules = Listing::of_workspace(&workspace, &[Kind::Rule]);

    let library = rules.library.unwrap();
    assert_eq!(names(&library.artifacts), ["commits"]);
    assert!(library
        .directories
        .iter()
        .all(|folder| folder.kind == Kind::Rule));
    assert!(library
        .templates
        .iter()
        .all(|template| template.kind == Kind::Rule));
    assert_eq!(rules.projects.len(), snapshot.projects.len());
    for (listed, whole) in rules.projects.iter().zip(&snapshot.projects) {
        let of_rules: Vec<Held> = whole
            .artifacts
            .iter()
            .filter(|held| held.kind == Kind::Rule)
            .cloned()
            .collect();
        assert_eq!(listed.artifacts, of_rules);
        assert_eq!(listed.stale, whole.stale);
        assert!(listed
            .directories
            .iter()
            .all(|folder| folder.kind == Kind::Rule));
        // Counted for the kind read, which is the only one a count is of.
        assert_eq!(listed.tally.total, of_rules.len());
        assert_eq!(
            listed
                .tally
                .kinds
                .iter()
                .map(|counted| (counted.kind, counted.count))
                .collect::<Vec<_>>(),
            [(Kind::Rule, of_rules.len())]
        );
    }
    assert_eq!(names(&rules.projects[0].artifacts), ["commits"]);
}

/// A listing of one kind tells each project's health from what it read, as
/// it counts the tally from it: a skill that does not read breaks the
/// project the snapshot holds, and is nothing to a listing of its rules.
#[test]
fn a_listing_of_one_kind_tells_health_from_that_kind_alone() {
    let fixture = fixture();
    fs::write(
        fixture.hub.join("api/skills/deploy/SKILL.md"),
        "no front matter\n",
    )
    .unwrap();
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None))
        .unwrap()
        .unwrap();
    let skills = Listing::of_workspace(&workspace, &[Kind::Skill]);
    let rules = Listing::of_workspace(&workspace, &[Kind::Rule]);

    assert_eq!(snapshot.projects[0].name.as_deref(), Some("api"));
    assert_eq!(snapshot.projects[0].health, Health::Broken);
    assert_eq!(skills.projects[0].health, Health::Broken);
    assert_eq!(rules.projects[0].health, Health::Ok);
}

/// The name of each row, in order.
fn names(held: &[Held]) -> Vec<&str> {
    held.iter().map(|held| held.name.as_str()).collect()
}
