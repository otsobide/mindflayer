//! The real command surface, driven through the real clap parsers.

use std::fs;
use std::path::Path;

use clap::Parser;
use mindflayer_cli::{run, run_flayer_cli, Cli, CliError, FlayerCli, Outcome};
use mindflayer_core::{FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR};
use tempfile::TempDir;

/// Build an argument line with `-C dir` appended.
fn line(binary: &str, args: &[&str], dir: &Path) -> Vec<String> {
    let mut line: Vec<String> = vec![binary.to_owned()];
    line.extend(args.iter().map(|arg| (*arg).to_owned()));
    line.push("-C".to_owned());
    line.push(dir.to_string_lossy().into_owned());
    line
}

/// Run `mind ...`.
///
/// `try_parse_from` rather than `parse_from`: the latter exits the process on a
/// parse error, which in a test harness kills the whole run and hides which
/// argument line was at fault.
fn mind(dir: &Path, args: &[&str]) -> Result<Outcome, CliError> {
    let line = line("mind", args, dir);
    let cli = Cli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run(&cli)
}

/// Run `flayer ...`, through the second binary's parser.
fn flayer(dir: &Path, args: &[&str]) -> Result<Outcome, CliError> {
    let line = line("flayer", args, dir);
    let cli = FlayerCli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run_flayer_cli(&cli)
}

/// Write a skill into an already initialized mind project.
fn write_skill(root: &Path, directory: &str, contents: &str) {
    let dir = root.join(MIND_DIR).join("skills").join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

fn skill_file(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
}

/// A workspace with `alpha` and `beta` linked, each holding one skill.
fn workspace_with_two() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        mind(&root, &["init"]).unwrap();
        write_skill(&root, name, &skill_file(name, "A skill"));
        flayer(dir.path(), &["link", name]).unwrap();
    }
    dir
}

// ---------------------------------------------------------------------------
// The two levels
// ---------------------------------------------------------------------------

#[test]
fn mind_init_creates_a_project_and_flayer_init_a_workspace() {
    let project = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let one = mind(project.path(), &["init"]).unwrap();
    let two = flayer(workspace.path(), &["init"]).unwrap();

    assert!(project.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(project.path().join(MIND_DIR).join("skills").is_dir());
    assert!(!project.path().join(FLAYER_DIR).exists());
    assert!(one.stdout.starts_with("initialized mind project"));

    assert!(workspace
        .path()
        .join(FLAYER_DIR)
        .join(FLAYER_CONFIG)
        .is_file());
    assert!(!workspace.path().join(MIND_DIR).exists());
    assert!(two.stdout.starts_with("initialized flayer workspace"));
}

#[test]
fn flayer_is_a_shortcut_for_mind_flayer() {
    let dir = workspace_with_two();

    for args in [
        vec!["list"],
        vec!["validate"],
        vec!["show", "alpha"],
        vec!["link", "alpha"],
    ] {
        let direct = flayer(dir.path(), &args).unwrap();
        let nested = {
            let mut nested = vec!["flayer"];
            nested.extend(args.iter().copied());
            mind(dir.path(), &nested).unwrap()
        };
        assert_eq!(
            direct, nested,
            "`flayer {args:?}` differs from `mind flayer`"
        );
    }
}

#[test]
fn mind_init_no_longer_takes_a_kind() {
    // The old surface was `mind init flayer`; that lives at `flayer init` now,
    // and leaving both spellings alive would be two ways to do one thing.
    assert!(Cli::try_parse_from(["mind", "init", "flayer"]).is_err());
    assert!(Cli::try_parse_from(["mind", "init", "mind"]).is_err());
}

#[test]
fn mind_list_sees_only_its_own_project_inside_a_workspace() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project_level = mind(&alpha, &["list"]).unwrap();
    let workspace_level = flayer(&alpha, &["list"]).unwrap();

    assert_eq!(project_level.stdout.lines().count(), 1);
    assert!(project_level.stdout.contains("alpha"));
    assert!(
        !project_level.stdout.contains("beta"),
        "the project level leaked its neighbour:\n{}",
        project_level.stdout
    );
    assert_eq!(workspace_level.stdout.lines().count(), 2);
    assert!(workspace_level.stdout.contains("beta"));
}

