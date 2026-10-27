//! What a project holds, from where and for which agents, against real
//! projects on disk and the ledger of the workspace that links them.

use std::fs;
use std::path::PathBuf;

use mindflayer_core::ledger::{InstallRecord, Target};
use mindflayer_core::{
    hash, Agent, ArtifactStatus, Directories, FlayerWorkspace, Kind, Ledger, MarkerFile,
    MindProject, ProjectStatus, Share, State, Status, StatusFailure, Unreached, MIND_DIR,
    SETTINGS_JSON_FILE,
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
    write_skill(&dir, name);
    dir
}

/// Write `name`'s SKILL.md into the folder `dir`, as [`put`] does.
fn write_skill(dir: &std::path::Path, name: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Does {name} things\n---\n\n# {name}\n"),
    )
    .unwrap();
}

/// Offer `name` among the workspace's own, holding what [`put`] writes: the
/// place a record from `workspace` came from, offering what its copies hold.
fn offer(workspace: &FlayerWorkspace, name: &str) -> PathBuf {
    let dir = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join(name);
    write_skill(&dir, name);
    dir
}

/// The hash of `name` as [`put`] writes it, which is what an install of it
/// records: a copy that holds it reads as untouched.
fn written(name: &str) -> String {
    let dir = TempDir::new().unwrap();
    write_skill(&dir.path().join(name), name);
    hash::tree(&dir.path().join(name)).unwrap()
}

