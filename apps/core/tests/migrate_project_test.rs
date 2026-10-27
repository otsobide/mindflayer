//! Moving a project an older Mindflayer made from its `mind.toml` to a
//! `settings.toml`, and what is installed into it to where its agents read,
//! or from its `settings.json` to a `settings.toml` saying the same.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{skill, workspace};
use mindflayer_core::install;
use mindflayer_core::ledger::{InstallRecord, Target};
use mindflayer_core::migrate::{self, Refusal};
use mindflayer_core::reapply::Why;
use mindflayer_core::{
    Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MarkerFile, MindProject, Settings,
    Share, MIND_CONFIG, MIND_DIR, SETTINGS_FILE, SETTINGS_JSON_FILE, SETTINGS_TOML_VERSION,
    TARGETS_VERSION,
};
use tempfile::TempDir;

/// A workspace keeping `names` as skills of its own, and `collapse`, a
/// repository a version 2 `mind init` made a project, authoring in
/// `directories` and linked, with each of `names` installed into it.
fn older(names: &[&str], directories: &Directories) -> (TempDir, FlayerWorkspace, MindProject) {
    let (dir, mut workspace) = workspace();
    let own = workspace.own().directory_for(Kind::Skill).unwrap();
    for name in names {
        fs::create_dir_all(own.join(name)).unwrap();
        fs::write(
            own.join(name).join("SKILL.md"),
            skill(name, &format!("What {name} does")),
        )
        .unwrap();
    }

    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) = MindProject::init_with(&root, directories).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    for name in names {
        let candidate =
            install::offered(&workspace, &ledger, &project, Kind::Skill, name, None).unwrap();
        install::install(
            &workspace,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    (dir, workspace, project)
}

/// Rewrite the record of `name` the way a ledger from before schema 3 holds
/// it, once migrated: no origin for something of the workspace's own, and
/// no hash.
fn from_before_hashes(workspace: &FlayerWorkspace, name: &str) {
    let ledger = workspace.ledger().unwrap();
    let record = ledger
        .installation("collapse", Kind::Skill, name)
        .unwrap()
        .unwrap();
    ledger
        .installed(
            "collapse",
            &InstallRecord {
                origin: None,
                content_hash: None,
                ..record
            },
        )
        .unwrap();
}

fn claudes(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".claude").join("skills").join(name)
}

fn agents(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".agents").join("skills").join(name)
}

fn authored(project: &MindProject, name: &str) -> PathBuf {
    project.root().join("skills").join(name)
}

fn write_skill(folder: &Path, name: &str, description: &str) {
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join("SKILL.md"), skill(name, description)).unwrap();
}

/// What the ledger records of `name` in `collapse`.
fn targets(workspace: &FlayerWorkspace, name: &str) -> Vec<Target> {
    workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, name)
        .unwrap()
        .map(|record| record.targets)
        .unwrap_or_default()
}

fn target(agent: Option<&str>, path: &str) -> Target {
    Target {
        agent: agent.map(str::to_owned),
        path: path.to_owned(),
        content_hash: None,
    }
}

/// Every file under `root`, with what it holds: a picture of a tree to hold
/// a dry run to.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

fn plan(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    agents: &[Agent],
    share: Share,
) -> migrate::ProjectPlan {
    migrate::project_plan(
        Some(workspace),
        project,
        agents,
        share,
        &AgentHomes::default(),
    )
    .unwrap()
    .expect("a project an older marker marks")
}

