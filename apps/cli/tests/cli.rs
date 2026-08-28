//! The real command surface, driven through the real clap parsers.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use mindflayer_cli::{run, run_flayer_cli, Cli, CliError, Failure, FlayerCli, Outcome};
use mindflayer_core::{Kind, MindProject, FLAYER_CONFIG, FLAYER_DIR, MIND_CONFIG, MIND_DIR};
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
fn mind(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let line = line("mind", args, dir);
    let cli = Cli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run(&cli)
}

/// Run `flayer ...`, through the second binary's parser.
fn flayer(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let line = line("flayer", args, dir);
    let cli = FlayerCli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
    run_flayer_cli(&cli)
}

/// Write a skill into an already initialized mind project.
/// Where an initialized project keeps a kind, asked of the project itself
/// rather than assumed, so a test that moves one still writes to the right
/// place.
fn directory_for(root: &Path, kind: Kind) -> PathBuf {
    MindProject::open(root)
        .expect("an initialized mind project")
        .directory_for(kind)
}

fn write_skill(root: &Path, directory: &str, contents: &str) {
    let dir = directory_for(root, Kind::Skill).join(directory);
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
    // `.mind` is the marker and the configuration; the skills sit beside the
    // code, where the agents that read them look.
    assert!(project.path().join("skills").is_dir());
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

    assert!(matches!(*project.error, CliError::NotInProject(_)));
    assert!(project.error.to_string().contains("mind init"));
    assert!(matches!(*workspace.error, CliError::NotInWorkspace(_)));
    assert!(workspace.error.to_string().contains("flayer init"));
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

    assert!(
        error.error.to_string().contains(MIND_DIR),
        "{}",
        error.error
    );
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

    // Forward slashes on every platform, and — the invariant that matters —
    // the entry the message names is the entry in the file, verbatim. On
    // Windows `Path::display` would report `..\collapse` for a line that
    // reads `../collapse`, and what a command says it wrote has to be what
    // someone opening the file finds.
    assert_eq!(outcome.stdout, "linked collapse as ../collapse\n");
    let config = fs::read_to_string(inner.join(FLAYER_DIR).join(FLAYER_CONFIG)).unwrap();
    assert!(config.contains("\"../collapse\""), "{config}");
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

    assert!(
        error.error.to_string().contains("not registered"),
        "{}",
        error.error
    );
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

    assert!(matches!(*error.error, CliError::NotInWorkspace(_)));
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

    assert!(matches!(*error.error, CliError::UnknownArtifact(name) if name == "absent"));
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

#[test]
fn a_workspace_whose_projects_all_broke_does_not_claim_to_manage_none() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    fs::remove_dir_all(dir.path().join("beta")).unwrap();

    let outcome = flayer(dir.path(), &["list"]).unwrap();

    // "manages no projects yet" would send the user to link something that is
    // already linked; the registry has two entries that simply will not open.
    assert!(
        !outcome.stdout.contains("manages no projects yet"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains("none of which could be opened"));
    assert!(outcome.stdout.contains("flayer unlink"));
    assert_eq!(outcome.stderr.len(), 2);
    assert!(!outcome.ok);
}

#[test]
fn a_failing_command_still_reports_what_it_had_already_noticed() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let failure = mind(dir.path(), &["show", "broken"]).unwrap_err();

    // "no skill named `broken`" on its own is baffling when the file is right
    // there; the warning is the half that explains it.
    assert!(matches!(*failure.error, CliError::UnknownArtifact(_)));
    assert_eq!(failure.warnings.len(), 1);
    assert!(failure.warnings[0].contains("front matter"));
}

#[test]
fn unlink_reports_every_entry_it_removed() {
    let dir = workspace_with_two();
    fs::write(
        dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG),
        "version = 1\nname = \"w\"\nprojects = [\"alpha\", \"./alpha\", \"beta\"]\n",
    )
    .unwrap();

    let outcome = flayer(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\nunlinked ./alpha\n");
    // And it is genuinely gone, rather than reported gone.
    let listed = flayer(dir.path(), &["list"]).unwrap();
    assert!(!listed.stdout.contains("alpha"), "{}", listed.stdout);
    assert!(listed.stdout.contains("beta"));
}

