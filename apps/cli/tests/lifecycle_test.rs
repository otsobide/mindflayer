//! Changing an artifact: edit, rename, remove, and templates to start from.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// Changing an artifact: edit, rename, remove — and templates to start from
// ---------------------------------------------------------------------------

#[test]
fn remove_without_yes_says_what_it_would_delete_and_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(
        dir.path(),
        &["add", "skill", "deploy", "Ship it", "--template", "full"],
    )
    .unwrap();

    let failure = mind(dir.path(), &["remove", "deploy"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::Unconfirmed { .. }));
    let said = failure.error.to_string();
    assert!(said.contains("and the 3 files in it"), "{said}");
    assert!(said.contains("--yes"), "{said}");
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
}

#[test]
fn remove_with_yes_deletes_it_and_rm_is_the_short_way() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "rule", "git/no-force-push", "Never"]).unwrap();

    let outcome = mind(dir.path(), &["rm", "rule:git/no-force-push", "--yes"]).unwrap();

    assert!(
        outcome
            .stdout
            .starts_with("removed rule git/no-force-push from "),
        "{}",
        outcome.stdout
    );
    assert!(!dir.path().join("rules/git").exists());
}

#[test]
fn a_skill_that_does_not_read_can_still_be_removed_by_its_folder() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "broken", "no front matter at all\n");

    mind(dir.path(), &["remove", "broken", "--yes"]).unwrap();

    assert!(!dir.path().join("skills/broken").exists());
}

#[test]
fn rename_moves_a_skill_and_what_it_declares_together() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    let outcome = mind(dir.path(), &["rename", "deploy", "ship"]).unwrap();

    assert!(
        outcome
            .stdout
            .starts_with("renamed skill deploy to ship at "),
        "{}",
        outcome.stdout
    );
    assert!(mind(dir.path(), &["show", "ship"]).is_ok());
    assert!(mind(dir.path(), &["validate"]).unwrap().ok);
    // `mv` is the short way, for a rule too.
    mind(dir.path(), &["add", "rule", "style", "Plainly"]).unwrap();
    mind(dir.path(), &["mv", "style", "docs/style"]).unwrap();
    assert!(dir.path().join("rules/docs/style.md").is_file());
}

#[test]
fn rename_will_not_take_a_name_that_is_in_use() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    mind(dir.path(), &["add", "skill", "ship", "Also here"]).unwrap();

    let failure = mind(dir.path(), &["rename", "deploy", "ship"]).unwrap_err();

    assert!(failure.error.to_string().contains("already exists"));
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
}

#[cfg(unix)]
#[test]
fn edit_opens_the_file_and_checks_it_once_the_editor_closes() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    // A stand-in editor: the command is run with the file after it, so this
    // appends a line to whatever it is given.
    let outcome = mind(
        dir.path(),
        &["edit", "deploy", "--editor", "printf 'More steps.\\n' >>"],
    )
    .unwrap();

    assert_eq!(outcome.stdout, "edited skill deploy: ok\n");
    let manifest = fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.ends_with("More steps.\n"), "{manifest}");

    let untouched = mind(dir.path(), &["edit", "deploy", "--editor", "true"]).unwrap();
    assert_eq!(untouched.stdout, "left skill deploy unchanged: ok\n");
}

#[cfg(unix)]
#[test]
fn edit_says_what_the_edit_broke() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    let replacement = dir.path().join("replacement.md");
    fs::write(
        &replacement,
        "---\nname: Deploy\ndescription: Ship it\n---\n",
    )
    .unwrap();

    let outcome = mind(
        dir.path(),
        &[
            "edit",
            "deploy",
            "--editor",
            &format!("cp '{}'", replacement.display()),
        ],
    )
    .unwrap();

    assert!(!outcome.ok);
    assert!(
        outcome
            .stdout
            .starts_with("edited skill deploy: 2 problems\n"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn edit_says_so_when_the_editor_does_not_work() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    let failure = mind(
        dir.path(),
        &["edit", "deploy", "--editor", "/no/such/editor"],
    )
    .unwrap_err();

    assert!(
        matches!(
            *failure.error,
            CliError::Editor { .. } | CliError::EditorFailed { .. }
        ),
        "{}",
        failure.error
    );
}

#[test]
fn templates_lists_what_add_can_start_from_and_where_each_comes_from() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    for (route, contents) in [
        (".mind/templates/runbook/SKILL.md", "---\n---\n"),
        (".mind/templates/policy.md", "## Why\n"),
    ] {
        let path = dir.path().join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    let outcome = mind(dir.path(), &["templates"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "full     skill  built in\n\
         runbook  skill  .mind/templates/runbook\n\
         policy   rule   .mind/templates/policy.md\n"
    );
}

#[test]
fn add_from_a_template_that_is_not_there_says_which_are() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let failure = mind(
        dir.path(),
        &["add", "skill", "deploy", "Ship it", "--template", "nope"],
    )
    .unwrap_err();

    assert!(matches!(*failure.error, CliError::Template(_)));
    assert!(failure.error.to_string().contains("there are full"));
}

#[test]
fn a_removed_install_leaves_no_record_for_what_is_written_there_next() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    let project = dir.path().join("collapse");

    let outcome = mind(&project, &["remove", "deploy", "--yes"]).unwrap();
    assert!(
        outcome.stdout.contains("`flayer install` can put it back"),
        "{}",
        outcome.stdout
    );

    // Somebody writes their own under the same name. Had the record outlived
    // the copy, the install screen would count this as its own to delete.
    mind(&project, &["add", "skill", "deploy", "Mine now"]).unwrap();
    let screen = screen_for(&workspace, &ledger);
    assert_eq!(
        screen.targets[0].rows[0].candidate.standing,
        Standing::Foreign
    );
}

#[cfg(unix)]
#[test]
fn an_installed_skill_that_is_edited_is_yours_from_then_on() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);

    let outcome = mind(
        &dir.path().join("collapse"),
        &["edit", "deploy", "--editor", "printf 'Mine.\\n' >>"],
    )
    .unwrap();

    assert!(outcome.stdout.contains("it is yours"), "{}", outcome.stdout);
    let screen = screen_for(&workspace, &ledger);
    assert_eq!(
        screen.targets[0].rows[0].candidate.standing,
        Standing::Foreign
    );
}
