//! The install screen and the rule it works by.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// install
//
// The screen is driven the way a person drives it (a sequence of key presses)
// and drawn into an in-memory terminal, so the two things a user actually
// meets, what a key does and what they see, are both under test. Only the
// event loop that reads a real keyboard is not, and it holds nothing else.
// ---------------------------------------------------------------------------

#[test]
fn the_screen_opens_on_the_projects_with_the_shelf_beside_them() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let screen = screen_for(&workspace, &ledger);

    assert_eq!(screen.focus, Focus::Projects);
    assert_eq!(screen.targets.len(), 2);
    let drawn = rendered(&screen);
    assert!(drawn.contains("Projects"), "{drawn}");
    assert!(drawn.contains("collapse"), "{drawn}");
    assert!(drawn.contains("tanukeys"), "{drawn}");
    assert!(drawn.contains("Skills in collapse"), "{drawn}");
    assert!(drawn.contains("[ ] deploy"), "{drawn}");
}

#[test]
fn moving_down_the_projects_changes_which_skills_are_shown() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Down]);

    assert_eq!(screen.current().unwrap().name(), "tanukeys");
    assert!(rendered(&screen).contains("Skills in tanukeys"));
}

#[test]
fn right_enters_the_skills_and_left_comes_back() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Right]);
    assert_eq!(screen.focus, Focus::Skills);
    // The footer says what the keys do here, and it is not what it said there.
    assert!(rendered(&screen).contains("space mark"));

    press(&mut screen, &[KeyCode::Left]);
    assert_eq!(screen.focus, Focus::Projects);
    press(&mut screen, &[KeyCode::Enter]);
    assert_eq!(screen.focus, Focus::Skills, "enter does what right does");
    press(&mut screen, &[KeyCode::Esc]);
    assert_eq!(screen.focus, Focus::Projects, "esc does what left does");
}

#[test]
fn space_marks_a_skill_and_the_screen_says_what_is_waiting() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (1, 0));
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "{drawn}");
    assert!(drawn.contains("+ install"), "{drawn}");
    assert!(drawn.contains("1 to install, 0 to remove"), "{drawn}");
}

#[test]
fn each_project_keeps_its_own_marks() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    // Mark it for the first project only.
    press(
        &mut screen,
        &[KeyCode::Right, KeyCode::Char(' '), KeyCode::Left],
    );
    press(&mut screen, &[KeyCode::Down]);

    let drawn = rendered(&screen);
    assert!(drawn.contains("Skills in tanukeys"), "{drawn}");
    assert!(
        drawn.contains("[ ] deploy"),
        "the mark belongs to the other project:\n{drawn}"
    );
    assert_eq!(screen.counts(), (1, 0), "and it is still waiting");
    // The project column shows where the marks are.
    assert!(drawn.contains("collapse"), "{drawn}");
}

#[test]
fn applying_asks_first_because_it_deletes_things() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let step = press(&mut screen, &[KeyCode::Char('a')]);

    assert_eq!(step, Step::Stay, "not yet");
    assert!(screen.confirming);
    let drawn = rendered(&screen);
    assert!(drawn.contains("Install 1, remove 0?"), "{drawn}");

    assert_eq!(press(&mut screen, &[KeyCode::Char('n')]), Step::Stay);
    assert!(!screen.confirming, "answering no puts it away");
    assert_eq!(
        press(&mut screen, &[KeyCode::Char('a'), KeyCode::Char('y')]),
        Step::Apply
    );
}

#[test]
fn applying_nothing_says_so_rather_than_asking() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    let step = press(&mut screen, &[KeyCode::Char('a')]);

    assert_eq!(step, Step::Stay);
    assert!(!screen.confirming);
    assert!(rendered(&screen).contains("nothing marked"));
}

#[test]
fn q_leaves_without_doing_anything() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(press(&mut screen, &[KeyCode::Char('q')]), Step::Quit);
}