/// Record in `ledger` that `name` came from `origin` into the project the
/// workspace stores as `project`, with a copy at each of `paths` holding
/// what [`put`] writes.
fn record(ledger: &Ledger, project: &str, name: &str, origin: &str, paths: &[&str]) {
    ledger
        .installed(
            project,
            &InstallRecord {
                kind: Kind::Skill,
                name: name.to_owned(),
                source_id: None,
                origin: Some(origin.to_owned()),
                content_hash: Some(written(name)),
                targets: paths
                    .iter()
                    .map(|path| Target {
                        agent: None,
                        path: (*path).to_owned(),
                        content_hash: None,
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
fn each_artifact_is_current_outdated_missing_orphaned_foreign_or_authored_with_the_agents_it_reaches(
) {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    // Both copies there, both recorded, and offered as they hold it.
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, ".claude/skills", "deploy");
    offer(&workspace, "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    // The same, but offered changed since.
    put(&alpha, ".agents/skills", "ship");
    put(&alpha, ".claude/skills", "ship");
    let ship = offer(&workspace, "ship");
    fs::write(ship.join("notes.md"), "Since.\n").unwrap();
    record(
        &ledger,
        "alpha",
        "ship",
        "workspace",
        &[".agents/skills/ship", ".claude/skills/ship"],
    );
    // Claude's copy deleted since.
    put(&alpha, ".agents/skills", "audit");
    offer(&workspace, "audit");
    record(
        &ledger,
        "alpha",
        "audit",
        "workspace",
        &[".agents/skills/audit", ".claude/skills/audit"],
    );
    // Every copy deleted since, from a source nothing offers it from now.
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

    let (status, failures) = Status::of_workspace(&workspace);

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
                State::Missing,
                &[Agent::Codex, Agent::OpenCode][..],
                Some("workspace")
            ),
            ("deploy", State::Current, &Agent::ALL[..], Some("workspace")),
            (
                "gone",
                State::Orphaned,
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
            ("ship", State::Outdated, &Agent::ALL[..], Some("workspace")),
            ("style", State::Authored, &[][..], None),
        ]
    );
    assert_eq!(project.artifacts[6].kind, Kind::Rule);
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
    offer(&workspace, "deploy");
    record(&ledger, "beta", "deploy", "workspace", &["skills/deploy"]);
    put(&beta, "skills", "review");

    let (status, failures) = Status::of_workspace(&workspace);

    assert!(failures.is_empty(), "{failures:?}");
    let project = &status.projects[0];
    assert!(project.agents.is_empty());
    assert_eq!(
        states(project),
        [
            ("deploy", State::Current, &[][..], Some("workspace")),
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

    let (status, _) = Status::of_workspace(&workspace);

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
fn an_install_a_copy_of_which_was_changed_since_is_edited_even_beside_one_gone() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    put(&alpha, ".agents/skills", "deploy");
    let claude = put(&alpha, ".claude/skills", "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    fs::write(claude.join("notes.md"), "Ours.\n").unwrap();

    let edited = Status::of_workspace(&workspace).0;
    fs::remove_dir_all(alpha.root().join(".agents/skills/deploy")).unwrap();
    let gone_too = Status::of_workspace(&workspace).0;

    assert_eq!(
        states(&edited.projects[0]),
        [("deploy", State::Edited, &Agent::ALL[..], Some("workspace"))]
    );
    // The next install is refused over the edit unless forced, so missing,
    // which promises an update puts the copy back, would say too much.
    assert_eq!(
        states(&gone_too.projects[0]),
        [(
            "deploy",
            State::Edited,
            &[Agent::Claude, Agent::OpenCode][..],
            Some("workspace")
        )]
    );
}

#[test]
fn a_copy_gone_from_an_install_beside_the_projects_own_is_missing() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    put(&alpha, "skills", "deploy");
    offer(&workspace, "deploy");
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, ".claude/skills", "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    let whole = Status::of_workspace(&workspace).0;
    fs::remove_dir_all(alpha.root().join(".claude/skills/deploy")).unwrap();

    let (status, failures) = Status::of_workspace(&workspace);

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
            State::Missing,
            &[Agent::Codex, Agent::OpenCode][..],
            Some("workspace")
        )]
    );
}

#[test]
fn a_copy_gone_from_an_install_nothing_offers_any_more_is_orphaned() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let ledger = workspace.ledger().unwrap();
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, ".claude/skills", "deploy");
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    let whole = Status::of_workspace(&workspace).0;
    fs::remove_dir_all(alpha.root().join(".claude/skills/deploy")).unwrap();

    let (status, failures) = Status::of_workspace(&workspace);

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        states(&whole.projects[0]),
        [(
            "deploy",
            State::Orphaned,
            &Agent::ALL[..],
            Some("workspace")
        )]
    );
    // Nothing can put the copy back, so missing would promise too much.
    assert_eq!(
        states(&status.projects[0]),
        [(
            "deploy",
            State::Orphaned,
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

    let (status, failures) = Status::of_workspace(&workspace);

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
fn each_project_says_the_file_its_marker_is_read_from() {
    let (dir, mut workspace) = workspace();
    marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let beta = dir.path().join("beta");
    fs::create_dir_all(&beta).unwrap();
    workspace
        .link(&MindProject::init(&beta).unwrap().0)
        .unwrap();
    let gamma = dir.path().join("gamma");
    fs::create_dir_all(gamma.join(MIND_DIR)).unwrap();
    fs::write(
        gamma.join(MIND_DIR).join(SETTINGS_JSON_FILE),
        r#"{"version": 3, "id": "0123456789abcdef0123456789abcdef", "name": "gamma",
            "agents": ["claude"], "share": "shared", "directories": {}}"#,
    )
    .unwrap();
    workspace.link(&MindProject::open(&gamma).unwrap()).unwrap();
    let old = marked(&dir, &mut workspace, "old-api", &Agent::ALL);
    fs::remove_dir_all(old.root()).unwrap();
    let workspace = FlayerWorkspace::open(dir.path()).unwrap();

    let (status, _) = Status::of_workspace(&workspace);

    let markers: Vec<Option<MarkerFile>> = status
        .projects
        .iter()
        .map(|project| project.marker)
        .collect();
    assert_eq!(
        markers,
        [
            Some(MarkerFile::SettingsToml),
            Some(MarkerFile::MindToml),
            Some(MarkerFile::SettingsJson),
            None,
        ]
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

    let (status, _) = Status::of_workspace(&workspace);

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

    let (linked, _) = Status::of_project(&loose, Some(&workspace));
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
    let beta = marked(&dir, &mut workspace, "beta", &Agent::ALL);
    put(&alpha, ".agents/skills", "deploy");
    put(&beta, "skills", "review");
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

    let (status, failures) = Status::of_workspace(&workspace);

    assert_eq!(
        states(&status.projects[0]),
        [(
            "deploy",
            State::Foreign,
            &[Agent::Codex, Agent::OpenCode][..],
            None
        )]
    );
    assert_eq!(
        states(&status.projects[1]),
        [("review", State::Authored, &[][..], None)]
    );
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0]
        .to_string()
        .contains(&path.display().to_string()));
}

#[test]
fn a_ledger_whose_shelf_cannot_be_read_for_a_standing_is_said() {
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
    // The records read, but not what the shelf offers, which a standing
    // reads to hold an install to where it came from.
    let path = workspace.ledger_path();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE artifacts;")
        .unwrap();

    let (_, failures) = Status::of_workspace(&workspace);
    let (_, failed) = Status::of_project(&alpha, Some(&workspace));

    for failures in [&failures, &failed] {
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0]
            .to_string()
            .contains(&path.display().to_string()));
    }
}

#[test]
fn a_ledger_that_will_not_open_is_said_and_the_report_still_made() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let beta = marked(&dir, &mut workspace, "beta", &Agent::ALL);
    put(&alpha, ".agents/skills", "deploy");
    put(&beta, "skills", "review");
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy"],
    );
    drop(ledger);
    let path = workspace.ledger_path();
    let garbage = "not a database, whatever its name says\n";
    fs::write(&path, garbage).unwrap();

    let (status, failures) = Status::of_workspace(&workspace);
    let (alone, failed) = Status::of_project(&alpha, Some(&workspace));

    assert_eq!(
        states(&status.projects[0]),
        [(
            "deploy",
            State::Foreign,
            &[Agent::Codex, Agent::OpenCode][..],
            None
        )]
    );
    assert_eq!(
        states(&status.projects[1]),
        [("review", State::Authored, &[][..], None)]
    );
    // Linked all the same: what `-p` takes is no record of the ledger's.
    assert_eq!(status.projects[0].entry.as_deref(), Some("alpha"));
    assert_eq!(status.projects[1].entry.as_deref(), Some("beta"));
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0]
        .to_string()
        .contains(&path.display().to_string()));
    assert_eq!(alone.projects, status.projects[..1]);
    assert_eq!(failed.len(), 1, "{failed:?}");
    // Read, so left as it was found.
    assert_eq!(fs::read_to_string(&path).unwrap(), garbage);
}

