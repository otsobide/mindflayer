//! Installing rules from the command line: `mind install` and `flayer
//! install -p` take a rule's route, bare or after `rule:`, and `uninstall`
//! takes it back out, in a project that keeps what is installed to itself
//! and in one that shares it with the team.

mod common;

use common::*;

/// What `flayer add rule` writes for the workspace's own `git/commits`.
const COMMITS: &str = "# Commit one coherent step at a time\n";

/// The block a project marked by its settings is given for it alone.
const BLOCK: &str = "<!-- mindflayer:begin -->\n<!-- mindflayer:rule git/commits -->\n\
                     # Commit one coherent step at a time\n<!-- mindflayer:end -->\n";

/// What a team's `AGENTS.md` might hold before Mindflayer writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A rule about Rust files only.
const SCOPED: &str = "---\npaths:\n  - \"**/*.rs\"\n---\n\n# Comments say why, not what\n";

/// A workspace linking `alpha`, a repository given `mind init` with
/// `init` (`--shared` or nothing more), and keeping the rule `git/commits`
/// of its own.
fn workspace(init: &[&str]) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    gix::init(&alpha).unwrap();
    let mut args = vec!["init"];
    args.extend(init);
    mind(&alpha, &args).unwrap();
    flayer(dir.path(), &["link", "alpha"]).unwrap();
    flayer(
        dir.path(),
        &[
            "add",
            "rule",
            "git/commits",
            "Commit one coherent step at a time",
        ],
    )
    .unwrap();
    (dir, alpha)
}

/// The workspace's own rule at `route`, holding `source`.
fn own_rule(dir: &Path, route: &str, source: &str) {
    let file = own_directory_for(dir, Kind::Rule).join(format!("{route}.md"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, source).unwrap();
}

/// Nothing said on stderr, and a zero exit.
fn quiet(outcome: &Outcome) {
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
}

#[test]
fn a_shared_project_is_given_a_rule_in_its_agents_md_for_the_team_and_it_comes_back_out() {
    let (dir, alpha) = workspace(&["--shared"]);

    let installed = flayer(dir.path(), &["install", "-p", "alpha", "rule:git/commits"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed rule git/commits into alpha, from workspace\n"
    );
    quiet(&installed);
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), BLOCK);
    // Shared: the team commits it, so nothing keeps it out of git.
    assert!(untracked(&alpha).contains(&String::from("AGENTS.md")));

    let removed = mind(&alpha, &["uninstall", "git/commits"]).unwrap();

    assert_eq!(removed.stdout, "removed rule git/commits from alpha\n");
    quiet(&removed);
    assert!(!alpha.join("AGENTS.md").exists());
}

#[test]
fn a_local_project_keeps_the_agents_md_a_rule_makes_out_of_git() {
    let (_dir, alpha) = workspace(&[]);

    let installed = mind(&alpha, &["install", "git/commits"]).unwrap();
    let again = mind(&alpha, &["install", "git/commits"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed rule git/commits into alpha, from workspace\n"
    );
    quiet(&installed);
    assert_eq!(
        again.stdout,
        "rule git/commits in alpha was already current\n"
    );
    quiet(&again);
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), BLOCK);
    assert!(
        !untracked(&alpha).contains(&String::from("AGENTS.md")),
        "{:?}",
        untracked(&alpha)
    );

    let removed = mind(&alpha, &["uninstall", "rule:git/commits"]).unwrap();

    assert_eq!(removed.stdout, "removed rule git/commits from alpha\n");
    quiet(&removed);
    assert!(!alpha.join("AGENTS.md").exists());
}