#[test]
fn applying_installs_what_was_marked_and_reports_it() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.contains("collapse: installed deploy"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("1 installed, 0 removed"),
        "{}",
        outcome.stdout
    );
    assert!(dir.path().join("collapse/skills/deploy/SKILL.md").is_file());
}

#[test]
fn an_installed_skill_opens_already_marked_next_time() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    install_screen::apply(&workspace, &ledger, &screen).unwrap();

    let reopened = screen_for(&workspace, &ledger);

    assert_eq!(
        reopened.targets[0].rows[0].candidate.standing,
        Standing::Installed
    );
    assert!(rendered(&reopened).contains("[x] deploy"));
    assert_eq!(
        reopened.counts(),
        (0, 0),
        "nothing waiting until you touch it"
    );
}

#[test]
fn unmarking_an_installed_skill_removes_it() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    install_screen::apply(&workspace, &ledger, &screen).unwrap();

    let mut again = screen_for(&workspace, &ledger);
    press(&mut again, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(again.counts(), (0, 1));
    assert!(rendered(&again).contains("- remove"));
    let outcome = install_screen::apply(&workspace, &ledger, &again).unwrap();

    assert!(
        outcome.stdout.contains("collapse: removed deploy"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("0 installed, 1 removed"),
        "{}",
        outcome.stdout
    );
    assert!(!dir.path().join("collapse/skills/deploy").exists());
}

#[test]
fn a_skill_written_by_hand_is_shown_but_not_touched() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    // Same name as a shelf entry, but nobody here put it there.
    write_skill(
        &dir.path().join("collapse"),
        "deploy",
        &skill_file("deploy", "Written by a person"),
    );

    let mut screen = screen_for(&workspace, &ledger);
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "shown as present:\n{drawn}");
    assert!(drawn.contains("not installed by mindflayer"), "{drawn}");

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (0, 0), "the box did not move");
    let after = rendered(&screen);
    assert!(after.contains("left alone"), "and it says why:\n{after}");
    let kept = fs::read_to_string(dir.path().join("collapse/skills/deploy/SKILL.md")).unwrap();
    assert!(kept.contains("Written by a person"));
}

#[test]
fn one_name_from_two_shelves_cannot_be_marked_twice() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    // A second repository offering the same name.
    let other = repository(&[(
        "skills/deploy/SKILL.md",
        &skill_file("deploy", "A different deploy"),
    )]);
    flayer(
        dir.path(),
        &["gather", "git", &other.path().to_string_lossy()],
    )
    .unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    assert_eq!(screen.targets[0].rows.len(), 2, "both are offered");

    // Mark the first, then the second: a project has one directory called
    // `deploy`, so the second unmarks the first rather than both being applied.
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    press(&mut screen, &[KeyCode::Down, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (1, 0));
    assert!(rendered(&screen).contains("unmarked the one from"));
}

// ---------------------------------------------------------------------------
// What an interrupted install left behind
// ---------------------------------------------------------------------------

/// Leave in `project` what an install stopped halfway leaves: a stage named
/// for a process that has gone, holding half a copy, in its scratch folder.
fn interrupted(project: &Path) -> PathBuf {
    let left = project.join(".mind/tmp/4242-00000000deadbeef");
    fs::create_dir_all(left.join("new/deploy")).unwrap();
    fs::write(left.join("new/deploy/SKILL.md"), "half").unwrap();
    left
}

#[test]
fn an_install_clears_what_an_interrupted_one_left_and_says_so_once() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    let left = interrupted(&alpha);

    let outcome = mind(&alpha, &["install", "commit-style"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "installed commit-style into alpha, from workspace\n  \
         cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish\n"
    );
    assert!(
        outcome.ok && outcome.stderr.is_empty(),
        "{:?}",
        outcome.stderr
    );
    assert!(!left.exists());
    assert_eq!(
        mind(&alpha, &["install", "commit-style"]).unwrap().stdout,
        "commit-style in alpha was already current\n"
    );
}

