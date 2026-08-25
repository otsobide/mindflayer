//! The real command surface, driven through the real clap parser.

use std::fs;
use std::path::Path;

use clap::Parser;
use mindflayer_cli::{run, Cli, CliError, Outcome};
use mindflayer_core::{FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR};
use tempfile::TempDir;

/// Parse and run a command line, with `-C dir` appended.
fn mind(dir: &Path, args: &[&str]) -> Result<Outcome, CliError> {
    let mut line: Vec<String> = vec!["mind".to_owned()];
    line.extend(args.iter().map(|arg| (*arg).to_owned()));
    line.push("-C".to_owned());
    line.push(dir.to_string_lossy().into_owned());
    run(&Cli::parse_from(line))
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

#[test]
fn init_creates_a_mind_project_by_default() {
    let dir = TempDir::new().unwrap();

    let outcome = mind(dir.path(), &["init"]).unwrap();

    assert!(dir.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(dir.path().join(MIND_DIR).join("skills").is_dir());
    assert!(!dir.path().join(FLAYER_DIR).exists(), "no workspace here");
    assert!(outcome.ok);
    assert!(
        outcome.stdout.starts_with("initialized mind project"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn init_mind_is_the_same_as_init() {
    let bare = TempDir::new().unwrap();
    let named = TempDir::new().unwrap();

    mind(bare.path(), &["init"]).unwrap();
    mind(named.path(), &["init", "mind"]).unwrap();

    let one = fs::read_to_string(bare.path().join(MIND_DIR).join(MIND_CONFIG)).unwrap();
    let two = fs::read_to_string(named.path().join(MIND_DIR).join(MIND_CONFIG)).unwrap();
    // Only the name differs, and it comes from the temporary directory.
    assert_eq!(one.lines().count(), two.lines().count());
}

#[test]
fn init_flayer_creates_a_workspace_with_an_empty_registry() {
    let dir = TempDir::new().unwrap();

    let outcome = mind(dir.path(), &["init", "flayer"]).unwrap();

    assert!(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
    assert!(!dir.path().join(MIND_DIR).exists(), "no project here");
    assert!(outcome.stdout.starts_with("initialized flayer workspace"));
}

#[test]
fn init_run_twice_says_so_and_succeeds() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();

    let outcome = mind(dir.path(), &["init", "mind"]).unwrap();

    assert!(outcome.ok);
    assert!(
        outcome.stdout.contains("already initialized"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn init_rejects_a_kind_that_is_neither() {
    let parsed = Cli::try_parse_from(["mind", "init", "brain"]);

    assert!(parsed.is_err());
}

#[test]
fn listing_outside_a_project_explains_what_to_run() {
    let dir = TempDir::new().unwrap();

    let error = mind(dir.path(), &["list"]).unwrap_err();

    assert!(matches!(error, CliError::Nowhere(_)));
    // Both ways out are named, because from an empty directory either could be
    // the one that was meant.
    assert!(error.to_string().contains("mind init"));
    assert!(error.to_string().contains("mind init flayer"));
}

#[test]
fn a_fresh_project_lists_no_skills_and_says_where_it_looked() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.starts_with("no skills found"));
    assert!(outcome.stdout.contains("skills"));
}

#[test]
fn listing_shows_the_project_the_name_and_the_description() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(
        dir.path(),
        "deploy",
        &skill_file("deploy", "Ship the service to staging"),
    );

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stderr.is_empty());
    assert!(
        outcome.stdout.contains("deploy") && outcome.stdout.contains("Ship the service to staging"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn a_workspace_lists_the_skills_of_every_project_it_references() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "flayer"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        mind(&root, &["init", "mind"]).unwrap();
        write_skill(&root, name, &skill_file(name, "A skill"));
    }
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"beta\"]\n",
    )
    .unwrap();

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert_eq!(outcome.stdout.lines().count(), 2, "{}", outcome.stdout);
    assert!(outcome.stdout.contains("alpha"));
    assert!(outcome.stdout.contains("beta"));
}

#[test]
fn a_stale_workspace_reference_is_a_warning_not_a_dead_end() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "flayer"]).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    mind(&root, &["init", "mind"]).unwrap();
    write_skill(&root, "alpha", &skill_file("alpha", "A skill"));
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"moved-away\"]\n",
    )
    .unwrap();

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.contains("alpha"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("moved-away"));
    assert!(!outcome.ok, "a warning is still a non-zero exit");
}

#[test]
fn a_project_is_found_from_a_subdirectory() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    let outcome = mind(&deep, &["list"]).unwrap();

    assert!(outcome.stdout.contains("deploy"));
}

#[test]
fn show_prints_the_metadata_and_the_instructions() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nallowed-tools: Bash, Read\nlicense: MIT\n---\n\nRun the pipeline.\n",
    );

    let outcome = mind(dir.path(), &["show", "deploy"]).unwrap();

    assert!(outcome.stdout.contains("deploy ("));
    assert!(outcome.stdout.contains("SKILL.md"));
    assert!(outcome.stdout.contains("Ship it"));
    assert!(outcome.stdout.contains("allowed-tools: Bash, Read"));
    assert!(outcome.stdout.contains("license: MIT"));
    assert!(outcome.stdout.contains("Run the pipeline."));
}

#[test]
fn show_names_the_skill_it_could_not_find() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();

    let error = mind(dir.path(), &["show", "absent"]).unwrap_err();

    assert!(matches!(error, CliError::UnknownSkill(name) if name == "absent"));
}

#[test]
fn validate_passes_a_well_formed_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let outcome = mind(dir.path(), &["validate"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("deploy"));
    assert!(outcome.stdout.contains("ok"));
    assert!(outcome.stdout.contains("1 skill checked, 0 invalid"));
}

#[test]
fn validate_fails_and_explains_a_broken_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
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
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "bad", &skill_file("Bad", "Also fine"));

    let outcome = mind(dir.path(), &["validate", "good"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("1 skill checked, 0 invalid"));
}

#[test]
fn an_unreadable_skill_is_a_warning_on_stderr() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn ls_is_an_alias_for_list() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "mind"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let listed = mind(dir.path(), &["list"]).unwrap();
    let aliased = mind(dir.path(), &["ls"]).unwrap();

    assert_eq!(listed, aliased);
}