#[test]
fn only_the_workspace_level_names_the_project_each_skill_came_from() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project = mind(&alpha, &["validate"]).unwrap();
    let workspace = flayer(&alpha, &["validate"]).unwrap();

    assert!(project.stdout.contains("alpha: ok"), "{}", project.stdout);
    assert!(
        workspace.stdout.contains("alpha (alpha): ok"),
        "{}",
        workspace.stdout
    );
}

#[test]
fn each_level_names_the_command_that_creates_what_is_missing() {
    let dir = TempDir::new().unwrap();

    let project = mind(dir.path(), &["list"]).unwrap_err();
    let workspace = flayer(dir.path(), &["list"]).unwrap_err();

    assert!(matches!(project, CliError::NotInProject(_)));
    assert!(project.to_string().contains("mind init"));
    assert!(matches!(workspace, CliError::NotInWorkspace(_)));
    assert!(workspace.to_string().contains("flayer init"));
}

#[test]
fn ls_is_an_alias_at_both_levels() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    assert_eq!(
        mind(&alpha, &["list"]).unwrap(),
        mind(&alpha, &["ls"]).unwrap()
    );
    assert_eq!(
        flayer(dir.path(), &["list"]).unwrap(),
        flayer(dir.path(), &["ls"]).unwrap()
    );
}

// ---------------------------------------------------------------------------
// link and unlink
// ---------------------------------------------------------------------------

#[test]
fn link_registers_a_project_and_says_how_it_was_stored() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    mind(&root, &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["link", "alpha"]).unwrap();

    assert!(outcome.ok);
    assert_eq!(outcome.stdout, "linked alpha as alpha\n");
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("projects = [\"alpha\"]"), "{config}");
    assert!(config.contains("# Mindflayer workspace."), "comments kept");
}

#[test]
fn a_fresh_workspace_says_it_manages_nothing_yet() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("manages no projects yet"));
    assert!(outcome.stdout.contains("flayer link"));
}

#[test]
fn linking_the_same_project_twice_is_not_an_error() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["link", "alpha"]).unwrap();

    assert!(outcome.ok);
    assert_eq!(outcome.stdout, "alpha is already linked as alpha\n");
    let config = fs::read_to_string(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert_eq!(config.matches("\"alpha\"").count(), 1, "{config}");
}

#[test]
fn link_refuses_a_directory_that_is_not_a_mind_project() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    fs::create_dir(dir.path().join("not-a-project")).unwrap();

    let error = flayer(dir.path(), &["link", "not-a-project"]).unwrap_err();

    assert!(error.to_string().contains(MIND_DIR), "{error}");
}

#[test]
fn link_resolves_the_path_against_the_directory_it_runs_in() {
    let dir = workspace_with_two();
    let deep = dir.path().join("alpha/nested/deeper");
    fs::create_dir_all(&deep).unwrap();
    let gamma = dir.path().join("gamma");
    fs::create_dir(&gamma).unwrap();
    mind(&gamma, &["init"]).unwrap();

    // Typed relative to where the command runs, stored relative to the
    // workspace, which is three directories up.
    let outcome = flayer(&deep, &["link", "../../../gamma"]).unwrap();

    assert_eq!(outcome.stdout, "linked gamma as gamma\n");
    assert_eq!(
        flayer(dir.path(), &["list"])
            .unwrap()
            .stdout
            .lines()
            .count(),
        2
    );
}

#[test]
fn a_project_outside_the_workspace_is_stored_as_a_route_out() {
    let outer = TempDir::new().unwrap();
    let inner = outer.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    flayer(&inner, &["init"]).unwrap();
    let sibling = outer.path().join("collapse");
    fs::create_dir(&sibling).unwrap();
    mind(&sibling, &["init"]).unwrap();
    write_skill(&sibling, "deploy", &skill_file("deploy", "Ship it"));

    let outcome = flayer(&inner, &["link", "../collapse"]).unwrap();

    assert_eq!(outcome.stdout, "linked collapse as ../collapse\n");
    assert!(flayer(&inner, &["list"]).unwrap().stdout.contains("deploy"));
}

