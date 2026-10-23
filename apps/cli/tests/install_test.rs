//! The install screen and the rule it works by.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// install
//
// The screen is driven the way a person drives it — a sequence of key presses
// — and drawn into an in-memory terminal, so the two things a user actually
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
