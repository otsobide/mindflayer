//! Changing an artifact: edit, rename, remove, and templates to start from.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// Changing an artifact: edit, rename, remove, and templates to start from
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
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
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

#[test]
fn an_installed_skill_that_is_renamed_stays_installed_under_its_new_name() {
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    let project = dir.path().join("collapse");

    let outcome = mind(&project, &["rename", "deploy", "ship"]).unwrap();

    assert!(
        outcome.stdout.ends_with(
            "  it was installed by `flayer install`, and stays so under its new name; renamed, \
             it is left alone by `flayer install` and `flayer uninstall` unless they are forced\n"
        ),
        "{}",
        outcome.stdout
    );
    // Its name line is not what was installed, so it is taken out only when
    // that is forced.
    let refused = mind(&project, &["uninstall", "ship"]).unwrap();
    assert!(!refused.ok);
    assert!(project.join("skills/ship/SKILL.md").is_file());
    let forced = mind(&project, &["uninstall", "ship", "--force"]).unwrap();
    assert_eq!(forced.stdout, "removed ship from collapse\n");
    assert!(!project.join("skills/ship").exists());
}

#[test]
fn an_installed_rule_that_is_renamed_still_holds_what_was_installed_and_says_so() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let project = dir.path().join("collapse");
    fs::create_dir(&project).unwrap();
    older_project(&project);
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    let own = dir.path().join(".mindflayer/library/rules/git");
    fs::create_dir_all(&own).unwrap();
    fs::write(own.join("no-force-push.md"), "Never force-push.\n").unwrap();
    mind(&project, &["install", "git/no-force-push"]).unwrap();

    let outcome = mind(&project, &["rename", "git/no-force-push", "git/no-push"]).unwrap();

    // A rule's rename moves its file as it is, so nothing in it is an edit.
    assert!(
        outcome.stdout.ends_with(
            "  it was installed by `flayer install`, and stays so under its new name; renamed, \
             it still holds what was installed, and `flayer uninstall` takes it out\n"
        ),
        "{}",
        outcome.stdout
    );
    // Orphaned, since nothing offers it by its new route.
    let status = mind(&project, &["status"]).unwrap().stdout;
    assert!(status.contains("git/no-push  orphaned"), "{status}");
    let removed = mind(&project, &["uninstall", "git/no-push"]).unwrap();
    assert!(removed.ok, "{:?}", removed.stderr);
    assert_eq!(removed.stdout, "removed rule git/no-push from collapse\n");
    assert!(!project.join("rules/git/no-push.md").exists());
}

