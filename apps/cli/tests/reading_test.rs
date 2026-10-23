//! The reading commands — `list`, `show`, `validate` — over every kind, from
//! wherever a project keeps them.

mod common;

use common::*;

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