#[test]
fn a_tracked_agents_md_in_a_local_project_is_left_alone_and_the_agents_it_misses_are_said() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    gix::init(&alpha).unwrap();
    fs::write(alpha.join("AGENTS.md"), TEAM).unwrap();
    indexed(&alpha);
    mind(&alpha, &["init"]).unwrap();
    flayer(dir.path(), &["link", "alpha"]).unwrap();
    own_rule(dir.path(), "git/commits", COMMITS);

    let installed = flayer(dir.path(), &["install", "-p", "alpha", "git/commits"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed rule git/commits into alpha, from workspace\n"
    );
    assert_eq!(
        installed.stderr,
        [format!(
            "warning: {}: git tracks it, and the project is local without edit_tracked, so no \
             rules are written there, and codex and opencode read none of them",
            alpha.join("AGENTS.md").display()
        )]
    );
    assert!(!installed.ok);
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), TEAM);
    // Claude has it from a file of its own instead, kept out of git.
    let file = alpha.join(".claude/rules/mindflayer/git/commits.md");
    assert_eq!(fs::read_to_string(&file).unwrap(), COMMITS);
    assert!(
        !untracked(&alpha)
            .iter()
            .any(|path| path.starts_with(".claude")),
        "{:?}",
        untracked(&alpha)
    );

    // With the last rule gone no agent misses any, so nothing is said.
    let removed = flayer(dir.path(), &["uninstall", "-p", "alpha", "git/commits"]).unwrap();

    assert_eq!(removed.stdout, "removed rule git/commits from alpha\n");
    quiet(&removed);
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), TEAM);
    assert!(!file.exists());
}

