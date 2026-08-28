//! Creating, finding and reading mind projects and flayer workspaces.

use std::fs;

use mindflayer_core::workspace::FORMAT_VERSION;
use mindflayer_core::{
    Directories, FlayerWorkspace, Initialization, Kind, MindProject, Registration, WorkspaceError,
    FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR,
};
use tempfile::TempDir;

#[test]
fn initializing_a_mind_project_creates_its_marker_and_its_skills_folder() {
    let dir = TempDir::new().unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(project.directory_for(Kind::Skill).is_dir());
    assert_eq!(project.root(), dir.path());
    assert_eq!(project.config().version, FORMAT_VERSION);
}

#[test]
fn a_new_project_is_named_after_its_directory() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();

    let (project, _) = MindProject::init(&root).unwrap();

    assert_eq!(project.name(), "collapse");
}

#[test]
fn initializing_twice_never_rewrites_the_marker() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let marker = dir.path().join(MIND_DIR).join(MIND_CONFIG);
    fs::write(&marker, "version = 1\nname = \"renamed by hand\"\n").unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert_eq!(project.name(), "renamed by hand");
}

#[test]
fn initializing_fills_in_a_skills_folder_someone_deleted() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    fs::remove_dir(project.directory_for(Kind::Skill)).unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert!(project.directory_for(Kind::Skill).is_dir());
}

#[test]
fn a_project_is_found_from_any_directory_below_it() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    let found = MindProject::locate(&deep)
        .unwrap()
        .expect("a project above");

    assert_eq!(found.root(), dir.path());
}

#[test]
fn locate_finds_nothing_when_there_is_no_project() {
    let dir = TempDir::new().unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
    assert!(FlayerWorkspace::locate(dir.path()).unwrap().is_none());
}

#[test]
fn opening_a_directory_that_is_not_a_project_says_so() {
    let dir = TempDir::new().unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotAProject { .. }));
    assert!(error.to_string().contains(MIND_DIR));
}

#[test]
fn an_empty_mind_directory_is_not_a_project() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
}

#[test]
fn a_marker_from_a_newer_mindflayer_is_refused() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "version = 99\nname = \"future\"\n",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { found: 99, .. }));
}

#[test]
fn a_corrupt_marker_is_reported_as_such() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "not = toml = at all",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Parse { .. }));
}

#[test]
fn initializing_a_workspace_creates_an_empty_registry() {
    let dir = TempDir::new().unwrap();

    let (workspace, outcome) = FlayerWorkspace::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
    assert!(workspace.config().projects.is_empty());
    assert_eq!(workspace.projects().0, vec![]);
}

#[test]
fn a_workspace_opens_the_projects_it_references() {
    let dir = TempDir::new().unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        MindProject::init(&root).unwrap();
    }
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"beta\"]\n",
    )
    .unwrap();

    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert!(failures.is_empty());
    let names: Vec<&str> = projects.iter().map(MindProject::name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn a_stale_reference_is_reported_without_losing_the_rest() {
    let dir = TempDir::new().unwrap();
    FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"moved-away\"]\n",
    )
    .unwrap();

    let workspace = FlayerWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert_eq!(projects.len(), 1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].to_string().contains("moved-away"));
}

#[test]
fn one_directory_can_be_both_a_workspace_and_a_project() {
    let dir = TempDir::new().unwrap();

    FlayerWorkspace::init(dir.path()).unwrap();
    MindProject::init(dir.path()).unwrap();

    assert!(FlayerWorkspace::locate(dir.path()).unwrap().is_some());
    assert!(MindProject::locate(dir.path()).unwrap().is_some());
}

// ---------------------------------------------------------------------------
// Registering projects: link and unlink
// ---------------------------------------------------------------------------

/// A workspace with `count` mind projects beside it, none of them registered.
fn workspace_with(count: usize) -> (TempDir, FlayerWorkspace, Vec<MindProject>) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let projects = ["alpha", "beta", "gamma"]
        .iter()
        .take(count)
        .map(|name| {
            let root = dir.path().join(name);
            fs::create_dir(&root).unwrap();
            MindProject::init(&root).unwrap().0
        })
        .collect();
    (dir, workspace, projects)
}