#[test]
fn removing_an_install_scrubs_the_ledger_it_drops_the_record_from_and_says_so() {
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    // What an older build could have left in the same file.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();

    let outcome = mind(&dir.path().join("collapse"), &["remove", "deploy", "--yes"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.contains("`flayer install` can put it back"),
        "{}",
        outcome.stdout
    );
    assert_eq!(
        outcome.stderr,
        [
            "note: https://example.com/acme/skills: the user name, password or token an older \
             Mindflayer stored in this URL is taken out; gathering it again needs a git \
             credential helper that has it"
        ]
    );
    assert_eq!(ledger.tainted_sources().unwrap(), []);
}

#[cfg(unix)]
#[test]
fn an_editor_that_writes_and_then_fails_leaves_the_ledger_for_the_next_write_to_scrub() {
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();

    let failure = mind(
        &dir.path().join("collapse"),
        &[
            "edit",
            "deploy",
            "--editor",
            "printf 'Mine.\\n' >> \"$1\"; false",
        ],
    )
    .unwrap_err();

    // An edit keeps the record as it was, so it writes nothing to the
    // ledger, and leaves its scrub to the next command that does.
    assert!(
        matches!(*failure.error, CliError::EditorFailed { .. }),
        "{}",
        failure.error
    );
    assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
    assert_eq!(ledger.tainted_sources().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn an_installed_skill_that_is_edited_stays_installed_and_reads_as_edited() {
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);

    let outcome = mind(
        &dir.path().join("collapse"),
        &["edit", "deploy", "--editor", "printf 'Mine.\\n' >>"],
    )
    .unwrap();

    assert!(
        outcome.stdout.ends_with(
            "  it was installed by `flayer install`, and stays so; edited, it is left alone by \
             `flayer install` and `flayer uninstall` unless they are forced\n"
        ),
        "{}",
        outcome.stdout
    );
    let screen = screen_for(&workspace, &ledger);
    assert_eq!(
        screen.targets[0].rows[0].candidate.standing,
        Standing::Edited
    );
}

#[cfg(unix)]
#[test]
fn an_installed_skill_reads_as_edited_when_the_editor_writes_it_and_then_fails() {
    let (dir, workspace, ledger) = shelved_older(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    let project = dir.path().join("collapse");

    // Writes, then exits non-zero, as vim does on `:cq` after `:w`.
    let failure = mind(
        &project,
        &[
            "edit",
            "deploy",
            "--editor",
            "printf 'Mine.\\n' >> \"$1\"; false",
        ],
    )
    .unwrap_err();

    assert!(
        matches!(*failure.error, CliError::EditorFailed { .. }),
        "{}",
        failure.error
    );
    assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
    let manifest = fs::read_to_string(project.join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.ends_with("Mine.\n"), "{manifest}");
    let screen = screen_for(&workspace, &ledger);
    assert_eq!(
        screen.targets[0].rows[0].candidate.standing,
        Standing::Edited
    );
}

/// `hub`, a workspace, and `collapse`, a project `make` makes, which the
/// workspace links from beside it rather than from above: walking up from the
/// project never reaches it.
fn linked_from_beside(make: fn(&Path)) -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    let project = dir.path().join("collapse");
    for root in [&hub, &project] {
        fs::create_dir(root).unwrap();
    }
    flayer(&hub, &["init"]).unwrap();
    make(&project);
    flayer(&hub, &["link", "../collapse"]).unwrap();
    (dir, hub, project)
}

/// `linked_from_beside`, with the workspace's own `deploy` installed into
/// the project, one an older Mindflayer made, so the copy is in the folder
/// it authors in, where a change from the workspace reaches it.
fn installed_from_beside() -> (TempDir, PathBuf, PathBuf) {
    let (dir, hub, project) = linked_from_beside(older_project);
    flayer(&hub, &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(&hub, &["install", "-p", "collapse", "deploy"]).unwrap();
    (dir, hub, project)
}

#[cfg(unix)]
#[test]
fn a_workspace_beside_a_project_keeps_its_record_when_flayer_edits_the_copy() {
    let (_dir, hub, project) = installed_from_beside();

    let outcome = flayer(
        &hub,
        &[
            "edit",
            "-p",
            "collapse",
            "deploy",
            "--editor",
            "printf 'Mine.\\n' >>",
        ],
    )
    .unwrap();

    // Walking up from the project never reaches the workspace; the one the
    // command runs in is the one asked.
    assert!(outcome.stdout.contains("stays so"), "{}", outcome.stdout);
    let left = flayer(&hub, &["uninstall", "-p", "collapse", "deploy"]).unwrap();
    assert_eq!(
        left.stderr,
        vec![
            "warning: deploy in collapse was changed since it was installed, at skills/deploy, \
             so it was left alone; --force takes it out"
        ]
    );
    assert!(project.join("skills/deploy/SKILL.md").is_file());
}

#[cfg(unix)]
#[test]
fn mind_keeps_the_record_of_the_workspace_in_your_home_when_that_manages_the_project() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let project = elsewhere.path().join("collapse");
    fs::create_dir(&project).unwrap();
    // Made as an older Mindflayer made it, so the copy installed is in the
    // folder it authors in, where `mind edit` reaches it.
    older_project(&project);
    flayer_at_home(elsewhere.path(), home.path(), &["link", "collapse"]).unwrap();
    flayer_at_home(
        elsewhere.path(),
        home.path(),
        &["add", "skill", "deploy", "Ship it"],
    )
    .unwrap();
    mind_at_home(&project, home.path(), &["install", "deploy"]).unwrap();

    let outcome = mind_at_home(
        &project,
        home.path(),
        &["edit", "deploy", "--editor", "printf 'Mine.\\n' >>"],
    )
    .unwrap();

    assert!(outcome.stdout.contains("stays so"), "{}", outcome.stdout);
    let left = mind_at_home(&project, home.path(), &["uninstall", "deploy"]).unwrap();
    assert!(!left.ok);
    assert!(project.join("skills/deploy/SKILL.md").is_file());
}

#[test]
fn a_workspace_beside_a_project_offers_it_the_workspaces_templates() {
    let (_dir, hub, project) = linked_from_beside(new_project);
    let template = hub.join(".mindflayer/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nThe hub's runbook.\n").unwrap();

    flayer(
        &hub,
        &[
            "add",
            "-p",
            "collapse",
            "skill",
            "deploy",
            "Ship it",
            "--template",
            "runbook",
        ],
    )
    .unwrap();

    let manifest = fs::read_to_string(project.join("skills/deploy/SKILL.md")).unwrap();
    assert!(
        manifest.ends_with("\n# deploy\n\nThe hub's runbook.\n"),
        "{manifest}"
    );
    assert_eq!(
        flayer(&hub, &["templates", "-p", "collapse"])
            .unwrap()
            .stdout,
        "full     skill  built in\n\
         runbook  skill  ../hub/.mindflayer/templates/runbook\n"
    );
}

#[test]
fn mind_add_from_a_template_refuses_when_the_workspace_that_lends_it_cannot_be_read() {
    let dir = workspace_with_two();
    let template = dir.path().join(".mindflayer/templates/full/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nThe team's full.\n").unwrap();
    let marker = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    let newer: String = fs::read_to_string(&marker)
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("version = ") {
                String::from("version = 99\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(&marker, newer).unwrap();
    let alpha = dir.path().join("alpha");

    let failure = mind(
        &alpha,
        &["add", "skill", "deploy", "Ship it", "--template", "full"],
    )
    .unwrap_err();

    // Passed over, the workspace's `full` would leave the built-in one to
    // stand in for it.
    assert!(matches!(*failure.error, CliError::Workspace(_)));
    assert!(!alpha.join("skills/deploy").exists());
}

/// Write `contents` at `route` below `root`, wherever that is.
fn put(root: &Path, route: &str, contents: &str) {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn a_skill_only_in_an_agents_folder_is_not_changed_from_a_project() {
    let dir = TempDir::new().unwrap();
    let directories = mindflayer_core::Directories::default().with(Kind::Skill, "skills");
    MindProject::init_settings(
        dir.path(),
        &mindflayer_core::Agent::ALL,
        mindflayer_core::Share::Shared,
        &directories,
    )
    .unwrap();
    put(
        dir.path(),
        ".claude/skills/review/SKILL.md",
        &skill_file("review", "Reads the diff"),
    );

    let failure = mind(dir.path(), &["rename", "review", "audit"]).unwrap_err();

    assert!(
        failure
            .error
            .to_string()
            .contains("in a folder this project's agents read and it does not author in"),
        "{}",
        failure.error
    );
    assert!(dir.path().join(".claude/skills/review/SKILL.md").is_file());
    assert!(!dir.path().join(".claude/skills/audit").exists());
}

#[test]
fn a_name_the_project_authors_and_its_agents_read_is_one_row_whose_change_reaches_only_its_own() {
    let dir = TempDir::new().unwrap();
    let directories = mindflayer_core::Directories::default().with(Kind::Skill, "skills");
    MindProject::init_settings(
        dir.path(),
        &mindflayer_core::Agent::ALL,
        mindflayer_core::Share::Shared,
        &directories,
    )
    .unwrap();
    let deploy = skill_file("deploy", "My own source");
    put(dir.path(), "skills/deploy/SKILL.md", &deploy);
    put(dir.path(), ".claude/skills/deploy/SKILL.md", &deploy);

    // One row, so the words a screen runs for it name one thing: the
    // project's own, the copy the row reads as.
    let listed = mind(dir.path(), &["list"]).unwrap();
    assert_eq!(listed.stdout, "deploy  My own source\n");
    let shown = mind(dir.path(), &["show", "skill:deploy"]).unwrap();
    let own = dir.path().join("skills/deploy/SKILL.md");
    assert!(
        shown.stdout.contains(&own.display().to_string()),
        "{}",
        shown.stdout
    );

    mind(dir.path(), &["remove", "--yes", "skill:deploy"]).unwrap();

    assert!(!dir.path().join("skills/deploy").exists());
    assert!(dir.path().join(".claude/skills/deploy/SKILL.md").is_file());
    let left = mind(dir.path(), &["list"]).unwrap();
    assert_eq!(left.stdout, "deploy  My own source\n");
}