#[test]
fn a_rule_about_some_files_reaches_claude_alone_and_says_so() {
    let (dir, alpha) = workspace(&["--shared"]);
    own_rule(dir.path(), "style/comments", SCOPED);

    let installed = flayer(dir.path(), &["install", "-p", "alpha", "style/comments"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed rule style/comments into alpha, from workspace\n"
    );
    assert_eq!(
        installed.stderr,
        [
            "warning: rule `style/comments` states `paths`, so it reaches claude only: codex and \
          opencode read no .claude/rules"
        ]
    );
    assert!(!installed.ok);
    assert!(!alpha.join("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(alpha.join(".claude/rules/mindflayer/style/comments.md")).unwrap(),
        SCOPED
    );

    // Said of the rule it is about: another going in or out after it says
    // nothing of it, which `status` says whenever asked.
    let other = flayer(dir.path(), &["install", "-p", "alpha", "git/commits"]).unwrap();
    let out = flayer(dir.path(), &["uninstall", "-p", "alpha", "git/commits"]).unwrap();

    quiet(&other);
    quiet(&out);
}

#[test]
fn one_name_a_skill_and_a_rule_share_is_installed_and_taken_out_by_its_kind() {
    let (dir, alpha) = workspace(&[]);
    flayer(
        dir.path(),
        &["add", "skill", "deploy", "Ship it to staging"],
    )
    .unwrap();
    flayer(
        dir.path(),
        &["add", "rule", "deploy", "Ship on a Friday never"],
    )
    .unwrap();

    let both = mind(&alpha, &["install", "deploy"]).unwrap_err();
    let rule = mind(&alpha, &["install", "rule:deploy"]).unwrap();
    let skill = mind(&alpha, &["install", "skill:deploy"]).unwrap();

    assert_eq!(
        both.error.to_string(),
        "`deploy` is offered as a skill and as a rule, so say which: `skill:deploy` or \
         `rule:deploy`"
    );
    assert_eq!(
        rule.stdout,
        "installed rule deploy into alpha, from workspace\n"
    );
    assert_eq!(
        skill.stdout,
        "installed deploy into alpha, from workspace\n"
    );
    assert!(fs::read_to_string(alpha.join("AGENTS.md"))
        .unwrap()
        .contains("<!-- mindflayer:rule deploy -->\n# Ship on a Friday never\n"));
    for copy in copies(&alpha, "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }

    let both = mind(&alpha, &["uninstall", "deploy"]).unwrap_err();
    let rule = mind(&alpha, &["uninstall", "rule:deploy"]).unwrap();
    let skill = mind(&alpha, &["uninstall", "deploy"]).unwrap();

    assert_eq!(
        both.error.to_string(),
        "`deploy` is installed as a skill and as a rule, so say which: `skill:deploy` or \
         `rule:deploy`"
    );
    assert_eq!(rule.stdout, "removed rule deploy from alpha\n");
    assert_eq!(skill.stdout, "removed deploy from alpha\n");
    assert!(!alpha.join("AGENTS.md").exists());
}

#[test]
fn a_rule_not_installed_is_not_there_to_take_out() {
    let (_dir, alpha) = workspace(&[]);

    let failure = mind(&alpha, &["uninstall", "rule:git/commits"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInstalled { .. }));
    assert_eq!(
        failure.error.to_string(),
        "alpha has no rule git/commits installed, so there is nothing to take out"
    );
}

#[test]
fn a_section_somebody_else_wrote_is_neither_written_over_nor_taken_out() {
    let (dir, alpha) = workspace(&["--shared"]);
    own_rule(dir.path(), "review/tone", "Say what a change does first.\n");
    let theirs = format!(
        "{TEAM}\n<!-- mindflayer:begin -->\n<!-- mindflayer:rule review/tone -->\nTheirs.\n\
         <!-- mindflayer:end -->\n"
    );
    fs::write(alpha.join("AGENTS.md"), &theirs).unwrap();

    let installed = mind(&alpha, &["install", "review/tone"]).unwrap();
    let removed = mind(&alpha, &["uninstall", "review/tone"]).unwrap();

    assert_eq!(installed.stdout, "");
    assert_eq!(
        installed.stderr,
        [
            "warning: alpha already has a rule review/tone at AGENTS.md that Mindflayer did not \
          put there, so it was left alone"
        ]
    );
    assert!(!installed.ok);
    assert_eq!(removed.stdout, "");
    assert_eq!(
        removed.stderr,
        [
            "warning: rule review/tone at AGENTS.md in alpha was not installed by Mindflayer, so \
          it was left alone"
        ]
    );
    assert!(!removed.ok);
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), theirs);
}

#[test]
fn a_project_a_mind_toml_marks_is_given_a_copy_of_the_rule_filed_by_its_route() {
    let (dir, _alpha) = workspace(&[]);
    let beta = dir.path().join("beta");
    fs::create_dir(&beta).unwrap();
    older_project(&beta);
    flayer(dir.path(), &["link", "beta"]).unwrap();

    let installed = flayer(dir.path(), &["install", "-p", "beta", "rule:git/commits"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed rule git/commits into beta, from workspace\n"
    );
    let copy = directory_for(&beta, Kind::Rule).join("git/commits.md");
    assert_eq!(fs::read_to_string(&copy).unwrap(), COMMITS);

    let removed = flayer(dir.path(), &["uninstall", "-p", "beta", "git/commits"]).unwrap();

    assert_eq!(removed.stdout, "removed rule git/commits from beta\n");
    assert!(!copy.exists());
}

#[test]
fn a_rule_the_project_received_is_not_changed_from_it_and_says_how_to_take_it_out() {
    let (_dir, alpha) = workspace(&[]);
    mind(&alpha, &["install", "git/commits"]).unwrap();
    let said = "`git/commits` is a rule alpha received, rendered from where it was installed \
                from rather than kept in it, so it is not changed from here: uninstall it, or \
                change it where it is offered and install it again";

    let removed = mind(&alpha, &["remove", "git/commits", "--yes"]).unwrap_err();
    let renamed = mind(&alpha, &["rename", "git/commits", "git/steps"]).unwrap_err();
    let edited = mind(&alpha, &["edit", "git/commits", "--editor", "true"]).unwrap_err();

    for failure in [&removed, &renamed, &edited] {
        assert_eq!(failure.error.to_string(), said);
    }
    assert_eq!(fs::read_to_string(alpha.join("AGENTS.md")).unwrap(), BLOCK);
}

#[test]
fn a_route_two_places_offer_is_installed_from_the_one_from_names() {
    let (dir, alpha) = workspace(&[]);
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    flayer(&team, &["init"]).unwrap();
    flayer(
        &team,
        &["add", "rule", "git/commits", "Commit as the team does"],
    )
    .unwrap();
    flayer(dir.path(), &["load", "team"]).unwrap();

    let twice = mind(&alpha, &["install", "git/commits"]).unwrap_err();
    let theirs = mind(&alpha, &["install", "rule:git/commits", "--from", "team"]).unwrap();

    assert_eq!(
        twice.error.to_string(),
        "`git/commits` is offered by more than one place (workspace, load:team), so say which \
         with --from"
    );
    assert_eq!(
        theirs.stdout,
        "installed rule git/commits into alpha, from load:team\n"
    );
    quiet(&theirs);
    assert!(fs::read_to_string(alpha.join("AGENTS.md"))
        .unwrap()
        .contains("<!-- mindflayer:rule git/commits -->\n# Commit as the team does\n"));
    assert_eq!(
        mind(&alpha, &["status"]).unwrap().stdout,
        "git/commits  current  claude,codex,opencode  load:team\n"
    );
}
