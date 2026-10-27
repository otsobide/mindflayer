//! Changing a project's agents or share mode, and carrying what was
//! installed into it along.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{skill, workspace};
use mindflayer_core::hash;
use mindflayer_core::install;
use mindflayer_core::ledger::{Ledger, Target};
use mindflayer_core::reapply::{self, Change, Ignore, Why};
use mindflayer_core::{
    Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, ReapplyError, Share,
    WorkspaceError, MIND_DIR, MIND_IGNORE, SETTINGS_FILE,
};
use tempfile::TempDir;

/// A workspace keeping a `deploy` skill of its own, and `collapse`, a
/// repository linked to it, marked by its settings naming `agents` in
/// `share` mode, with `deploy` installed.
fn installed(agents: &[Agent], share: Share) -> (TempDir, FlayerWorkspace, MindProject) {
    let (dir, mut workspace) = workspace();
    let folder = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("SKILL.md"), skill("deploy", "Ship it")).unwrap();

    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) = MindProject::init_settings(
        &root,
        agents,
        share,
        &Directories::default().with(Kind::Skill, "skills"),
    )
    .unwrap();
    workspace.link(&project).unwrap();

    let ledger = workspace.ledger().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    (dir, workspace, project)
}

fn claudes(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".claude").join("skills").join(name)
}

fn agents(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".agents").join("skills").join(name)
}

/// What the ledger records of `deploy` in `collapse`.
fn targets(workspace: &FlayerWorkspace) -> Vec<Target> {
    workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .map(|record| record.targets)
        .unwrap_or_default()
}