#[test]
fn an_install_that_fails_still_says_what_it_cleared_before_the_error() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    let left = interrupted(&alpha);
    // A file where the skills folder goes: the copy is built, but cannot be
    // put in place.
    let skills = directory_for(&alpha, Kind::Skill);
    fs::remove_dir_all(&skills).unwrap();
    fs::write(&skills, "not a folder").unwrap();

    let failure = mind(&alpha, &["install", "commit-style"]).unwrap_err();

    assert_eq!(
        failure.warnings,
        ["cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish"]
    );
    assert!(
        failure.error.to_string().contains("cannot be written"),
        "{}",
        failure.error
    );
    assert!(!left.exists());
}

#[test]
fn applying_says_what_an_interrupted_install_left_in_a_project() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    interrupted(&dir.path().join("collapse"));
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.contains(
            "collapse: installed deploy\n  \
             cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish\n"
        ),
        "{}",
        outcome.stdout
    );
}

#[test]
fn applying_says_what_an_install_that_failed_had_cleared() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let collapse = dir.path().join("collapse");
    interrupted(&collapse);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    // A file where the skills folder goes, once the screen is open: the copy
    // is built, but cannot be put in place.
    let skills = directory_for(&collapse, Kind::Skill);
    fs::remove_dir_all(&skills).unwrap();
    fs::write(&skills, "not a folder").unwrap();

    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(!outcome.ok);
    assert_eq!(
        outcome.stdout,
        "collapse: cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish\n\
         \n0 installed, 0 removed\n"
    );
    assert!(
        outcome
            .stderr
            .iter()
            .any(|line| line.starts_with("warning: collapse: ")
                && line.contains("cannot be written")),
        "{:?}",
        outcome.stderr
    );
}

#[test]
fn a_workspace_with_no_projects_says_so_instead_of_opening_a_screen() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["install"]).unwrap();

    assert!(outcome.ok);
    assert!(
        outcome.stdout.contains("manages no projects yet"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains("flayer link"), "{}", outcome.stdout);
}

#[test]
fn opening_the_screen_makes_no_ledger() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    flayer(dir.path(), &["install"]).unwrap();

    assert!(!dir.path().join(FLAYER_DIR).join("mindflayer.db").exists());
}

#[test]
fn install_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = flayer(dir.path(), &["install"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("flayer init"));
}

#[test]
fn a_project_with_an_empty_shelf_is_told_where_skills_come_from() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    mind(&root, &["init"]).unwrap();
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    let workspace = FlayerWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let screen = screen_for(&workspace, &ledger);

    assert!(rendered(&screen).contains("nothing on the shelf"));
}

/// A project beside the shelf's `beta`, marked by settings that name
/// `agents` and author skills nowhere, linked to the workspace.
fn with_settings(dir: &Path, agents: &str) {
    let root = dir.join("alpha");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(
        root.join(MIND_DIR).join("settings.json"),
        format!(
            r#"{{
  "version": 3,
  "id": "0123456789abcdef0123456789abcdef",
  "name": "alpha",
  "agents": [{agents}],
  "share": "shared",
  "directories": {{"rules": "rules"}}
}}
"#
        ),
    )
    .unwrap();
    flayer(dir, &["link", "alpha"]).unwrap();
}

/// Where the screen lists the project called `name`.
fn listed(screen: &Screen, name: &str) -> usize {
    screen
        .targets
        .iter()
        .position(|target| target.name() == name)
        .unwrap_or_else(|| panic!("{name} is not listed"))
}