#[test]
fn both_spellings_answer_version_and_help() {
    // `mind flayer --version` erroring while `flayer --version` worked was the
    // two spellings drifting, which is the one thing the shortcut must not do.
    for line in [
        vec!["mind", "flayer", "--version"],
        vec!["mind", "--version"],
    ] {
        let error = Cli::try_parse_from(&line).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayVersion,
            "{line:?} did not answer with a version"
        );
    }
    assert_eq!(
        FlayerCli::try_parse_from(["flayer", "--version"])
            .unwrap_err()
            .kind(),
        clap::error::ErrorKind::DisplayVersion
    );
}

#[test]
fn each_binary_describes_itself_rather_than_the_crate() {
    let mind = Cli::try_parse_from(["mind", "--help"])
        .unwrap_err()
        .to_string();
    let flayer = FlayerCli::try_parse_from(["flayer", "--help"])
        .unwrap_err()
        .to_string();

    // One crate description cannot be right for two binaries: `mind --help`
    // used to advertise the pre-split, cross-project scope.
    assert!(mind.contains("in a mind project"), "{mind}");
    assert!(flayer.contains("flayer workspace"), "{flayer}");
    assert_ne!(mind.lines().next().unwrap(), flayer.lines().next().unwrap());
}

/// Write a rule into an already initialized mind project.
fn write_rule(root: &Path, route: &str, contents: &str) {
    let path = directory_for(root, Kind::Rule).join(route);
    fs::create_dir_all(path.parent().unwrap()).expect("create the rule folder");
    fs::write(path, contents).expect("write the rule");
}

// ---------------------------------------------------------------------------
// More than one kind
// ---------------------------------------------------------------------------

#[test]
fn init_makes_a_folder_for_every_kind() {
    let dir = TempDir::new().unwrap();

    mind(dir.path(), &["init"]).unwrap();

    for kind in Kind::ALL {
        assert!(
            dir.path().join(kind.folder()).is_dir(),
            "no folder for {kind}"
        );
    }
}

#[test]
fn the_kind_column_appears_only_when_more_than_one_kind_is_in_play() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let alone = mind(dir.path(), &["list"]).unwrap();
    assert_eq!(alone.stdout, "deploy  Ship it\n");

    write_rule(dir.path(), "no-force-push.md", "# Never force-push\n");
    let mixed = mind(dir.path(), &["list"]).unwrap();

    // The same rule the project column follows: qualify only when it
    // disambiguates.
    assert_eq!(
        mixed.stdout,
        "skill  deploy         Ship it\nrule   no-force-push  Never force-push\n"
    );
}

#[test]
fn a_kind_can_be_listed_on_its_own() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    write_rule(dir.path(), "no-force-push.md", "Never force-push.\n");

    let rules = mind(dir.path(), &["list", "rules"]).unwrap();
    let skills = mind(dir.path(), &["list", "skills"]).unwrap();

    // Narrowed to one kind, the column goes away again.
    assert_eq!(rules.stdout, "no-force-push  Never force-push.\n");
    assert_eq!(skills.stdout, "deploy  Ship it\n");
    // Singular is accepted too: one word in two grammatical positions.
    assert_eq!(mind(dir.path(), &["list", "rule"]).unwrap(), rules);
}

#[test]
fn an_unknown_kind_says_what_was_expected() {
    let error = Cli::try_parse_from(["mind", "list", "prompts"]).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("skills"), "{message}");
    assert!(message.contains("rules"), "{message}");
}

#[test]
fn show_takes_a_qualified_name_when_one_name_belongs_to_two_kinds() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "The skill"));
    write_rule(dir.path(), "deploy.md", "The rule.\n");

    let both = mind(dir.path(), &["show", "deploy"]).unwrap();
    let just_the_rule = mind(dir.path(), &["show", "rule:deploy"]).unwrap();

    // Ambiguous on purpose: both are shown, and each is labelled by kind
    // because that is what tells them apart.
    assert!(both.stdout.contains("skill:deploy"));
    assert!(both.stdout.contains("rule:deploy"));
    assert!(both.stdout.contains("\n---\n"));
    // Resolved, the qualifier is no longer doing any work, so it goes.
    assert!(just_the_rule.stdout.starts_with("deploy\n"));
    assert!(just_the_rule.stdout.contains("The rule."));
}