#[test]
fn a_plan_moves_every_installation_where_the_agents_read_and_writes_nothing() {
    let (dir, workspace, project) = older(&["deploy", "review"], &Directories::default());
    from_before_hashes(&workspace, "review");
    write_skill(&authored(&project, "notes"), "notes", "Mine");
    let before = tree(dir.path());

    let plan = plan(&workspace, &project, &Agent::ALL, Share::Shared);

    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert!(plan.recorded);
    assert_eq!(plan.settings.name, "collapse");
    assert_eq!(plan.settings.agents, Agent::ALL);
    assert_eq!(plan.settings.share, Share::Shared);
    assert_eq!(
        plan.settings.directories,
        Directories::default()
            .with(Kind::Skill, "skills")
            .with(Kind::Rule, "rules")
    );
    let mind = project.root().join(MIND_DIR);
    assert_eq!(plan.settings_file, mind.join(SETTINGS_FILE));
    assert_eq!(plan.marker, mind.join(MIND_CONFIG));
    assert_eq!(plan.backup, mind.join("tmp").join("mind.toml.v2"));
    assert_eq!(plan.ignore, None);
    // One with a hash and one without, which is held to its offer: both are
    // what was installed, so both move.
    let names: Vec<&str> = plan
        .installations
        .iter()
        .map(|retarget| retarget.name.as_str())
        .collect();
    assert_eq!(names, ["deploy", "review"]);
    for retarget in &plan.installations {
        let added: Vec<(&Path, &[Agent])> = retarget
            .added
            .iter()
            .map(|added| (added.path.as_path(), added.agents.as_slice()))
            .collect();
        assert_eq!(
            added,
            [
                (
                    agents(&project, &retarget.name).as_path(),
                    &[Agent::Codex, Agent::OpenCode][..]
                ),
                (
                    claudes(&project, &retarget.name).as_path(),
                    &[Agent::Claude][..]
                ),
            ]
        );
        assert_eq!(retarget.removed, [authored(&project, &retarget.name)]);
        assert!(retarget.left.is_empty(), "{:?}", retarget.left);
    }
    assert_eq!(plan.authored, [authored(&project, "notes")]);
    assert!(plan.foreign.is_empty(), "{:?}", plan.foreign);
    assert!(!plan.tracked);
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_project_its_settings_mark_has_nothing_to_plan() {
    let (dir, workspace) = workspace();
    let root = dir.path().join("collapse");
    let (project, _) = MindProject::init_asked(
        &root,
        &[Agent::Claude],
        Share::Local,
        &Directories::default(),
    )
    .unwrap();

    let plan = migrate::project_plan(
        Some(&workspace),
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(plan.is_none(), "{plan:?}");
}

#[test]
fn a_plan_for_a_local_project_keeps_its_mind_and_its_copies_out_of_git() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());

    let plan = plan(&workspace, &project, &[Agent::Claude], Share::Local);

    assert_eq!(
        plan.ignore,
        Some(project.root().join(MIND_DIR).join(".gitignore"))
    );
    assert_eq!(
        plan.exclude_file,
        Some(project.root().join(".git").join("info").join("exclude"))
    );
    assert!(!plan.outside);
    // Nothing was in place where Claude reads, so nothing is kept out but
    // the copy put there.
    assert!(plan.excluded.is_empty(), "{:?}", plan.excluded);
    assert_eq!(
        plan.installations[0]
            .added
            .iter()
            .map(|added| added.path.clone())
            .collect::<Vec<_>>(),
        [claudes(&project, "deploy")]
    );
}

#[test]
fn a_copy_already_where_an_agent_reads_stays_and_a_new_one_goes_beside_it() {
    let (_dir, workspace, project) = older(
        &["deploy"],
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    );
    assert_eq!(
        targets(&workspace, "deploy"),
        [target(None, ".claude/skills/deploy")]
    );

    let plan = plan(&workspace, &project, &Agent::ALL, Share::Local);

    let retarget = &plan.installations[0];
    assert!(retarget.removed.is_empty(), "{retarget:?}");
    assert_eq!(retarget.added.len(), 1);
    assert_eq!(retarget.added[0].path, agents(&project, "deploy"));
    assert_eq!(
        plan.settings.directories.get(Kind::Skill),
        Some(Path::new(".claude/skills"))
    );
    // In place where Claude reads, so a local project keeps it out of git.
    assert_eq!(plan.excluded, [claudes(&project, "deploy")]);
}