#[test]
fn a_project_whose_settings_name_no_agent_is_said_and_hides_no_other() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), "");
    let workspace = FlayerWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let mut screen = screen_for(&workspace, &ledger);

    assert_eq!(screen.targets.len(), 2);
    screen.cursor = listed(&screen, "alpha");
    let drawn = rendered(&screen);
    assert!(drawn.contains("alpha names no agent"), "{drawn}");
    assert!(!drawn.contains("nothing on the shelf"), "{drawn}");
    press(&mut screen, &[KeyCode::Right]);
    assert_eq!(screen.focus, Focus::Projects);
    assert_eq!(
        screen.message.as_deref(),
        Some(
            "alpha names no agent in its settings that skills are installed for, so none can \
             be installed into it"
        )
    );
    screen.cursor = listed(&screen, "beta");
    assert!(rendered(&screen).contains("[ ] deploy"));
    assert!(!dir.path().join("alpha/skills").exists());
}

#[test]
fn the_screen_installs_into_every_folder_a_project_marked_by_its_settings_has_agents_read() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), r#""claude", "codex""#);
    let workspace = FlayerWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    screen.cursor = listed(&screen, "alpha");
    assert!(rendered(&screen).contains("[ ] deploy"));

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.contains("alpha: installed deploy"),
        "{}",
        outcome.stdout
    );
    let root = dir.path().join("alpha");
    let claude = fs::read(root.join(".claude/skills/deploy/SKILL.md")).unwrap();
    let agents = fs::read(root.join(".agents/skills/deploy/SKILL.md")).unwrap();
    assert_eq!(claude, agents);
    let mut reopened = screen_for(&workspace, &ledger);
    reopened.cursor = listed(&reopened, "alpha");
    assert!(rendered(&reopened).contains("[x] deploy"));

    // Unticked, both go.
    press(&mut reopened, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &ledger, &reopened).unwrap();
    assert!(
        outcome.stdout.contains("alpha: removed deploy"),
        "{}",
        outcome.stdout
    );
    assert!(!root.join(".claude/skills/deploy").exists());
    assert!(!root.join(".agents/skills/deploy").exists());
}

#[test]
fn a_skill_somebody_keeps_in_one_agents_folder_is_left_alone_from_the_screen() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), r#""claude", "codex""#);
    let root = dir.path().join("alpha");
    fs::create_dir_all(root.join(".claude/skills/deploy")).unwrap();
    fs::write(
        root.join(".claude/skills/deploy/SKILL.md"),
        skill_file("deploy", "Written by a person"),
    )
    .unwrap();
    let workspace = FlayerWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    screen.cursor = listed(&screen, "alpha");

    let drawn = rendered(&screen);
    assert!(drawn.contains("not installed by mindflayer"), "{drawn}");
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(screen.counts(), (0, 0));
    assert!(!root.join(".agents/skills/deploy").exists());
}

/// Put a token into the URL of every source on the shelf, the way a Mindflayer
/// from before the refusal stored one.
fn token_in_the_shelf(workspace: &FlayerWorkspace) {
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
}

const SCRUBBED: &str = "note: https://example.com/acme/skills: the user name, password or \
     token an older Mindflayer stored in this URL is taken out; gathering it again needs a git \
     credential helper that has it";

/// How many sources still hold a credential in the file, which a read hands
/// back without it.
fn tainted(workspace: &FlayerWorkspace) -> usize {
    workspace
        .ledger_for_reading()
        .unwrap()
        .tainted_sources()
        .unwrap()
        .len()
}