#[test]
fn show_prints_no_metadata_block_for_a_rule() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_rule(
        dir.path(),
        "git/no-force-push.md",
        "# Never force-push\n\nUse --force-with-lease.\n",
    );

    let outcome = mind(dir.path(), &["show", "git/no-force-push"]).unwrap();

    let expected_head = "git/no-force-push\n";
    assert!(
        outcome.stdout.starts_with(expected_head),
        "{}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains("no-force-push.md"));
    assert!(outcome.stdout.contains("Use --force-with-lease."));
    // A rule declares nothing, so it gets no metadata section rather than an
    // empty one.
    assert!(!outcome.stdout.contains("allowed-tools"));
    assert!(!outcome.stdout.contains("description"));
}

#[test]
fn validate_counts_each_kind_by_its_own_name() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "one", &skill_file("one", "Fine"));
    write_skill(dir.path(), "two", &skill_file("two", "Fine"));
    write_rule(dir.path(), "solo.md", "Context.\n");

    let all = mind(dir.path(), &["validate"]).unwrap();
    let rules_only = mind(dir.path(), &["validate", "rules"]).unwrap();

    assert!(
        all.stdout
            .contains("2 skills and 1 rule checked, 0 invalid"),
        "{}",
        all.stdout
    );
    assert!(rules_only.stdout.contains("1 rule checked, 0 invalid"));
    // Narrowed to one kind, labels stop naming it.
    assert!(rules_only.stdout.contains("solo: ok"));
    assert!(all.stdout.contains("rule:solo: ok"));
}

#[test]
fn validate_can_still_be_pointed_at_one_artifact() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_rule(dir.path(), "empty.md", "\n  \n");

    let one = mind(dir.path(), &["validate", "good"]).unwrap();
    let broken = mind(dir.path(), &["validate", "empty"]).unwrap();

    assert!(one.ok);
    assert!(one.stdout.contains("1 skill checked, 0 invalid"));
    assert!(!broken.ok);
    assert!(
        broken.stdout.contains("the file has no content"),
        "{}",
        broken.stdout
    );
}

#[test]
fn a_workspace_lists_every_kind_of_every_project() {
    let dir = workspace_with_two();
    write_rule(&dir.path().join("alpha"), "team/style.md", "House style.\n");

    let outcome = flayer(dir.path(), &["list"]).unwrap();

    // Three columns now: project, kind, name — each earning its place.
    assert_eq!(
        outcome.stdout,
        "alpha  skill  alpha       A skill\n\
         beta   skill  beta        A skill\n\
         alpha  rule   team/style  House style.\n"
    );
    // Narrowed to rules there is only one project left with any, so the
    // project column stops telling the reader anything and goes.
    let rules = flayer(dir.path(), &["list", "rules"]).unwrap();
    assert_eq!(rules.stdout, "team/style  House style.\n");
}

// ---------------------------------------------------------------------------
// gather
//
// The repository is built here rather than fetched, for the reason the core
// suite builds one: a test that needs the network fails for reasons that have
// nothing to do with the code.
// ---------------------------------------------------------------------------

/// A git repository holding these files, with one commit.
fn repository(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, contents) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    let repo = gix::init(dir.path()).expect("initialize a repository");
    let tree = write_tree(&repo, dir.path());
    let who = gix::actor::Signature {
        name: "Fixture".into(),
        email: "fixture@example.com".into(),
        time: gix::date::Time::new(0, 0),
    };
    let id = repo
        .write_object(&gix::objs::Commit {
            tree,
            parents: Default::default(),
            author: who.clone(),
            committer: who,
            encoding: None,
            message: "fixture".into(),
            extra_headers: Vec::new(),
        })
        .unwrap()
        .detach();

    let head = fs::read_to_string(dir.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().strip_prefix("ref: ").unwrap();
    let reference = dir.path().join(".git").join(branch);
    fs::create_dir_all(reference.parent().unwrap()).unwrap();
    fs::write(reference, format!("{id}\n")).unwrap();
    dir
}

fn write_tree(repo: &gix::Repository, dir: &Path) -> gix::ObjectId {
    use gix::objs::tree::{Entry, EntryKind};

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let (kind, oid) = if path.is_dir() {
            (EntryKind::Tree, write_tree(repo, &path))
        } else {
            (
                EntryKind::Blob,
                repo.write_blob(fs::read(&path).unwrap()).unwrap().detach(),
            )
        };
        entries.push(Entry {
            mode: kind.into(),
            filename: name.to_string_lossy().as_bytes().into(),
            oid,
        });
    }
    entries.sort();
    repo.write_object(&gix::objs::Tree { entries })
        .unwrap()
        .detach()
}