#[test]
fn a_copy_edited_since_it_was_installed_without_a_hash_is_left_and_copied_nowhere() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());
    from_before_hashes(&workspace, "deploy");
    write_skill(&authored(&project, "deploy"), "deploy", "Edited by hand");

    let plan = plan(&workspace, &project, &Agent::ALL, Share::Shared);

    let retarget = &plan.installations[0];
    assert!(retarget.added.is_empty() && retarget.removed.is_empty());
    let left: Vec<(&Path, &Why)> = retarget
        .left
        .iter()
        .map(|left| (left.path.as_path(), &left.why))
        .collect();
    assert_eq!(
        left,
        [
            (authored(&project, "deploy").as_path(), &Why::Edited),
            (agents(&project, "deploy").as_path(), &Why::Unsourced),
            (claudes(&project, "deploy").as_path(), &Why::Unsourced),
        ]
    );
    // Its record goes, so it is not listed with what the project wrote
    // itself until it is migrated: said once, as edited.
    assert!(plan.authored.is_empty(), "{:?}", plan.authored);
}

#[test]
fn somebodys_skill_where_a_copy_would_go_is_left_and_what_else_is_there_said() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());
    write_skill(&claudes(&project, "deploy"), "deploy", "Mine");
    write_skill(&claudes(&project, "notes"), "notes", "Mine too");

    let plan = plan(&workspace, &project, &Agent::ALL, Share::Shared);

    let retarget = &plan.installations[0];
    assert_eq!(retarget.added.len(), 1, "{retarget:?}");
    assert_eq!(retarget.added[0].path, agents(&project, "deploy"));
    assert_eq!(retarget.left[0].path, claudes(&project, "deploy"));
    assert_eq!(retarget.left[0].why, Why::Foreign);
    assert_eq!(plan.foreign, [claudes(&project, "notes")]);
}

#[test]
fn a_project_no_workspace_links_moves_to_settings_and_carries_nothing() {
    let (dir, _workspace, project) = older(&["deploy"], &Directories::default());
    let elsewhere = TempDir::new().unwrap();
    let (other, _) = FlayerWorkspace::init(elsewhere.path()).unwrap();
    let before = tree(dir.path());

    let plan = plan(&other, &project, &Agent::ALL, Share::Shared);

    assert!(!plan.recorded);
    assert!(plan.installations.is_empty());
    // Nothing says it was installed, so it reads as the project's own.
    assert_eq!(plan.authored, [authored(&project, "deploy")]);
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn every_reason_not_to_go_ahead_is_said_at_once() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("ancient");
    fs::create_dir_all(root.join(MIND_DIR).join("tmp")).unwrap();
    // A version 1 marker: every kind inside `.mind`.
    fs::write(
        root.join(MIND_DIR).join(MIND_CONFIG),
        "version = 1\nname = \"ancient\"\n",
    )
    .unwrap();
    fs::write(root.join(MIND_DIR).join("tmp").join("mind.toml.v1"), "").unwrap();
    let project = MindProject::open(&root).unwrap();

    let plan = migrate::project_plan(None, &project, &[], Share::Shared, &AgentHomes::default())
        .unwrap()
        .unwrap();

    let said: Vec<String> = plan.refusals.iter().map(ToString::to_string).collect();
    assert_eq!(said.len(), 4, "{said:#?}");
    assert!(
        matches!(&plan.refusals[0], Refusal::Authored(_)),
        "{said:#?}"
    );
    assert!(said[0].starts_with(".mind/skills: cannot hold a project's skills"));
    assert!(said[1].starts_with(".mind/rules: cannot hold a project's rules"));
    assert_eq!(
        said[2],
        "ancient would name no agent, and what is installed into it would reach none"
    );
    assert!(matches!(&plan.refusals[3], Refusal::Backup { .. }));
}