fn config_text(workspace: &FlayerWorkspace) -> String {
    fs::read_to_string(workspace.config_path()).unwrap()
}

#[test]
fn link_registers_a_project_relative_to_the_workspace() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    let (entry, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::Added);
    assert_eq!(entry, std::path::Path::new("alpha"));
    assert_eq!(workspace.config().projects, vec![entry]);
    assert!(config_text(&workspace).contains("projects = [\"alpha\"]"));
}

#[test]
fn link_keeps_the_comments_that_explain_the_file() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    let before = config_text(&workspace);
    let comments: Vec<&str> = before.lines().filter(|l| l.starts_with('#')).collect();
    assert!(!comments.is_empty(), "the template ships with comments");

    workspace.link(&projects[0]).unwrap();

    let after = config_text(&workspace);
    for comment in &comments {
        assert!(
            after.contains(comment),
            "editing dropped the comment {comment:?}:\n{after}"
        );
    }
    // And the keys it was not asked to touch.
    assert!(after.contains(&format!("version = {FORMAT_VERSION}")));
    assert!(after.contains(&format!("name = \"{}\"", workspace.name())));
}

#[test]
fn linking_the_same_project_twice_changes_nothing() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let after_first = config_text(&workspace);

    let (_, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(workspace.config().projects.len(), 1);
    assert_eq!(
        config_text(&workspace),
        after_first,
        "the file was rewritten"
    );
}

#[test]
fn link_matches_on_where_an_entry_points_not_how_it_is_spelled() {
    let (dir, mut workspace, _) = workspace_with(0);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();

    // The same directory, reached by a path nobody would normalise by hand.
    let awkward = MindProject::open(dir.path().join("./beta/../alpha")).unwrap();
    workspace.link(&awkward).unwrap();
    let (_, outcome) = workspace.link(&MindProject::open(&root).unwrap()).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(workspace.config().projects.len(), 1);
}

#[test]
fn a_project_outside_the_workspace_is_stored_as_a_route_out_of_it() {
    let outer = TempDir::new().unwrap();
    let inner = outer.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(&inner).unwrap();
    let sibling = outer.path().join("collapse");
    fs::create_dir(&sibling).unwrap();
    let (project, _) = MindProject::init(&sibling).unwrap();

    let (entry, _) = workspace.link(&project).unwrap();

    assert_eq!(entry, std::path::Path::new("../collapse"));
    let (opened, failures) = workspace.projects();
    assert!(failures.is_empty());
    assert_eq!(opened[0].root(), project.root());
}

#[test]
fn a_linked_project_is_one_the_workspace_can_open() {
    let (_dir, mut workspace, projects) = workspace_with(2);
    workspace.link(&projects[0]).unwrap();
    workspace.link(&projects[1]).unwrap();

    let (opened, failures) = workspace.projects();

    assert!(failures.is_empty());
    let names: Vec<&str> = opened.iter().map(MindProject::name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn unlink_removes_only_the_entry_it_was_given() {
    let (_dir, mut workspace, projects) = workspace_with(2);
    workspace.link(&projects[0]).unwrap();
    workspace.link(&projects[1]).unwrap();

    let removed = workspace.unlink(projects[0].root()).unwrap();

    assert_eq!(removed, vec![std::path::PathBuf::from("alpha")]);
    assert_eq!(
        workspace.config().projects,
        vec![std::path::PathBuf::from("beta")]
    );
    assert!(config_text(&workspace).contains("# Mindflayer workspace."));
}

#[test]
fn unlink_says_so_when_the_project_was_never_registered() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    let error = workspace.unlink(projects[0].root()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotRegistered { .. }));
}