#[test]
fn gather_reports_the_source_and_what_it_took() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it")),
        (
            "skills/commit-style/SKILL.md",
            &skill_file("commit-style", "How we commit"),
        ),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stdout.contains("added"), "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains("commit-style  How we commit"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("2 skills: 2 added, 0 updated, 0 unchanged"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn gathering_again_says_nothing_moved() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let again = flayer(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        again
            .stdout
            .contains("1 skill: 0 added, 0 updated, 1 unchanged"),
        "{}",
        again.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_a_warning_and_a_non_zero_exit() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill_file("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter\n"),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn gather_list_names_where_each_skill_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("deploy"));
    assert!(outcome.stdout.contains(&url), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("Ship it"));
}

#[test]
fn an_empty_shelf_says_how_to_fill_it() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.starts_with("nothing gathered yet"));
    assert!(outcome.stdout.contains("flayer gather git"));
}

#[test]
fn gathering_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = flayer(dir.path(), &["gather", "list"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("flayer init"));
}

#[test]
fn gather_is_reachable_the_long_way_round_too() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let short = flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let long = mind(dir.path(), &["flayer", "gather", "list"]).unwrap();

    assert!(short.ok);
    assert!(long.stdout.contains("deploy"), "{}", long.stdout);
}

#[test]
fn gather_list_tells_two_branches_of_one_repository_apart() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    // The same URL at its default branch and at a named one: two sources, and
    // a listing that printed the URL alone would show one origin twice.
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    flayer(dir.path(), &["gather", "git", &url, "--ref", &branch]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert_eq!(outcome.stdout.lines().count(), 2, "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains(&format!("{url}#{branch}")),
        "{}",
        outcome.stdout
    );
}

// ---------------------------------------------------------------------------
// Where a project keeps its artifacts
// ---------------------------------------------------------------------------

#[test]
fn init_can_be_told_where_this_project_keeps_its_skills() {
    let dir = TempDir::new().unwrap();

    let outcome = mind(dir.path(), &["init", "--skills", ".claude/skills"]).unwrap();

    assert!(outcome.ok);
    assert!(dir.path().join(".claude").join("skills").is_dir());
    let written = fs::read_to_string(dir.path().join(MIND_DIR).join(MIND_CONFIG)).unwrap();
    assert!(
        written.contains(r#"skills = ".claude/skills""#),
        "{written}"
    );
    // The kind that was not mentioned keeps its default.
    assert!(dir.path().join("rules").is_dir());
}

#[test]
fn listing_reads_from_where_the_project_says_its_skills_are() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "--skills", ".claude/skills"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let outcome = mind(dir.path(), &["list", "skills"]).unwrap();

    assert_eq!(outcome.stdout, "deploy  Ship it\n");
    // And it really is the configured folder that was read.
    assert!(dir.path().join(".claude/skills/deploy/SKILL.md").is_file());
}

#[test]
fn an_empty_project_says_which_directories_it_looked_in() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "--skills", "docs/skills"]).unwrap();

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.starts_with("nothing found"));
    // The configured directory, not the marker: a project that keeps its
    // skills elsewhere is exactly the one whose empty listing needs explaining.
    //
    // Compared against what the project itself says rather than against
    // `"docs/skills"`, because the command prints a path and Windows prints
    // one with backslashes.
    for kind in [Kind::Skill, Kind::Rule] {
        let looked_in = directory_for(dir.path(), kind).display().to_string();
        assert!(outcome.stdout.contains(&looked_in), "{}", outcome.stdout);
    }
}

#[test]
fn init_refuses_a_directory_outside_the_project() {
    let dir = TempDir::new().unwrap();

    let failure = mind(dir.path(), &["init", "--skills", "../elsewhere"]).unwrap_err();

    assert!(
        failure.error.to_string().contains("inside the project"),
        "{}",
        failure.error
    );
    assert!(!dir.path().join(MIND_DIR).exists(), "nothing was made");
}

#[test]
fn a_workspace_sees_a_project_that_keeps_its_skills_elsewhere() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let project = dir.path().join("collapse");
    fs::create_dir(&project).unwrap();
    mind(&project, &["init", "--skills", ".claude/skills"]).unwrap();
    write_skill(&project, "deploy", &skill_file("deploy", "Ship it"));
    flayer(dir.path(), &["link", "collapse"]).unwrap();

    let outcome = flayer(dir.path(), &["list"]).unwrap();

    // The workspace reads the project's own answer rather than assuming one.
    assert!(outcome.stdout.contains("deploy"), "{}", outcome.stdout);
}