#[test]
fn unlink_removes_one_entry_and_leaves_the_rest() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\n");
    let listed = flayer(dir.path(), &["list"]).unwrap();
    assert!(listed.stdout.contains("beta"));
    assert!(!listed.stdout.contains("alpha"));
}

#[test]
fn unlink_says_so_when_nothing_was_registered_under_that_path() {
    let dir = workspace_with_two();

    let error = flayer(dir.path(), &["unlink", "gamma"]).unwrap_err();

    assert!(error.to_string().contains("not registered"), "{error}");
}

#[test]
fn unlink_works_on_a_project_whose_directory_has_gone() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    // The stale entry is exactly the one worth removing, and it warns until
    // it is gone.
    assert!(!flayer(dir.path(), &["list"]).unwrap().ok);

    let outcome = flayer(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\n");
    assert!(flayer(dir.path(), &["list"]).unwrap().ok);
}

#[test]
fn link_needs_a_workspace_not_just_a_project() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let error = flayer(dir.path(), &["link", "."]).unwrap_err();

    assert!(matches!(error, CliError::NotInWorkspace(_)));
}

#[test]
fn a_workspace_can_manage_the_project_it_sits_in() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let outcome = flayer(dir.path(), &["link", "."]).unwrap();

    assert!(outcome.stdout.starts_with("linked"));
    assert!(flayer(dir.path(), &["list"])
        .unwrap()
        .stdout
        .contains("deploy"));
}

// ---------------------------------------------------------------------------
// Reading skills, at whichever level
// ---------------------------------------------------------------------------

#[test]
fn show_prints_the_metadata_and_the_instructions() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nallowed-tools: Bash, Read\nlicense: MIT\n---\n\nRun the pipeline.\n",
    );

    let outcome = mind(dir.path(), &["show", "deploy"]).unwrap();

    assert!(outcome.stdout.starts_with("deploy\n"));
    assert!(outcome.stdout.contains("SKILL.md"));
    assert!(outcome.stdout.contains("Ship it"));
    assert!(outcome.stdout.contains("allowed-tools: Bash, Read"));
    assert!(outcome.stdout.contains("license: MIT"));
    assert!(outcome.stdout.contains("Run the pipeline."));
}

#[test]
fn show_names_the_skill_it_could_not_find() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let error = mind(dir.path(), &["show", "absent"]).unwrap_err();

    assert!(matches!(error, CliError::UnknownSkill(name) if name == "absent"));
}

#[test]
fn the_workspace_shows_both_projects_declaring_one_name() {
    let dir = workspace_with_two();
    // Give beta a skill named like alpha's, which is legal across projects.
    write_skill(
        &dir.path().join("beta"),
        "alpha",
        &skill_file("alpha", "The other one"),
    );

    let outcome = flayer(dir.path(), &["show", "alpha"]).unwrap();

    assert!(outcome.stdout.contains("alpha (alpha)"));
    assert!(outcome.stdout.contains("alpha (beta)"));
    assert!(outcome.stdout.contains("\n---\n"), "separated");
}

#[test]
fn validate_fails_and_explains_a_broken_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("Deployment", "Ship it"));

    let outcome = mind(dir.path(), &["validate"]).unwrap();

    assert!(!outcome.ok);
    assert!(outcome.stdout.contains("2 problems"), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("the directory is `deploy`"));
    assert!(outcome.stdout.contains("1 skill checked, 1 invalid"));
}

#[test]
fn validate_can_be_pointed_at_one_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "bad", &skill_file("Bad", "Also fine"));

    let outcome = mind(dir.path(), &["validate", "good"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("1 skill checked, 0 invalid"));
}

#[test]
fn an_unreadable_skill_is_a_warning_on_stderr() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn a_project_is_found_from_a_subdirectory() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    assert!(mind(&deep, &["list"]).unwrap().stdout.contains("deploy"));
}

#[test]
fn init_run_twice_says_so_and_succeeds() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let project = mind(dir.path(), &["init"]).unwrap();
    let workspace = flayer(dir.path(), &["init"]).unwrap();

    assert!(project.ok && workspace.ok);
    assert!(project.stdout.contains("already initialized"));
    assert!(workspace.stdout.contains("already initialized"));
}
