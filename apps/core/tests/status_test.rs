//! What a project holds, from where and for which agents, against real
//! projects on disk and the ledger of the workspace that links them.

use std::fs;
use std::path::PathBuf;

use mindflayer_core::ledger::{InstallRecord, Target};
use mindflayer_core::{
    Agent, ArtifactStatus, Directories, FlayerWorkspace, Kind, Ledger, MindProject, ProjectStatus,
    Share, State, Status, StatusFailure,
};
use tempfile::TempDir;

/// A workspace in a temporary folder.
fn workspace() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    (dir, workspace)
}

/// `name` beside the workspace, marked by its settings: naming `agents`,
/// authoring skills in `skills` and rules in `rules`, and linked.
fn marked(
    dir: &TempDir,
    workspace: &mut FlayerWorkspace,
    name: &str,
    agents: &[Agent],
) -> MindProject {
    let root = dir.path().join(name);
    fs::create_dir_all(&root).unwrap();
    let directories = Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules");
    let project = MindProject::init_settings(&root, agents, Share::Shared, &directories)
        .unwrap()
        .0;
    workspace.link(&project).unwrap();
    project
}

/// Write `name`'s SKILL.md in `folder`, a path below the project.
fn put(project: &MindProject, folder: &str, name: &str) -> PathBuf {
    let dir = project.root().join(folder).join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Does {name} things\n---\n\n# {name}\n"),
    )
    .unwrap();
    dir
}

/// Record in `ledger` that `name` came from `origin` into the project the
/// workspace stores as `project`, with a copy at each of `paths`.
fn record(ledger: &Ledger, project: &str, name: &str, origin: &str, paths: &[&str]) {
    ledger
        .installed(
            project,
            &InstallRecord {
                kind: Kind::Skill,
                name: name.to_owned(),
                source_id: None,
                origin: Some(origin.to_owned()),
                content_hash: None,
                targets: paths
                    .iter()
                    .map(|path| Target {
                        agent: None,
                        path: (*path).to_owned(),
                    })
                    .collect(),
            },
        )
        .unwrap();
}

/// How each artifact stands, as `(name, state, agents, origin)`.
fn states(project: &ProjectStatus) -> Vec<(&str, State, &[Agent], Option<&str>)> {
    project
        .artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.name.as_str(),
                artifact.state,
                artifact.agents.as_slice(),
                artifact.origin.as_deref(),
            )
        })
        .collect()
}

#[test]
fn each_artifact_is_installed_partial_foreign_or_authored_with_the_agents_it_reaches() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    // Both copies there, both recorded.
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, ".claude/skills", "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    // Claude's copy deleted since.
    put(&alpha, ".agents/skills", "audit");
    record(
        &ledger,
        "alpha",
        "audit",
        "load:../team",
        &[".agents/skills/audit", ".claude/skills/audit"],
    );
    // Every copy deleted since.
    record(
        &ledger,
        "alpha",
        "gone",
        "https://github.com/acme/skills",
        &[".agents/skills/gone", ".claude/skills/gone"],
    );
    // Somebody's, in Claude's folder, which OpenCode reads too.
    put(&alpha, ".claude/skills", "notes");
    // The project's own, where no agent reads.
    put(&alpha, "skills", "review");
    fs::create_dir_all(alpha.root().join("rules")).unwrap();
    fs::write(alpha.root().join("rules/style.md"), "# Say why\n").unwrap();

    let (status, failures) = Status::of_workspace(&workspace, &ledger);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(status.projects.len(), 1);
    let project = &status.projects[0];
    assert_eq!(project.name.as_deref(), Some("alpha"));
    assert_eq!(project.entry.as_deref(), Some("alpha"));
    assert_eq!(project.path, alpha.root());
    assert_eq!(project.agents, Agent::ALL);
    assert_eq!(project.stale, None);
    assert_eq!(
        states(project),
        [
            (
                "audit",
                State::Partial,
                &[Agent::Codex, Agent::OpenCode][..],
                Some("load:../team")
            ),
            (
                "deploy",
                State::Installed,
                &Agent::ALL[..],
                Some("workspace")
            ),
            (
                "gone",
                State::Partial,
                &[][..],
                Some("https://github.com/acme/skills")
            ),
            (
                "notes",
                State::Foreign,
                &[Agent::Claude, Agent::OpenCode][..],
                None
            ),
            ("review", State::Authored, &[][..], None),
            ("style", State::Authored, &[][..], None),
        ]
    );
    assert_eq!(project.artifacts[5].kind, Kind::Rule);
}

#[test]
fn a_project_a_mind_toml_marks_is_read_from_its_one_folder_and_names_no_agent() {
    let (dir, mut workspace) = workspace();
    let root = dir.path().join("beta");
    fs::create_dir_all(&root).unwrap();
    let beta = MindProject::init(&root).unwrap().0;
    workspace.link(&beta).unwrap();
    let ledger = workspace.ledger().unwrap();
    put(&beta, "skills", "deploy");
    record(&ledger, "beta", "deploy", "workspace", &["skills/deploy"]);
    put(&beta, "skills", "review");

    let (status, failures) = Status::of_workspace(&workspace, &ledger);

    assert!(failures.is_empty(), "{failures:?}");
    let project = &status.projects[0];
    assert!(project.agents.is_empty());
    assert_eq!(
        states(project),
        [
            ("deploy", State::Installed, &[][..], Some("workspace")),
            ("review", State::Authored, &[][..], None),
        ]
    );
}