#[test]
fn a_version_1_project_naming_its_folders_outside_mind_moves_with_them() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("ancient");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    // What the refusal of one that kept them inside `.mind` asks for.
    fs::write(
        root.join(MIND_DIR).join(MIND_CONFIG),
        "version = 1\nname = \"ancient\"\n\n[directories]\nskills = \"skills\"\nrules = \"rules\"\n",
    )
    .unwrap();
    write_skill(&root.join("skills").join("notes"), "notes", "Mine");
    let project = MindProject::open(&root).unwrap();

    let plan = migrate::project_plan(
        None,
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap()
    .unwrap();

    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert_eq!(
        plan.settings.directories.get(Kind::Skill),
        Some(Path::new("skills"))
    );
    assert_eq!(
        plan.settings.directories.get(Kind::Rule),
        Some(Path::new("rules"))
    );
    assert_eq!(plan.authored, [root.join("skills").join("notes")]);
}

fn migrated(
    workspace: &FlayerWorkspace,
    project: &MindProject,
    agents: &[Agent],
    share: Share,
) -> migrate::ProjectMigrated {
    migrate::project(
        Some(workspace),
        project,
        agents,
        share,
        &AgentHomes::default(),
    )
    .unwrap()
    .expect("a project an older marker marks")
}

/// The lines of the project's group in the repository's exclude file.
fn excluded(project: &MindProject) -> Vec<String> {
    let id = project.settings().unwrap().id.clone();
    let text = fs::read_to_string(project.root().join(".git/info/exclude")).unwrap();
    let header = format!("# project {id}");
    text.lines()
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn migrating_moves_the_installs_where_the_agents_read_and_leaves_what_the_project_wrote() {
    let (_dir, workspace, project) = older(&["deploy", "review"], &Directories::default());
    from_before_hashes(&workspace, "review");
    write_skill(&authored(&project, "notes"), "notes", "Mine");
    let marker = fs::read(project.root().join(MIND_DIR).join(MIND_CONFIG)).unwrap();
    let installed = fs::read(authored(&project, "deploy").join("SKILL.md")).unwrap();

    let done = migrated(&workspace, &project, &Agent::ALL, Share::Shared);

    assert_eq!(done.plan.authored, [authored(&project, "notes")]);
    // Marked by its settings now, and named, keeping its folders, as before.
    let reopened = MindProject::open(project.root()).unwrap();
    assert!(reopened.warnings().is_empty(), "{:?}", reopened.warnings());
    let settings = reopened.settings().expect("marked by its settings");
    assert_eq!(settings.name, "collapse");
    assert_eq!(settings.agents, Agent::ALL);
    assert_eq!(settings.share, Share::Shared);
    assert_eq!(settings.id, done.plan.settings.id);
    assert_eq!(
        reopened.directory_for(Kind::Rule),
        Some(project.root().join("rules"))
    );
    let mind = project.root().join(MIND_DIR);
    assert!(!mind.join(MIND_CONFIG).exists());
    assert_eq!(
        fs::read(mind.join("tmp").join("mind.toml.v2")).unwrap(),
        marker
    );
    // Shared, as a project from before settings was committed by design.
    assert!(!mind.join(".gitignore").exists());

    for name in ["deploy", "review"] {
        assert!(!authored(&project, name).exists(), "{name}");
        assert_eq!(
            targets(&workspace, name),
            [
                target(None, &format!(".agents/skills/{name}")),
                target(Some("claude"), &format!(".claude/skills/{name}")),
            ]
        );
    }
    assert_eq!(
        fs::read(claudes(&project, "deploy").join("SKILL.md")).unwrap(),
        installed
    );
    assert_eq!(
        fs::read(agents(&project, "deploy").join("SKILL.md")).unwrap(),
        installed
    );
    assert!(authored(&project, "notes").join("SKILL.md").is_file());
    // A record from before hashes was found current by its offer, and now
    // holds the hash of what every copy holds.
    let review = workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, "review")
        .unwrap()
        .unwrap();
    assert_eq!(
        review.content_hash,
        Some(mindflayer_core::hash::tree(&claudes(&project, "review")).unwrap())
    );
    // Nothing of the stage it moved them through is left.
    let scratch: Vec<String> = fs::read_dir(mind.join("tmp"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    assert_eq!(scratch, ["mind.toml.v2"]);
}

#[test]
fn a_project_whose_skills_were_where_claude_reads_keeps_that_copy() {
    let (_dir, workspace, project) = older(
        &["deploy"],
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    );
    let copy = claudes(&project, "deploy").join("SKILL.md");
    let installed = fs::read(&copy).unwrap();

    migrated(&workspace, &project, &Agent::ALL, Share::Shared);

    assert_eq!(fs::read(&copy).unwrap(), installed);
    assert_eq!(
        fs::read(agents(&project, "deploy").join("SKILL.md")).unwrap(),
        installed
    );
    assert_eq!(
        targets(&workspace, "deploy"),
        [
            target(None, ".agents/skills/deploy"),
            target(Some("claude"), ".claude/skills/deploy"),
        ]
    );
    let reopened = MindProject::open(project.root()).unwrap();
    assert_eq!(
        reopened.directory_for(Kind::Skill),
        Some(project.root().join(".claude/skills"))
    );
}

#[test]
fn an_edited_copy_from_before_hashes_is_left_where_it_is_and_no_longer_recorded() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());
    from_before_hashes(&workspace, "deploy");
    write_skill(&authored(&project, "deploy"), "deploy", "Edited by hand");
    let edited = fs::read(authored(&project, "deploy").join("SKILL.md")).unwrap();

    let done = migrated(&workspace, &project, &Agent::ALL, Share::Shared);

    assert_eq!(done.plan.installations[0].left[0].why, Why::Edited);
    assert_eq!(
        fs::read(authored(&project, "deploy").join("SKILL.md")).unwrap(),
        edited
    );
    assert!(!claudes(&project, "deploy").exists());
    assert!(!agents(&project, "deploy").exists());
    // Its author's now: no record names it.
    assert!(targets(&workspace, "deploy").is_empty());
}