#[test]
fn unlink_works_on_an_entry_whose_directory_has_gone() {
    let (dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();

    // The stale entry is exactly the one worth removing, and it can no longer
    // be opened as a project.
    let removed = workspace.unlink(&dir.path().join("alpha")).unwrap();

    assert_eq!(removed, vec![std::path::PathBuf::from("alpha")]);
    assert!(workspace.config().projects.is_empty());
}

#[test]
fn link_restores_a_projects_key_someone_deleted() {
    let (_dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "# kept\nversion = 1\nname = \"work\"\n",
    )
    .unwrap();
    let mut workspace = FlayerWorkspace::open(workspace.root()).unwrap();

    workspace.link(&projects[0]).unwrap();

    assert_eq!(workspace.config().projects.len(), 1);
    assert!(config_text(&workspace).contains("# kept"));
}

#[test]
fn linking_leaves_no_temporary_file_behind() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    workspace.link(&projects[0]).unwrap();

    let leftovers: Vec<_> = fs::read_dir(workspace.flayer_dir())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "left behind {leftovers:?}");
}

#[test]
fn a_root_is_normalized_so_two_spellings_of_one_directory_are_one_project() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    fs::create_dir(dir.path().join("beta")).unwrap();
    MindProject::init(&root).unwrap();

    let direct = MindProject::open(&root).unwrap();
    let roundabout = MindProject::open(dir.path().join("beta/../alpha")).unwrap();

    // Not cosmetic: a workspace resolves `../collapse` by joining it onto its
    // own root, so without this every such project would carry a `..` and stop
    // comparing equal to itself reached any other way.
    assert_eq!(direct.root(), roundabout.root());
    assert!(!roundabout.root().to_string_lossy().contains(".."));
}

#[test]
fn unlink_removes_every_spelling_that_pointed_at_the_project() {
    let (dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "version = 1\nname = \"w\"\nprojects = [\"alpha\", \"./alpha\", \"./alpha/\"]\n",
    )
    .unwrap();
    let mut workspace = FlayerWorkspace::open(workspace.root()).unwrap();
    let _ = (&dir, &projects);

    let removed = workspace.unlink(projects[0].root()).unwrap();

    // Removing one of three and reporting success would leave the project
    // registered while telling the user it is gone.
    assert_eq!(removed.len(), 3);
    assert!(workspace.config().projects.is_empty());
}

#[test]
fn an_already_registered_link_reports_the_spelling_in_the_file() {
    let (_dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "version = 1\nname = \"w\"\nprojects = [\"./alpha/\"]\n",
    )
    .unwrap();
    let mut workspace = FlayerWorkspace::open(workspace.root()).unwrap();

    let (entry, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(entry, std::path::Path::new("./alpha/"), "not a fresh guess");
}

#[test]
fn a_link_that_changes_nothing_does_not_rewrite_the_file() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let before = fs::metadata(workspace.config_path())
        .unwrap()
        .modified()
        .unwrap();

    workspace.link(&projects[0]).unwrap();

    let after = fs::metadata(workspace.config_path())
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "an idempotent link touched the file");
}

#[test]
fn a_failed_unlink_leaves_the_file_alone() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let before = fs::read_to_string(workspace.config_path()).unwrap();

    let error = workspace
        .unlink(std::path::Path::new("/nowhere"))
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::NotRegistered { .. }));
    assert_eq!(fs::read_to_string(workspace.config_path()).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn an_entry_is_only_stored_as_a_route_when_the_route_really_resolves() {
    use std::os::unix::fs::symlink;

    // `/tmp` is a symlink to `/private/tmp` on every Mac, so a workspace
    // reached through one is not an exotic case. A lexical `..` climbs out of
    // the link's target, not out of the directory the name suggests.
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("real/deep/ws")).unwrap();
    fs::create_dir(dir.path().join("real/sibling")).unwrap();
    symlink("real/deep", dir.path().join("link")).unwrap();

    let (mut workspace, _) = FlayerWorkspace::init(dir.path().join("link/ws")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("real/sibling")).unwrap();

    let (entry, _) = workspace.link(&project).unwrap();

    let (opened, failures) = workspace.projects();
    assert!(
        failures.is_empty(),
        "the entry it just wrote does not open: {failures:?}"
    );
    assert_eq!(opened.len(), 1);
    // Read back through the directory's real spelling too, which is what any
    // later `cd` into it will use.
    let real = FlayerWorkspace::open(dir.path().join("real/deep/ws")).unwrap();
    assert_eq!(
        real.projects().0.len(),
        1,
        "entry {} does not resolve",
        entry.display()
    );
}

