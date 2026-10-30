//! What a front end watches to know a snapshot is out of date (#74), against
//! a real workspace on disk: every path a snapshot reads that somebody can
//! change, each named on its own, never a tree.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mindflayer_core::resolve::{Reason, Resolved};
use mindflayer_core::view::Snapshot;
use mindflayer_core::watch::{self, Scope, Watched};
use mindflayer_core::{
    claude_rules, Agent, Catalog, Directories, FlayerWorkspace, Kind, MindProject, Share,
    FLAYER_DIR, LEDGER_FILE,
};
use tempfile::TempDir;

use common::skill;

/// A workspace `hub` in a temporary folder, beside a `home` and a `team`:
///
/// - its library holds `review`, which keeps scripts in a folder of its own;
/// - it links `api`, marked by its settings for Claude and Codex, which
///   authors `deploy` and a rule filed under `team/`, holds a copy of
///   `review` in Claude's folder, and is a repository with everything a
///   repository has beside: `.git`, `node_modules`, `target`, its code;
/// - it links `gone`, whose directory has gone since;
/// - it loads `team`, which offers `notes`.
struct Fixture {
    dir: TempDir,
    hub: FlayerWorkspace,
    api: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let (mut hub, _) = FlayerWorkspace::init(dir.path().join("hub")).unwrap();
        let library = hub.own().directory_for(Kind::Skill).unwrap();
        common::write_files(
            &library,
            &[
                ("review/SKILL.md", &skill("review", "Read it")),
                ("review/scripts/deep/check.sh", "true\n"),
            ],
        );

        common::write_files(
            &hub.flayer_dir().join("templates"),
            &[
                ("release/SKILL.md", &skill("release", "Cut one")),
                ("release/assets/notes.md", "Notes.\n"),
            ],
        );

        let directories = Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules");
        let (api, _) = MindProject::init_settings(
            hub.root().join("api"),
            &[Agent::Claude, Agent::Codex],
            Share::Shared,
            &directories,
        )
        .unwrap();
        hub.link(&api).unwrap();
        common::write_files(
            api.root(),
            &[
                ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
                ("skills/deploy/scripts/deep/run.sh", "true\n"),
                ("rules/team/style.md", "Name things well.\n"),
                (
                    ".claude/skills/review/SKILL.md",
                    &skill("review", "Read it"),
                ),
                ("AGENTS.md", "# Working here\n"),
                (".git/objects/aa/bb", "blob\n"),
                ("node_modules/left-pad/index.js", "module.exports = 0\n"),
                ("target/debug/api", "binary\n"),
                ("src/main.rs", "fn main() {}\n"),
                ("README.md", "# api\n"),
            ],
        );

        let (gone, _) = MindProject::init(hub.root().join("gone")).unwrap();
        hub.link(&gone).unwrap();
        fs::remove_dir_all(gone.root()).unwrap();

        let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
        let notes = team.own().directory_for(Kind::Skill).unwrap();
        common::write_files(&notes, &[("notes/SKILL.md", &skill("notes", "Take them"))]);
        hub.load(&team).unwrap();

        let hub = FlayerWorkspace::open(hub.root()).unwrap();
        let api = api.root().to_path_buf();
        Fixture { dir, hub, api }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn team(&self) -> FlayerWorkspace {
        FlayerWorkspace::open(self.dir.path().join("team")).unwrap()
    }

    fn watched(&self) -> Vec<Watched> {
        watch::watched(&self.hub, Some(&self.home()))
    }
}

fn of(scope: &Scope, path: impl Into<PathBuf>) -> Watched {
    Watched {
        path: path.into(),
        scope: scope.clone(),
    }
}