#[test]
fn opening_the_screen_scrubs_nothing_and_applying_scrubs_first_and_says_so_without_failing() {
    let (_dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    token_in_the_shelf(&workspace);
    let (projects, _) = install_screen::registered(&workspace);

    let mut screen = install_screen::open(&workspace, projects).unwrap();

    assert_eq!(tainted(&workspace), 1, "opening the screen is a read");
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::commit(&workspace, &screen).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert_eq!(outcome.stderr, [SCRUBBED]);
    assert!(
        outcome.stdout.starts_with("collapse: installed deploy\n"),
        "{}",
        outcome.stdout
    );
    assert_eq!(tainted(&workspace), 0);
}

/// What an older build could leave: the same place gathered twice, once with
/// a token in its URL, into the folder `older`, and once, later, without.
fn gathered_twice(workspace: &FlayerWorkspace) -> rusqlite::Connection {
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute_batch(
            "UPDATE sources SET url = 'https://example.com/acme/skills', last_seen = 20;
             INSERT INTO sources (kind, url, reference, subdirectory, directory, first_seen, last_seen)
                 SELECT kind, 'https://me:ghp_t0ken@example.com/acme/skills', reference,
                        subdirectory, 'older', 0, 0 FROM sources;
             INSERT INTO artifacts (kind, name, source_id, path, summary, revision, gathered_at)
                 SELECT kind, name, (SELECT id FROM sources WHERE directory = 'older'), path,
                        summary, revision, 0 FROM artifacts;",
        )
        .unwrap();
    database
}

/// Put the cursor on the row offering `name` from the source gathered into
/// `directory`, and mark it.
fn mark_from(screen: &mut Screen, name: &str, directory: &str) {
    use mindflayer_core::install::Offer;

    let target = &mut screen.targets[0];
    target.cursor = target
        .rows
        .iter()
        .position(|row| {
            row.candidate.name() == name
                && matches!(&row.candidate.offer, Offer::Shelf(gathered)
                    if gathered.source.directory == directory)
        })
        .unwrap();
    press(screen, &[KeyCode::Right, KeyCode::Char(' ')]);
}

#[test]
fn a_mark_on_a_source_the_scrub_makes_one_with_another_installs_from_the_one_kept_on_record() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    gathered_twice(&workspace);
    let (projects, _) = install_screen::registered(&workspace);
    let mut screen = install_screen::open(&workspace, projects).unwrap();
    // Twice, as the file has it until a write scrubs it: a read takes the
    // token out of what it shows, not the row out of the file.
    assert_eq!(screen.targets[0].rows.len(), 2, "{}", rendered(&screen));

    // The row the scrub drops, whose source is gone by the time it installs.
    mark_from(&mut screen, "deploy", "older");
    let outcome = install_screen::commit(&workspace, &screen).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(dir.path().join("collapse/skills/deploy/SKILL.md").is_file());
    assert_eq!(
        workspace
            .ledger_for_reading()
            .unwrap()
            .installations("collapse", Kind::Skill)
            .unwrap(),
        ["deploy"],
        "recorded, so Mindflayer still counts the copy as its own"
    );
}

#[test]
fn a_mark_on_what_only_the_source_the_scrub_drops_had_is_said_and_left_undone() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    gathered_twice(&workspace)
        .execute(
            "INSERT INTO artifacts (kind, name, source_id, path, gathered_at)
                 SELECT 'skill', 'lint', id, '.mindflayer/skills/older/lint', 0
                   FROM sources WHERE directory = 'older'",
            [],
        )
        .unwrap();
    let (projects, _) = install_screen::registered(&workspace);
    let mut screen = install_screen::open(&workspace, projects).unwrap();

    mark_from(&mut screen, "lint", "older");
    let outcome = install_screen::commit(&workspace, &screen).unwrap();

    assert!(!outcome.ok);
    assert_eq!(
        outcome.stderr,
        [
            SCRUBBED,
            "warning: collapse: lint from https://example.com/acme/skills is no longer on the \
             shelf once the ledger was scrubbed, so it was left as it was"
        ]
    );
    assert!(!dir.path().join("collapse/skills/lint").exists());
}

#[test]
fn installing_and_uninstalling_one_skill_scrub_the_ledger_first() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    token_in_the_shelf(&workspace);

    let installed = flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    assert!(installed.ok, "{:?}", installed.stderr);
    assert_eq!(installed.stderr, [SCRUBBED]);
    assert_eq!(
        installed.stdout,
        "installed deploy into collapse, from https://example.com/acme/skills\n"
    );

    token_in_the_shelf(&workspace);
    let removed = mind(&dir.path().join("collapse"), &["uninstall", "deploy"]).unwrap();
    assert!(removed.ok, "{:?}", removed.stderr);
    assert_eq!(removed.stderr, [SCRUBBED]);
}