#[test]
fn an_install_gone_under_a_name_somebody_keeps_is_that_names_one_entry_as_what_is_there() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy"],
    );
    put(&alpha, ".claude/skills", "deploy");

    let (status, _) = Status::of_workspace(&workspace, &ledger);

    assert_eq!(
        status.projects[0].artifacts,
        [ArtifactStatus {
            kind: Kind::Skill,
            name: String::from("deploy"),
            state: State::Foreign,
            agents: vec![Agent::Claude, Agent::OpenCode],
            origin: None,
        }]
    );
}

#[test]
fn a_copy_gone_from_an_install_beside_the_projects_own_is_partial() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    put(&alpha, "skills", "deploy");
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, ".claude/skills", "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    let whole = Status::of_workspace(&workspace, &ledger).0;
    fs::remove_dir_all(alpha.root().join(".claude/skills/deploy")).unwrap();

    let (status, failures) = Status::of_workspace(&workspace, &ledger);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        states(&whole.projects[0]),
        [(
            "deploy",
            State::Authored,
            &Agent::ALL[..],
            Some("workspace")
        )]
    );
    assert_eq!(
        states(&status.projects[0]),
        [(
            "deploy",
            State::Partial,
            &[Agent::Codex, Agent::OpenCode][..],
            Some("workspace")
        )]
    );
}

#[test]
fn a_stale_entry_is_reported_as_stale_and_the_rest_still_are() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let old = marked(&dir, &mut workspace, "old-api", &Agent::ALL);
    put(&alpha, "skills", "review");
    fs::remove_dir_all(old.root()).unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();

    let (status, failures) = Status::of_workspace(&workspace, &ledger);

    assert_eq!(status.projects.len(), 2);
    assert_eq!(
        states(&status.projects[0]),
        [("review", State::Authored, &[][..], None)]
    );
    let stale = &status.projects[1];
    assert_eq!(stale.name, None);
    assert_eq!(stale.entry.as_deref(), Some("old-api"));
    assert_eq!(stale.path, dir.path().join("old-api"));
    assert!(stale.artifacts.is_empty());
    assert_eq!(failures.len(), 1);
    assert!(matches!(failures[0], StatusFailure::Stale(_)));
    assert_eq!(
        stale.stale.as_deref(),
        Some(failures[0].to_string().as_str())
    );
}

#[test]
fn two_entries_for_one_project_report_it_once() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    put(&alpha, "skills", "review");
    let marker = dir.path().join(".mindflayer/flayer.toml");
    let text = fs::read_to_string(&marker).unwrap();
    fs::write(&marker, text.replace("\"alpha\"", "\"alpha\", \"./alpha\"")).unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    assert_eq!(workspace.config().projects.len(), 2);
    let ledger = workspace.ledger_for_reading().unwrap();

    let (status, _) = Status::of_workspace(&workspace, &ledger);

    assert_eq!(status.projects.len(), 1);
    assert_eq!(status.projects[0].entry.as_deref(), Some("alpha"));
}

#[test]
fn a_project_the_workspace_does_not_link_reads_as_nothing_installed() {
    let (dir, workspace) = workspace();
    let root = dir.path().join("loose");
    fs::create_dir_all(&root).unwrap();
    let loose =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
            .unwrap()
            .0;
    put(&loose, ".claude/skills", "deploy");
    let ledger = workspace.ledger().unwrap();
    // Recorded under the route it would have, but by a workspace that does
    // not link it: not this workspace's to say.
    record(
        &ledger,
        "loose",
        "deploy",
        "workspace",
        &[".claude/skills/deploy"],
    );

    let (linked, _) = Status::of_project(&loose, Some((&workspace, &ledger)));
    let (alone, _) = Status::of_project(&loose, None);

    for status in [&linked, &alone] {
        let project = &status.projects[0];
        assert_eq!(project.entry, None);
        assert_eq!(
            states(project),
            [(
                "deploy",
                State::Foreign,
                &[Agent::Claude, Agent::OpenCode][..],
                None
            )]
        );
    }
}

#[test]
fn a_ledger_that_cannot_be_read_leaves_every_copy_unrecorded_and_is_said_once() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    put(&alpha, ".agents/skills", "deploy");
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy"],
    );
    let path = workspace.ledger_path();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE installation_targets;")
        .unwrap();

    let (status, failures) = Status::of_workspace(&workspace, &ledger);

    assert_eq!(
        states(&status.projects[0]),
        [(
            "deploy",
            State::Foreign,
            &[Agent::Codex, Agent::OpenCode][..],
            None
        )]
    );
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0]
        .to_string()
        .contains(&path.display().to_string()));
}

#[test]
fn reading_a_project_hands_back_the_catalog_it_read_with_the_records_marked() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, "skills", "review");
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy"],
    );

    let (status, catalog) =
        ProjectStatus::read(&alpha, &[Kind::Skill], Some((&workspace, &ledger)));

    let names: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|artifact| artifact.name())
        .collect();
    assert_eq!(names, ["deploy", "review"]);
    assert_eq!(
        status
            .find(Kind::Skill, "deploy")
            .map(|artifact| artifact.state),
        Some(State::Installed)
    );
    assert_eq!(status.find(Kind::Rule, "deploy"), None);
    assert!(catalog.artifacts()[0].copies()[0].is_installed());
}