fn target(agent: Option<&str>, path: &str) -> Target {
    Target {
        agent: agent.map(str::to_owned),
        path: path.to_owned(),
    }
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

fn settings_file(project: &MindProject) -> PathBuf {
    project.root().join(MIND_DIR).join(SETTINGS_FILE)
}

fn reopened(project: &MindProject) -> MindProject {
    MindProject::open(project.root()).unwrap()
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

#[test]
fn adding_codex_to_a_claude_only_project_copies_each_installation_where_codex_reads() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    assert!(!agents(&project, "deploy").exists());

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(applied.failures.is_empty(), "{:?}", applied.failures);
    let plan = &applied.plan;
    assert_eq!(plan.before.agents, [Agent::Claude]);
    assert_eq!(plan.after.agents, [Agent::Claude, Agent::Codex]);
    assert_eq!(plan.installations.len(), 1);
    let moved = &plan.installations[0];
    assert_eq!(moved.name, "deploy");
    assert_eq!(moved.added.len(), 1);
    assert_eq!(moved.added[0].path, agents(&project, "deploy"));
    assert_eq!(moved.added[0].agents, [Agent::Codex]);
    assert!(moved.removed.is_empty() && moved.left.is_empty());

    // The same bytes as the copy Claude reads, and recorded as one more
    // target of the one installation, which keeps its hash.
    let copy = agents(&project, "deploy");
    assert_eq!(
        hash::tree(&copy).unwrap(),
        hash::tree(&claudes(&project, "deploy")).unwrap()
    );
    assert_eq!(
        targets(&workspace),
        [
            target(Some("codex"), ".agents/skills/deploy"),
            target(Some("claude"), ".claude/skills/deploy"),
        ]
    );
    assert_eq!(
        reopened(&project).settings().unwrap().agents,
        plan.after.agents
    );
    // Local, so the new copy is kept out of git with the other.
    assert_eq!(
        excluded(&project),
        ["/.claude/skills/deploy/", "/.agents/skills/deploy/"]
    );
}

#[test]
fn removing_claude_takes_out_its_copies_and_leaves_somebodys_where_they_are() {
    let (_dir, workspace, project) = installed(&Agent::ALL, Share::Local);
    let notes = claudes(&project, "notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill("notes", "Mine")).unwrap();

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::RemoveAgents(vec![Agent::Claude]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(applied.failures.is_empty(), "{:?}", applied.failures);
    let plan = &applied.plan;
    assert_eq!(plan.after.agents, [Agent::Codex, Agent::OpenCode]);
    assert_eq!(plan.installations[0].removed, [claudes(&project, "deploy")]);
    // Somebody's is said, and left as it was.
    assert_eq!(plan.foreign, std::slice::from_ref(&notes));
    assert!(notes.join("SKILL.md").is_file());
    assert!(!claudes(&project, "deploy").exists());
    assert!(agents(&project, "deploy").join("SKILL.md").is_file());
    // `.agents/skills` is for the two agents still named.
    assert_eq!(targets(&workspace), [target(None, ".agents/skills/deploy")]);
    assert_eq!(excluded(&project), ["/.agents/skills/deploy/"]);
}

#[test]
fn a_dry_run_says_what_would_change_and_writes_nothing() {
    let (dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let before = tree(dir.path());

    let plan = reapply::plan(
        Some(&workspace),
        &project,
        &Change::Agents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(plan.after.agents, [Agent::Codex]);
    let moved = &plan.installations[0];
    assert_eq!(moved.added[0].path, agents(&project, "deploy"));
    assert_eq!(moved.removed, [claudes(&project, "deploy")]);
    assert!(plan.recorded);
    assert!(!plan.is_empty());
    assert_eq!(tree(dir.path()), before);

    // Done, it does what it said, and a second run finds nothing left to do.
    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::Agents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();
    assert_eq!(applied.plan, plan);
    let again = reapply::plan(
        Some(&workspace),
        &reopened(&project),
        &Change::Agents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();
    assert!(again.is_empty(), "{again:?}");
}

#[test]
fn a_project_a_mind_toml_marks_is_refused_and_sent_to_mind_migrate() {
    let (dir, mut workspace) = workspace();
    let root = dir.path().join("older");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    workspace.link(&project).unwrap();

    for change in [
        Change::AddAgents(vec![Agent::Codex]),
        Change::Share {
            share: Share::Shared,
            edit_tracked: false,
        },
    ] {
        let error =
            reapply::plan(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap_err();
        assert!(
            matches!(
                error,
                ReapplyError::Workspace(WorkspaceError::NoSettings { .. })
            ),
            "{error}"
        );
        assert!(error.to_string().contains("`mind migrate`"), "{error}");
        assert!(
            reapply::apply(Some(&workspace), &project, &change, &AgentHomes::default()).is_err()
        );
    }
    assert!(!settings_file(&project).exists());
    // Not even its ledger is opened to find that out.
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn removing_the_last_agent_is_refused() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let settings = fs::read(settings_file(&project)).unwrap();

    let error = reapply::apply(
        Some(&workspace),
        &project,
        &Change::RemoveAgents(vec![Agent::Claude]),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(matches!(error, ReapplyError::NoAgent { .. }), "{error}");
    assert_eq!(fs::read(settings_file(&project)).unwrap(), settings);
    assert!(claudes(&project, "deploy").join("SKILL.md").is_file());
}

#[test]
fn an_edited_copy_in_a_dropped_folder_is_left_and_stops_being_mindflayers() {
    let (_dir, workspace, project) = installed(&Agent::ALL, Share::Local);
    let edited = claudes(&project, "deploy").join("SKILL.md");
    fs::write(&edited, skill("deploy", "Ship it, my way")).unwrap();

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::RemoveAgents(vec![Agent::Claude]),
        &AgentHomes::default(),
    )
    .unwrap();

    let moved = &applied.plan.installations[0];
    assert!(moved.removed.is_empty());
    assert_eq!(moved.left.len(), 1);
    assert_eq!(moved.left[0].path, claudes(&project, "deploy"));
    assert_eq!(moved.left[0].why, Why::Edited);
    assert_eq!(
        fs::read_to_string(&edited).unwrap(),
        skill("deploy", "Ship it, my way")
    );
    // Its author's now: not recorded, and not kept out of git.
    assert_eq!(targets(&workspace), [target(None, ".agents/skills/deploy")]);
    assert_eq!(excluded(&project), ["/.agents/skills/deploy/"]);
}

#[test]
fn somebodys_skill_where_a_new_copy_would_go_is_left_and_nothing_is_written_over_it() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let theirs = agents(&project, "deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Theirs")).unwrap();

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    let moved = &applied.plan.installations[0];
    assert!(moved.added.is_empty());
    assert_eq!(moved.left[0].path, theirs);
    assert_eq!(moved.left[0].why, Why::Foreign);
    assert_eq!(
        fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        skill("deploy", "Theirs")
    );
    assert_eq!(
        targets(&workspace),
        [target(Some("claude"), ".claude/skills/deploy")]
    );
    // The agents change all the same.
    assert_eq!(
        reopened(&project).settings().unwrap().agents,
        [Agent::Claude, Agent::Codex]
    );
}

#[test]
fn with_no_copy_still_as_installed_nothing_is_copied_to_a_new_folder() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    fs::write(
        claudes(&project, "deploy").join("SKILL.md"),
        skill("deploy", "Edited"),
    )
    .unwrap();

    let plan = reapply::plan(
        Some(&workspace),
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    let moved = &plan.installations[0];
    assert!(moved.added.is_empty());
    assert_eq!(moved.left[0].path, agents(&project, "deploy"));
    assert_eq!(moved.left[0].why, Why::Unsourced);
}

#[test]
fn a_copy_gone_from_a_dropped_folder_is_forgotten() {
    let (_dir, workspace, project) = installed(&Agent::ALL, Share::Local);
    fs::remove_dir_all(claudes(&project, "deploy")).unwrap();

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::RemoveAgents(vec![Agent::Claude]),
        &AgentHomes::default(),
    )
    .unwrap();

    let moved = &applied.plan.installations[0];
    assert_eq!(moved.forgotten, [claudes(&project, "deploy")]);
    assert!(moved.removed.is_empty());
    assert_eq!(targets(&workspace), [target(None, ".agents/skills/deploy")]);
    assert_eq!(excluded(&project), ["/.agents/skills/deploy/"]);
}

#[test]
fn turning_shared_removes_the_gitignore_and_every_exclude_line_of_the_project() {
    let (_dir, workspace, project) = installed(&Agent::ALL, Share::Local);
    let ignore = project.root().join(MIND_DIR).join(".gitignore");
    assert!(ignore.is_file());

    let change = Change::Share {
        share: Share::Shared,
        edit_tracked: false,
    };
    let plan = reapply::plan(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap();
    assert_eq!(plan.ignore, Some(Ignore::Remove(ignore.clone())));
    assert_eq!(
        plan.unexcluded,
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
    assert_eq!(
        plan.exclude_file,
        Some(project.root().join(".git").join("info").join("exclude"))
    );

    let applied =
        reapply::apply(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap();
    assert!(applied.failures.is_empty(), "{:?}", applied.failures);
    assert!(!ignore.exists());
    assert!(excluded(&project).is_empty());
    assert_eq!(reopened(&project).settings().unwrap().share, Share::Shared);
    // The copies stay, for the team to commit.
    assert!(claudes(&project, "deploy").join("SKILL.md").is_file());
}

#[test]
fn turning_local_writes_the_gitignore_and_keeps_every_copy_in_place_out_of_git() {
    let (_dir, workspace, project) = installed(&Agent::ALL, Share::Shared);
    let ignore = project.root().join(MIND_DIR).join(".gitignore");
    assert!(!ignore.exists());

    let change = Change::Share {
        share: Share::Local,
        edit_tracked: true,
    };
    let plan = reapply::plan(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap();
    assert_eq!(plan.ignore, Some(Ignore::Write(ignore.clone())));
    assert_eq!(
        plan.excluded,
        [agents(&project, "deploy"), claudes(&project, "deploy")]
    );

    reapply::apply(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap();
    assert_eq!(fs::read_to_string(&ignore).unwrap(), MIND_IGNORE);
    assert_eq!(
        excluded(&project),
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
    let settings = reopened(&project).settings().unwrap().clone();
    assert_eq!(settings.share, Share::Local);
    assert!(settings.edit_tracked);
}

#[test]
fn a_gitignore_somebody_changed_is_left_when_the_project_turns_shared() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let ignore = project.root().join(MIND_DIR).join(".gitignore");
    fs::write(&ignore, "*\n!settings.json\n").unwrap();

    let change = Change::Share {
        share: Share::Shared,
        edit_tracked: false,
    };
    let applied =
        reapply::apply(Some(&workspace), &project, &change, &AgentHomes::default()).unwrap();

    assert_eq!(applied.plan.ignore, Some(Ignore::Kept(ignore.clone())));
    assert_eq!(fs::read_to_string(&ignore).unwrap(), "*\n!settings.json\n");
}

#[test]
fn a_project_no_workspace_links_changes_its_settings_and_moves_no_copy() {
    let (_dir, _workspace, project) = installed(&[Agent::Claude], Share::Local);

    let applied = reapply::apply(
        None,
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(!applied.plan.recorded);
    assert!(applied.plan.installations.is_empty());
    assert!(!agents(&project, "deploy").exists());
    assert_eq!(
        reopened(&project).settings().unwrap().agents,
        [Agent::Claude, Agent::Codex]
    );
}

#[test]
fn a_change_that_changes_nothing_writes_nothing() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let settings = settings_file(&project);
    let modified = fs::metadata(&settings).unwrap().modified().unwrap();

    let applied = reapply::apply(
        Some(&workspace),
        &project,
        &Change::AddAgents(vec![Agent::Claude]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(applied.plan.is_empty(), "{:?}", applied.plan);
    assert_eq!(
        fs::metadata(&settings).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn a_copy_that_would_land_in_an_agents_own_folder_is_refused_with_nothing_written() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    let settings = fs::read(settings_file(&project)).unwrap();
    let homes = AgentHomes {
        agents_dir: Some(project.root().join(".agents")),
        ..AgentHomes::default()
    };

    let error = reapply::apply(
        Some(&workspace),
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &homes,
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            ReapplyError::Install(install::InstallError::UserScope { .. })
        ),
        "{error}"
    );
    assert_eq!(fs::read(settings_file(&project)).unwrap(), settings);
    assert!(!agents(&project, "deploy").exists());
}

#[test]
fn the_record_of_another_workspace_is_never_read_or_written() {
    let (_dir, workspace, project) = installed(&[Agent::Claude], Share::Local);
    // A ledger that names nothing in the project stands for a workspace that
    // installed nothing there.
    let elsewhere = TempDir::new().unwrap();
    let (other, _) = FlayerWorkspace::init(elsewhere.path()).unwrap();
    Ledger::open(other.ledger_path()).unwrap();

    let plan = reapply::plan(
        Some(&other),
        &project,
        &Change::AddAgents(vec![Agent::Codex]),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(!plan.recorded);
    assert!(plan.installations.is_empty());
    assert_eq!(targets(&workspace).len(), 1);
}