#[test]
fn reading_a_project_hands_back_the_catalog_it_read_with_the_records_marked() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    put(&alpha, ".agents/skills", "deploy");
    put(&alpha, "skills", "review");
    offer(&workspace, "deploy");
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "alpha",
        "deploy",
        "workspace",
        &[".agents/skills/deploy"],
    );

    let (status, catalog) = ProjectStatus::read(&alpha, &[Kind::Skill], Some(&workspace));

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
        Some(State::Current)
    );
    assert_eq!(status.find(Kind::Rule, "deploy"), None);
    assert!(catalog.artifacts()[0].copies()[0].is_installed());
}

/// `project`, its settings changed to install skills in `folders`.
fn installing_in(project: &mut MindProject, folders: &[&str]) {
    project
        .change_settings(|settings| {
            settings.targets = mindflayer_core::Targets::default()
                .with(Kind::Skill, folders.iter().map(PathBuf::from).collect());
        })
        .unwrap();
}

#[test]
fn a_project_installing_where_no_agent_reads_says_which_agents_read_none_of_it() {
    let (dir, mut workspace) = workspace();
    let mut alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    installing_in(&mut alpha, &[".skills"]);
    let ledger = workspace.ledger().unwrap();
    put(&alpha, ".skills", "deploy");
    offer(&workspace, "deploy");
    record(&ledger, "alpha", "deploy", "workspace", &[".skills/deploy"]);

    let (status, failures) = Status::of_workspace(&workspace);

    assert!(failures.is_empty(), "{failures:?}");
    let alpha = &status.projects[0];
    assert_eq!(
        alpha.unreached,
        [Unreached {
            kind: Kind::Skill,
            targets: vec![String::from(".skills")],
            agents: Agent::ALL.to_vec(),
        }]
    );
    // Current, and reaching no agent.
    assert_eq!(
        states(alpha),
        [("deploy", State::Current, &[][..], Some("workspace"))]
    );
}

#[test]
fn an_agent_reading_one_of_the_targets_is_reached_and_one_pointed_there_by_a_link_too() {
    let (dir, mut workspace) = workspace();
    let mut alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    installing_in(&mut alpha, &[".skills", ".claude/skills"]);

    let (status, _) = Status::of_project(&alpha, Some(&workspace));

    // OpenCode reads Claude's folder too, so only Codex misses out.
    assert_eq!(
        status.projects[0].unreached,
        [Unreached {
            kind: Kind::Skill,
            targets: vec![String::from(".claude/skills"), String::from(".skills")],
            agents: vec![Agent::Codex],
        }]
    );

    #[cfg(unix)]
    {
        fs::create_dir_all(alpha.root().join(".skills")).unwrap();
        fs::create_dir_all(alpha.root().join(".agents")).unwrap();
        std::os::unix::fs::symlink(
            alpha.root().join(".skills"),
            alpha.root().join(".agents").join("skills"),
        )
        .unwrap();

        let (status, _) = Status::of_project(&alpha, Some(&workspace));

        assert_eq!(status.projects[0].unreached, []);
    }
}

#[test]
fn a_project_installing_where_its_agents_read_has_nothing_unreached() {
    let (dir, mut workspace) = workspace();
    marked(&dir, &mut workspace, "alpha", &Agent::ALL);

    let (status, _) = Status::of_workspace(&workspace);

    assert_eq!(status.projects[0].unreached, []);
}

#[test]
fn a_target_no_install_may_use_is_a_failure_beside_the_report() {
    let (dir, mut workspace) = workspace();
    let alpha = marked(&dir, &mut workspace, "alpha", &Agent::ALL);
    let path = alpha.marker_path();
    let text =
        fs::read_to_string(&path).unwrap() + "\n[targets]\nskills = [\".skills\", \"../shared\"]\n";
    fs::write(&path, text).unwrap();

    let (status, failures) = Status::of_workspace(&workspace);
    let alpha = MindProject::open(alpha.root()).unwrap();
    let (_, alone) = Status::of_project(&alpha, Some(&workspace));

    for failures in [&failures, &alone] {
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(
            matches!(&failures[0], StatusFailure::Target(misplaced)
                if misplaced.folder.as_path() == std::path::Path::new("../shared")),
            "{}",
            failures[0]
        );
    }
    assert_eq!(status.projects[0].unreached[0].targets, [".skills"]);
}