#[cfg(unix)]
#[test]
fn rewriting_the_config_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, mut workspace, projects) = workspace_with(1);
    let path = workspace.config_path();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    workspace.link(&projects[0]).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "a config chmodded to 600 came back {mode:o}");
}

// ---------------------------------------------------------------------------
// Where a project keeps its artifacts
// ---------------------------------------------------------------------------

#[test]
fn a_new_project_keeps_its_artifacts_beside_the_code() {
    let dir = TempDir::new().unwrap();

    let (project, _) = MindProject::init(dir.path()).unwrap();

    // Beside the code, not inside `.mind`: these are files the agents
    // themselves read, and an agent does not know what a `.mind` is.
    assert_eq!(
        project.directory_for(Kind::Skill),
        dir.path().join("skills")
    );
    assert_eq!(project.directory_for(Kind::Rule), dir.path().join("rules"));
    assert!(dir.path().join("skills").is_dir());
    assert!(dir.path().join("rules").is_dir());
}

#[test]
fn the_marker_spells_out_where_each_kind_goes() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let written = fs::read_to_string(dir.path().join(MIND_DIR).join(MIND_CONFIG)).unwrap();

    // In the file, not left to a default somebody has to go and read.
    assert!(written.contains("[directories]"), "{written}");
    assert!(written.contains(r#"skills = "skills""#), "{written}");
    assert!(written.contains(r#"rules = "rules""#), "{written}");
}

#[test]
fn a_project_can_keep_its_skills_where_its_agents_look() {
    let dir = TempDir::new().unwrap();
    let directories = Directories::default().with(Kind::Skill, ".claude/skills");

    let (project, _) = MindProject::init_with(dir.path(), &directories).unwrap();

    assert_eq!(
        project.directory_for(Kind::Skill),
        dir.path().join(".claude/skills")
    );
    assert!(dir.path().join(".claude").join("skills").is_dir());
    // The kind that was not mentioned keeps its default.
    assert_eq!(project.directory_for(Kind::Rule), dir.path().join("rules"));
}

#[test]
fn where_a_project_keeps_things_survives_reopening_it() {
    let dir = TempDir::new().unwrap();
    let directories = Directories::default().with(Kind::Skill, "docs/skills");
    MindProject::init_with(dir.path(), &directories).unwrap();

    let reopened = MindProject::open(dir.path()).unwrap();

    assert_eq!(
        reopened.directory_for(Kind::Skill),
        dir.path().join("docs/skills")
    );
}

#[test]
fn a_second_init_does_not_move_where_a_project_keeps_things() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let (project, outcome) = MindProject::init_with(
        dir.path(),
        &Directories::default().with(Kind::Skill, "elsewhere"),
    )
    .unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    // The marker already answered the question, and `init` never overwrites it.
    assert_eq!(
        project.directory_for(Kind::Skill),
        dir.path().join("skills")
    );
    assert!(!dir.path().join("elsewhere").exists());
}

#[test]
fn a_directory_outside_the_project_is_refused_before_anything_is_made() {
    let dir = TempDir::new().unwrap();

    for outside in ["/etc/skills", "../elsewhere"] {
        let directories = Directories::default().with(Kind::Skill, outside);
        let error = MindProject::init_with(dir.path(), &directories).unwrap_err();

        assert!(
            matches!(error, WorkspaceError::OutsideProject { .. }),
            "{outside}: {error}"
        );
    }
    // Refused before anything was written, not halfway through.
    assert!(!dir.path().join(MIND_DIR).exists());
}

#[test]
fn a_marker_written_before_directories_existed_is_read_the_way_it_was_written() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(MIND_DIR).join("skills")).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "version = 1\nname = \"collapse\"\n",
    )
    .unwrap();

    let project = MindProject::open(dir.path()).unwrap();

    // Today's default would point it at a directory it has never had, and it
    // would list nothing without saying why.
    assert_eq!(
        project.directory_for(Kind::Skill),
        dir.path().join(MIND_DIR).join("skills")
    );
}

#[test]
fn a_marker_from_the_future_is_still_refused() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(MIND_DIR)).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        format!("version = {}\nname = \"x\"\n", FORMAT_VERSION + 1),
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { .. }), "{error}");
}
