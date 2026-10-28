//! The snapshot every front end reads, against a real workspace on disk:
//! what it says of a project, as it will of every project a workspace
//! links.

mod common;

use std::fs;
use std::path::PathBuf;

use mindflayer_core::ledger::{Action, InstallRecord, Outcome, Target};
use mindflayer_core::view::Project;
use mindflayer_core::{
    hash, Agent, Directories, FlayerWorkspace, Kind, MindProject, Share, SourceKind, State,
};
use tempfile::TempDir;

use common::skill;

/// Seconds since the epoch every time in the fixture is pinned to, so what
/// the ledger says reads the same on every run.
const AT: i64 = 1_790_000_000;

/// A workspace `hub` in a temporary folder, as a front end finds one:
///
/// - its library holds `review`, which it offers;
/// - it links `api`, marked by its settings for Claude and Codex and shared,
///   which authors `deploy` and received `review` from the library, both
///   copies there as they were written;
/// - it links `gone`, whose directory has gone since;
/// - it loads `team`, beside it, which offers `notes`;
/// - its shelf holds `lint`, gathered from one source, and its log one
///   gather.
struct Fixture {
    _dir: TempDir,
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
    workspace.link(&api).unwrap();
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
    Fixture { _dir: dir, hub }
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

/// The project `name` in [`fixture`], as a snapshot shows it.
fn read(fixture: &Fixture, name: &str) -> Project {
    let workspace = FlayerWorkspace::open(&fixture.hub).unwrap();
    let project = MindProject::open(fixture.hub.join(name)).unwrap();
    Project::read(&project, Some(&workspace))
}

/// A file that does not read is listed where it would have been, by the
/// name its place gives it and in place of a copy of that name in an
/// agent's folder, with why it does not read: the one most in need of
/// opening. What no artifact can be named by is a failure of its holder.
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

    let project = read(&fixture, "api");

    let deploy: Vec<_> = project
        .artifacts
        .iter()
        .filter(|held| held.name == "deploy")
        .collect();
    assert_eq!(deploy.len(), 1, "{deploy:?}");
    let unread = deploy[0].unread.as_ref().expect("said why");
    assert!(!unread.message.contains("SKILL.md"), "{}", unread.message);
    assert_eq!(deploy[0].summary, None);
    assert!(project.failures.is_empty(), "{:?}", project.failures);
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