#[test]
fn a_second_migration_finds_nothing_to_do_and_writes_nothing() {
    let (dir, workspace, project) = older(&["deploy"], &Directories::default());
    migrated(&workspace, &project, &Agent::ALL, Share::Shared);
    let before = tree(dir.path());

    let again = migrate::project(
        Some(&workspace),
        &MindProject::open(project.root()).unwrap(),
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap();
    // Handed the project as it was opened before the first, too.
    let stale = migrate::project(
        Some(&workspace),
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(again.is_none() && stale.is_none());
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_moved_project_has_nothing_to_plan_even_when_the_ledger_cannot_be_read() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());
    migrated(&workspace, &project, &Agent::ALL, Share::Shared);
    // A ledger a newer Mindflayer wrote is refused: a dry run says what the
    // real one will, which never opens it for a project with nothing to move.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let moved = MindProject::open(project.root()).unwrap();

    let planned = migrate::project_plan(
        Some(&workspace),
        &moved,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap();
    let done = migrate::project(
        Some(&workspace),
        &moved,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(planned.is_none() && done.is_none());
}

#[test]
fn a_migration_that_fails_part_way_puts_back_everything_it_wrote() {
    let (dir, workspace, project) = older(&["deploy", "review"], &Directories::default());
    // Where the settings would go, so writing them fails once every copy
    // is placed and the old ones are moved out of the way.
    let settings = project.root().join(MIND_DIR).join(SETTINGS_FILE);
    fs::create_dir(&settings).unwrap();
    let before = tree(dir.path());
    let recorded = targets(&workspace, "deploy");

    let failed = migrate::project(
        Some(&workspace),
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(failed, migrate::MigrateError::Project { .. }),
        "{failed:?}"
    );
    assert!(
        failed.to_string().ends_with("; nothing was migrated"),
        "{failed}"
    );
    assert!(
        failed.left_behind().is_empty(),
        "{:?}",
        failed.left_behind()
    );
    assert!(project.root().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(MindProject::open(project.root())
        .unwrap()
        .settings()
        .is_none());
    assert!(!claudes(&project, "deploy").exists());
    assert!(!agents(&project, "review").exists());
    assert_eq!(targets(&workspace, "deploy"), recorded);
    // Every file as it was, and nothing new in the scratch folder but what
    // a stage leaves there for the next.
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_local_migration_keeps_mind_and_the_copies_out_of_git() {
    let (_dir, workspace, project) = older(&["deploy"], &Directories::default());

    migrated(&workspace, &project, &[Agent::Claude], Share::Local);

    let reopened = MindProject::open(project.root()).unwrap();
    assert_eq!(reopened.settings().unwrap().share, Share::Local);
    assert_eq!(
        fs::read_to_string(project.root().join(MIND_DIR).join(".gitignore")).unwrap(),
        mindflayer_core::MIND_IGNORE
    );
    assert_eq!(excluded(&reopened), ["/.claude/skills/deploy/"]);
    assert_eq!(
        targets(&workspace, "deploy"),
        [target(Some("claude"), ".claude/skills/deploy")]
    );
}

#[test]
fn a_project_no_workspace_links_is_moved_and_what_it_holds_left_alone() {
    let (_dir, _workspace, project) = older(&["deploy"], &Directories::default());

    let done = migrate::project(
        None,
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap()
    .unwrap();

    assert!(!done.plan.recorded);
    assert!(authored(&project, "deploy").join("SKILL.md").is_file());
    assert!(!claudes(&project, "deploy").exists());
    assert!(MindProject::open(project.root())
        .unwrap()
        .settings()
        .is_some());
}

// ---------------------------------------------------------------------------
// From a settings.json
// ---------------------------------------------------------------------------

/// What a version 4 `mind init` and `mind targets set` wrote, laid out as
/// somebody keeps it by hand, with a key this build does not know.
const JSON: &str = r#"{
    "version": 4,
    "id": "0123456789abcdef0123456789abcdef",
    "name": "collapse",
    "agents": ["claude", "codex"],
    "share": "local",
    "edit_tracked": true,
    "servers": {"github": ["npx"]},
    "directories": {"skills": "skills", "rules": "docs/rules", "mcp": "servers"},
    "targets": {"skills": [".skills"]}
}
"#;

/// The names in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A workspace, and `collapse` beside it, a project [`JSON`] marks, linked:
/// with no install, so the workspace has no ledger yet.
fn json_project() -> (TempDir, FlayerWorkspace, MindProject) {
    let (dir, mut workspace) = workspace();
    let root = dir.path().join("collapse");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(root.join(MIND_DIR).join(SETTINGS_JSON_FILE), JSON).unwrap();
    let project = MindProject::open(&root).unwrap();
    workspace.link(&project).unwrap();
    (dir, workspace, project)
}

#[test]
fn a_settings_json_is_planned_into_a_settings_toml_saying_the_same_and_nothing_written() {
    let (dir, workspace, project) = json_project();
    let before = tree(dir.path());

    // What `--agents` and `--local` ask is a mind.toml's to be given: a
    // settings.json says its own.
    let plan = plan(&workspace, &project, &[Agent::OpenCode], Share::Shared);

    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    let read = project.settings().unwrap();
    assert_eq!(read.version, TARGETS_VERSION);
    assert_eq!(
        plan.settings,
        Settings {
            version: SETTINGS_TOML_VERSION,
            ..read.clone()
        }
    );
    let mind = project.root().join(MIND_DIR);
    assert_eq!(plan.settings_file, mind.join(SETTINGS_FILE));
    assert_eq!(plan.marker, mind.join(SETTINGS_JSON_FILE));
    assert_eq!(plan.backup, mind.join("tmp").join("settings.json.v4"));
    assert!(!plan.recorded);
    assert_eq!(plan.ignore, None);
    assert!(plan.installations.is_empty());
    assert!(plan.authored.is_empty() && plan.foreign.is_empty() && plan.excluded.is_empty());
    assert!(!migrate::carries_installs(&project));
    assert_eq!(tree(dir.path()), before);
    // No workspace is read for a move that carries no copy.
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn migrating_a_settings_json_writes_a_settings_toml_and_keeps_it_as_it_was() {
    let (dir, workspace, project) = json_project();
    let mind = project.root().join(MIND_DIR);

    let done = migrated(&workspace, &project, &[Agent::OpenCode], Share::Shared);

    assert_eq!(names_in(&mind), ["settings.toml", "tmp"]);
    let written = fs::read_to_string(mind.join(SETTINGS_FILE)).unwrap();
    assert_eq!(written, done.plan.settings.text().unwrap());
    assert!(
        written.ends_with(
            "version = 5\n\
             id = \"0123456789abcdef0123456789abcdef\"\n\
             name = \"collapse\"\n\
             agents = [\"claude\", \"codex\"]\n\
             share = \"local\"\n\
             edit_tracked = true\n\
             \n\
             [directories]\n\
             skills = \"skills\"\n\
             rules = \"docs/rules\"\n\
             mcp = \"servers\"\n\
             \n\
             [targets]\n\
             skills = [\".skills\"]\n"
        ),
        "{written}"
    );
    // Kept as it was, where a project's own `.gitignore` keeps it out of git.
    assert_eq!(
        fs::read_to_string(mind.join("tmp").join("settings.json.v4")).unwrap(),
        JSON
    );
    assert!(mind.join("tmp").join(".gitignore").is_file());
    let reopened = MindProject::open(project.root()).unwrap();
    assert!(reopened.warnings().is_empty(), "{:?}", reopened.warnings());
    assert_eq!(
        reopened.settings().unwrap(),
        &Settings {
            version: SETTINGS_TOML_VERSION,
            ..project.settings().unwrap().clone()
        }
    );
    assert!(!workspace.ledger_path().exists());

    // Nothing to move a second time, whichever project it is handed.
    let before = tree(dir.path());
    for again in [&reopened, &project] {
        let moved = migrate::project(
            Some(&workspace),
            again,
            &Agent::ALL,
            Share::Shared,
            &AgentHomes::default(),
        )
        .unwrap();
        assert!(moved.is_none(), "{moved:?}");
    }
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_settings_json_moves_with_no_workspace_at_all() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(MIND_DIR)).unwrap();
    fs::write(dir.path().join(MIND_DIR).join(SETTINGS_JSON_FILE), JSON).unwrap();
    let project = MindProject::open(dir.path()).unwrap();

    let done = migrate::project(
        None,
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap()
    .expect("a project a settings.json marks");

    assert_eq!(
        MindProject::open(dir.path()).unwrap().settings(),
        Some(&done.plan.settings)
    );
}

#[test]
fn a_settings_json_kept_by_an_earlier_migration_refuses_the_move() {
    let (dir, workspace, project) = json_project();
    let kept = project.scratch_dir().join("settings.json.v4");
    fs::create_dir_all(project.scratch_dir()).unwrap();
    fs::write(&kept, "somebody's").unwrap();
    let before = tree(dir.path());

    let planned = plan(&workspace, &project, &Agent::ALL, Share::Shared);
    let failed = migrate::project(
        Some(&workspace),
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(&planned.refusals[..], [Refusal::Backup { path }] if *path == kept),
        "{:?}",
        planned.refusals
    );
    assert!(
        matches!(&failed, migrate::MigrateError::Refused(refusals) if refusals.len() == 1),
        "{failed}"
    );
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn a_mind_toml_is_moved_straight_to_a_settings_toml() {
    let (_dir, workspace, project) = older(&[], &Directories::default());
    let mind = project.root().join(MIND_DIR);

    let done = migrated(&workspace, &project, &[Agent::Claude], Share::Shared);

    assert_eq!(done.plan.settings.version, SETTINGS_TOML_VERSION);
    assert_eq!(
        fs::read_to_string(mind.join(SETTINGS_FILE)).unwrap(),
        done.plan.settings.text().unwrap()
    );
    assert!(!mind.join(SETTINGS_JSON_FILE).exists());
    assert_eq!(
        MindProject::open(project.root()).unwrap().marker_file(),
        MarkerFile::SettingsToml
    );
}