#[test]
fn the_workspace_is_watched_by_its_marker_its_ledger_its_library_its_shelf_and_its_loads() {
    let fixture = Fixture::new();
    let hub = fixture.hub.root();
    let library = fixture.hub.own().directory_for(Kind::Skill).unwrap();
    let team = fixture.team();
    let offered = team.own().directory_for(Kind::Skill).unwrap();

    let watched = fixture.watched();

    let workspace = Scope::Workspace;
    for expected in [
        of(&workspace, hub.join(FLAYER_DIR).join("flayer.toml")),
        of(&workspace, hub.join(FLAYER_DIR).join(LEDGER_FILE)),
        of(&workspace, fixture.hub.gathered_dir(Kind::Skill)),
        of(&workspace, library.clone()),
        of(&workspace, library.join("review")),
        of(&workspace, hub.join(FLAYER_DIR).join("templates")),
        of(
            &workspace,
            hub.join(FLAYER_DIR).join("templates").join("release"),
        ),
        of(&workspace, team.root().join(FLAYER_DIR).join("flayer.toml")),
        of(&workspace, offered.join("notes")),
        // Not made: the default workspace's marker, for when it is.
        of(
            &workspace,
            fixture.home().join(FLAYER_DIR).join("flayer.toml"),
        ),
    ] {
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
}

#[test]
fn a_project_is_watched_by_its_marker_what_its_agents_read_and_where_it_keeps_each_kind() {
    let fixture = Fixture::new();
    let api = &fixture.api;

    let watched = fixture.watched();

    let project = Scope::Project(String::from("api"));
    for expected in [
        of(&project, api.join(".mind").join("settings.toml")),
        of(&project, api.join("AGENTS.md")),
        // Named before they are there, for when they are.
        of(&project, api.join("CLAUDE.md")),
        of(&project, api.join(".mcp.json")),
        of(&project, api.join("opencode.json")),
        of(&project, api.join(".codex").join("config.toml")),
        of(&project, api.join("skills")),
        of(&project, api.join("skills").join("deploy")),
        of(&project, api.join("rules")),
        of(&project, api.join("rules").join("team")),
        of(&project, api.join(".claude").join("skills")),
        of(&project, api.join(".claude").join("skills").join("review")),
        of(&project, api.join(".agents").join("skills")),
        of(&project, api.join(claude_rules::folder())),
        of(&project, api.join(".mind").join("templates")),
    ] {
        assert!(watched.contains(&expected), "{expected:?} in {watched:#?}");
    }
    // An entry that opens no project is watched for its marker coming back.
    let gone = Scope::Project(String::from("gone"));
    let marker = fixture.hub.root().join("gone").join(".mind");
    assert!(watched.contains(&of(&gone, marker.join("settings.toml"))));
    assert!(watched.contains(&of(&gone, marker.join("mind.toml"))));
}

/// The acceptance's own: a folder is watched for what is directly in it, so
/// no folder named may be a root whose every file is somebody's code, and
/// none may sit deeper than discovery looks.
#[test]
fn no_path_named_covers_a_whole_tree() {
    let fixture = Fixture::new();
    let hub = fixture.hub.root().to_path_buf();
    let api = fixture.api.clone();
    let library = fixture.hub.own().directory_for(Kind::Skill).unwrap();

    let watched = fixture.watched();
    let paths: Vec<&Path> = watched.iter().map(|each| each.path.as_path()).collect();

    for whole in [
        fixture.dir.path().to_path_buf(),
        hub.clone(),
        hub.join(FLAYER_DIR),
        fixture.hub.cache_dir(),
        api.clone(),
        api.join(".mind"),
        api.join(".claude"),
        api.join("src"),
        api.join("skills").join("deploy").join("scripts"),
        library.join("review").join("scripts"),
        hub.join(FLAYER_DIR)
            .join("templates")
            .join("release")
            .join("assets"),
    ] {
        assert!(!paths.contains(&whole.as_path()), "{whole:?} is named");
    }
    for path in &paths {
        for part in [".git", "node_modules", "target", "scripts"] {
            assert!(
                !path.components().any(|each| each.as_os_str() == part),
                "{path:?} is inside a {part}"
            );
        }
    }

    // Every folder named is one a holder keeps a kind in, or the shelf's or
    // the templates', and below a folder of skills, only each skill's own.
    let holders = [fixture.hub.own(), MindProject::open(&api).unwrap()];
    let mut roots: Vec<(PathBuf, Option<Kind>)> = Vec::new();
    for holder in holders.iter().chain([&fixture.team().own()]) {
        for kind in Kind::ALL {
            for folder in Catalog::folders(holder, kind) {
                roots.push((folder, Some(kind)));
            }
        }
    }
    roots.push((api.join(claude_rules::folder()), Some(Kind::Rule)));
    for kind in Kind::ALL {
        roots.push((fixture.hub.gathered_dir(kind), None));
    }
    roots.push((hub.join(FLAYER_DIR).join("templates"), Some(Kind::Skill)));
    roots.push((api.join(".mind").join("templates"), Some(Kind::Skill)));
    roots.push((hub.join(".mind").join("templates"), Some(Kind::Skill)));
    for path in paths.iter().filter(|path| path.is_dir()) {
        let under = roots.iter().find(|(root, kind)| match kind {
            Some(Kind::Skill) => *path == root || path.parent() == Some(root.as_path()),
            Some(Kind::Rule) => path.starts_with(root),
            None => *path == root,
        });
        assert!(under.is_some(), "{path:?} is no folder discovery looks in");
    }
}

/// A name, and the size and time of what it names: what a watch sees change.
type Look = (String, u64, Option<SystemTime>);

/// What each path named holds, as far as a watch would see a change to it:
/// a file's size and time, and for a folder, each name directly in it, with
/// the same of each.
fn seen(watched: &[Watched]) -> Vec<(PathBuf, Vec<Look>)> {
    let look = |path: &Path| {
        let metadata = fs::metadata(path).ok();
        (
            metadata.as_ref().map_or(0, fs::Metadata::len),
            metadata.and_then(|metadata| metadata.modified().ok()),
        )
    };
    watched
        .iter()
        .map(|each| {
            let mut found = vec![{
                let (len, modified) = look(&each.path);
                (String::new(), len, modified)
            }];
            if each.path.is_dir() {
                let mut inside: Vec<Look> = fs::read_dir(&each.path)
                    .unwrap()
                    .map(|entry| {
                        let entry = entry.unwrap();
                        let (len, modified) = look(&entry.path());
                        (
                            entry.file_name().to_string_lossy().into_owned(),
                            len,
                            modified,
                        )
                    })
                    .collect();
                inside.sort();
                found.extend(inside);
            }
            (each.path.clone(), found)
        })
        .collect()
}

/// A snapshot changing something watched would set off another snapshot,
/// and that one another, for as long as the panel is open. SQLite makes its
/// log and shared memory beside the ledger to read it and takes them away
/// as the last connection closes, which a watch on the ledger's folder
/// would see however quickly they go: so only the file is named, and with
/// nothing in the log, closing writes nothing to it.
#[test]
fn reading_a_snapshot_changes_nothing_watched() {
    let fixture = Fixture::new();
    // A ledger that is there, as a write leaves one, its log checkpointed.
    fixture.hub.ledger().unwrap();
    let watched = fixture.watched();
    let before = seen(&watched);

    let resolved = Resolved {
        workspace: FlayerWorkspace::open(fixture.hub.root()).unwrap(),
        reason: Reason::Explicit,
    };
    Snapshot::of(&resolved, Some(&fixture.home()));

    assert_eq!(seen(&watched), before);
    let ledger = fixture.hub.root().join(FLAYER_DIR).join(LEDGER_FILE);
    let beside = ledger.parent().unwrap();
    assert!(watched.iter().any(|each| each.path == ledger));
    assert!(!watched.iter().any(|each| each.path == beside));
    for log in ["-wal", "-shm"] {
        let mut name = ledger.clone().into_os_string();
        name.push(log);
        assert!(!watched.iter().any(|each| each.path.as_os_str() == name));
    }
}

#[test]
fn a_project_linked_since_is_watched_once_the_list_is_asked_again() {
    let fixture = Fixture::new();
    let mut hub = FlayerWorkspace::open(fixture.hub.root()).unwrap();
    let before = watch::watched(&hub, None);
    let (web, _) = MindProject::init(fixture.dir.path().join("web")).unwrap();
    hub.link(&web).unwrap();

    let after = watch::watched(&FlayerWorkspace::open(hub.root()).unwrap(), None);

    let scope = Scope::Project(String::from("../web"));
    assert!(!before.iter().any(|each| each.scope == scope));
    let marker = web.root().join(".mind").join("mind.toml");
    assert!(after.contains(&of(&scope, marker)));
}
