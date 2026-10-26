//! The TUI, driven key by key through the state machine and drawn into an
//! in-memory terminal.

mod common;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use tui::state::Step as TuiStep;

// ---------------------------------------------------------------------------
// The TUI
//
// Driven the way the install screen is: keys pressed at the state machine,
// the work it asks for done by `tui::step` exactly as the loop does it, and
// the screen drawn into an in-memory terminal. Only the loop reading a real
// keyboard is left untested, and it holds nothing but the loop.
// ---------------------------------------------------------------------------

#[test]
fn flayer_on_its_own_is_the_tui_at_both_spellings() {
    let bare = FlayerCli::try_parse_from(["flayer"]).unwrap();
    let named = FlayerCli::try_parse_from(["flayer", "tui"]).unwrap();
    let long = Cli::try_parse_from(["mind", "flayer"]).unwrap();

    assert!(bare.command.is_none());
    assert!(matches!(
        named.command,
        Some(mindflayer_cli::FlayerCommand::Tui)
    ));
    assert!(matches!(
        long.command,
        Some(mindflayer_cli::Command::Flayer { command: None })
    ));
}

#[test]
fn the_tui_opens_on_every_linked_project_and_what_it_holds() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Projects (2)"), "{drawn}");
    assert!(drawn.contains("alpha"), "{drawn}");
    assert!(drawn.contains("beta"), "{drawn}");
    assert!(
        drawn.contains("A skill"),
        "the first project's skill:\n{drawn}"
    );

    press_key(&mut app, KeyCode::Down);
    assert_eq!(app.member().unwrap().name, "beta");
}

#[test]
fn without_a_workspace_the_tui_offers_to_create_one_here() {
    let dir = TempDir::new().unwrap();
    let mut app = tui_in(dir.path());
    assert!(matches!(app.view, View::Nowhere));
    assert!(tui_drawn(&app).contains("No flayer workspace here"));

    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(task, Some(Task::InitWorkspace));
    assert!(dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
    assert!(matches!(app.view, View::Workspace(_)));
    assert!(app.transcript().starts_with("$ flayer init\n"));
}

#[test]
fn a_new_workspace_goes_straight_to_linking_what_is_under_it() {
    let dir = TempDir::new().unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Linking(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("alpha"));
    let task = press_and_run(&mut app, KeyCode::Enter);
    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("alpha")
        })
    );
    assert!(registry(dir.path()).contains("\"alpha\""));
    // Nothing is left to link, so it is back on the projects.
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(app.member().unwrap().name, "alpha");
}

#[test]
fn a_repository_is_made_a_mind_project_only_when_that_is_confirmed() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    fs::create_dir_all(dir.path().join("gamma/.git")).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('l'));
    assert_eq!(press_key(&mut app, KeyCode::Enter), tui::state::Step::Stay);
    assert!(matches!(app.mode, Mode::Confirming(_)));
    assert!(tui_drawn(&app).contains("mind -C gamma init && flayer link gamma"));

    // No: back to the form, and nothing written.
    press_key(&mut app, KeyCode::Char('n'));
    assert!(matches!(app.mode, Mode::Linking(_)));
    assert!(!dir.path().join("gamma").join(MIND_DIR).exists());

    press_key(&mut app, KeyCode::Enter);
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::InitAndLink {
            path: String::from("gamma")
        })
    );
    assert!(dir.path().join("gamma").join(MIND_DIR).is_dir());
    assert!(registry(dir.path()).contains("\"gamma\""));
    let transcript = app.transcript();
    assert!(
        transcript.contains("$ mind -C gamma init\n"),
        "{transcript}"
    );
    assert!(transcript.contains("$ flayer link gamma\n"), "{transcript}");
}

#[test]
fn a_typed_path_that_is_not_a_project_is_asked_about_too() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    // Not a repository either, so the workspace could not have offered it.
    fs::create_dir(dir.path().join("plain")).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('l'));
    type_text(&mut app, "plain");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
    assert!(
        registry(dir.path()).contains("projects = []"),
        "nothing yet"
    );
    press_and_run(&mut app, KeyCode::Char('y'));
    assert!(registry(dir.path()).contains("\"plain\""));
}

#[test]
fn m_opens_the_minds_screen_with_the_linked_ones_ticked() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('m'));

    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let rows: Vec<(&str, bool)> = screen
        .rows
        .iter()
        .map(|row| (row.path.as_str(), row.ticked))
        .collect();
    assert_eq!(
        rows,
        [
            ("alpha", true),
            ("beta", true),
            ("gamma", false),
            ("repo", false)
        ]
    );
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("[x] alpha"), "{drawn}");
    assert!(drawn.contains("[ ] repo"), "{drawn}");
    assert!(drawn.contains("mind init first"), "{drawn}");
}

#[test]
fn applying_the_minds_screen_links_and_unlinks_in_one_pass() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));

    // Untick alpha, tick gamma and repo.
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Char(' '));
    assert!(tui_drawn(&app).contains("- unlink"));
    press_key(&mut app, KeyCode::Char('a'));

    // Asked first, naming the repository `mind init` will write into.
    assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Link 2 and unlink 1?"), "{drawn}");
    assert!(drawn.contains("repo is not a mind project yet"), "{drawn}");
    assert!(!registry(dir.path()).contains("\"gamma\""), "nothing yet");

    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Minds {
            init: vec![String::from("repo")],
            link: vec![String::from("gamma"), String::from("repo")],
            unlink: vec![String::from("alpha")],
        })
    );
    let registry = registry(dir.path());
    assert!(registry.contains("\"gamma\""), "{registry}");
    assert!(registry.contains("\"repo\""), "{registry}");
    assert!(registry.contains("\"beta\""), "{registry}");
    assert!(!registry.contains("\"alpha\""), "{registry}");
    assert!(dir.path().join("repo").join(MIND_DIR).is_dir());
    let transcript = app.transcript();
    assert!(transcript.contains("$ mind -C repo init\n"), "{transcript}");
    assert!(
        transcript.contains("$ flayer link gamma repo\n"),
        "{transcript}"
    );
    assert!(
        transcript.contains("$ flayer unlink alpha\n"),
        "{transcript}"
    );
}

#[test]
fn leaving_the_minds_screen_applies_nothing() {
    let dir = workspace_with_candidates();
    let before = registry(dir.path());
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));
    press_key(&mut app, KeyCode::Char(' '));

    press_key(&mut app, KeyCode::Esc);

    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.mode);
    assert_eq!(registry(dir.path()), before);
}

#[test]
fn applying_an_untouched_minds_screen_asks_nothing() {
    let dir = workspace_with_candidates();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('m'));

    press_key(&mut app, KeyCode::Char('a'));

    assert!(matches!(app.mode, Mode::Minds(_)), "{:?}", app.mode);
}

#[test]
fn flayer_minds_is_a_command_at_both_spellings() {
    let direct = FlayerCli::try_parse_from(["flayer", "minds"]).unwrap();
    let nested = Cli::try_parse_from(["mind", "flayer", "minds"]).unwrap();

    assert!(matches!(
        direct.command,
        Some(mindflayer_cli::FlayerCommand::Minds)
    ));
    assert!(matches!(
        nested.command,
        Some(mindflayer_cli::Command::Flayer {
            command: Some(mindflayer_cli::FlayerCommand::Minds)
        })
    ));
}

#[test]
fn adding_in_the_tui_is_mind_add() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Ship the service");
    // The form says what it is about to run, as it would be typed.
    assert!(tui_drawn(&app).contains("mind -C alpha add skill deploy 'Ship the service'"));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: String::from("Ship the service"),
            template: None,
        })
    );
    assert!(dir.path().join("alpha/skills/deploy/SKILL.md").is_file());
    // And the cursor is on what was just made.
    assert_eq!(app.current_item().unwrap().name, "deploy");
    assert_eq!(app.focus, tui::state::Focus::Items);
    assert!(app
        .transcript()
        .contains("$ mind -C alpha add skill deploy 'Ship the service'\ncreated skill deploy at "));
}

#[test]
fn switching_the_kind_adds_a_rule_instead() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    // Up past the template line to the kind, and back down to the name.
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "git/no-force-push");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Never force-push");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(dir
        .path()
        .join("alpha/rules/git/no-force-push.md")
        .is_file());
}

#[test]
fn a_rejected_name_keeps_the_form_open_with_the_reason_in_it() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "Bad Name");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Whatever");
    press_and_run(&mut app, KeyCode::Enter);

    let Mode::Adding(form) = &app.mode else {
        panic!("the form closed: {:?}", app.mode);
    };
    assert!(form.error.as_deref().unwrap().contains("`Bad Name`"));
    assert_eq!(form.name.text(), "Bad Name", "what was typed is kept");
    assert!(!dir.path().join("alpha/skills/Bad Name").exists());
}

#[test]
fn unlinking_asks_first_and_forgets_without_deleting() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('u'));
    assert!(tui_drawn(&app).contains("Unlink alpha?"));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Unlink {
            entry: String::from("alpha")
        })
    );
    assert!(!registry(dir.path()).contains("\"alpha\""));
    assert!(dir.path().join("alpha").join(MIND_DIR).is_dir());
    assert_eq!(app.workspace().unwrap().projects(), 1);
}

#[test]
fn a_stale_entry_is_shown_where_it_can_be_unlinked() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("! alpha"), "{drawn}");
    assert!(drawn.contains("cannot be opened"), "{drawn}");

    press_key(&mut app, KeyCode::Char('u'));
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!registry(dir.path()).contains("\"alpha\""));
}

#[test]
fn enter_on_an_artifact_shows_it_as_mind_show_does() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Right);
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Show {
            holder: Holder::Project(String::from("alpha")),
            reference: String::from("skill:alpha"),
        })
    );
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.title, "mind -C alpha show skill:alpha");
    let shown = mind(&dir.path().join("alpha"), &["show", "skill:alpha"]).unwrap();
    let lines: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines, shown.stdout.lines().collect::<Vec<_>>());
    // Reading output changes nothing, so it is not in the transcript.
    assert_eq!(app.transcript(), "");
}

#[test]
fn an_invalid_artifact_is_marked_where_it_is_listed() {
    let dir = workspace_with_two();
    write_skill(
        &dir.path().join("alpha"),
        "deploy",
        &skill_file("Deployment", "Ship it"),
    );
    let app = tui_in(dir.path());

    let drawn = tui_drawn(&app);

    assert!(drawn.contains("✗"), "{drawn}");
    assert!(drawn.contains("the directory is `deploy`"), "{drawn}");
}

#[test]
fn validate_in_the_tui_reads_as_flayer_validate_does() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('v'));

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    let expected = flayer(dir.path(), &["validate"]).unwrap().stdout;
    let lines: Vec<&str> = reader.lines.iter().map(|(_, line)| line.as_str()).collect();
    assert_eq!(lines, expected.lines().collect::<Vec<_>>());
    press_key(&mut app, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn a_stand_in_runner_is_handed_the_lines_a_task_is() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    let heard = Arc::new(Mutex::new(Vec::new()));
    app.runner = Some(Runner::new({
        let heard = Arc::clone(&heard);
        move |words, _, _, _| {
            heard.lock().unwrap().push(words.to_vec());
            Done {
                command: words.join(" "),
                stdout: String::from("stood in\n"),
                stderr: Vec::new(),
                error: None,
                ok: true,
            }
        }
    }));

    press_and_run(&mut app, KeyCode::Char('v'));

    assert_eq!(*heard.lock().unwrap(), [["flayer", "validate"]]);
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "stood in");
}

#[test]
fn gathering_in_the_tui_is_flayer_gather_git() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, &url);
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Gather {
            url: url.clone(),
            folder: String::from("skills"),
            reference: None,
        })
    );
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(reader.lines.iter().any(|(_, line)| line.contains("added")));
    assert_eq!(app.workspace().unwrap().shelf, 1);
    assert!(app.transcript().contains("$ flayer gather git "));
}

#[test]
fn a_gather_runs_apart_with_its_progress_drawn_until_it_is_over() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, SLOW);
    let TuiStep::Run(task) = press_key(&mut app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    tui::step(&mut app, task);

    // Back at once, with the gather still going and the screen drawing it.
    assert!(app.running.is_some());
    settle_until(&mut app, |app| {
        reported(app) == Some(Progress::Harvesting { done: 0, total: 3 })
    });
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains(&format!("flayer gather git {SLOW}")),
        "{drawn}"
    );
    assert!(drawn.contains("reading folders 0/3"), "{drawn}");
    // A key for anything else was aimed at a screen about to change.
    assert_eq!(press_key(&mut app, KeyCode::Char('q')), TuiStep::Stay);

    go.send(()).unwrap();
    settle_until(&mut app, |app| {
        reported(app) == Some(Progress::Harvesting { done: 1, total: 3 })
    });
    assert!(tui_drawn(&app).contains("reading folders 1/3"));

    for _ in 0..3 {
        go.send(()).unwrap();
    }
    settle_until(&mut app, |app| app.running.is_none());

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "3 skills: 3 added");
    assert!(app
        .transcript()
        .contains(&format!("$ flayer gather git {SLOW}\n3 skills: 3 added\n")));
}

#[test]
fn esc_cancels_a_running_gather_and_the_form_says_so() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);
    assert!(tui_drawn(&app).contains("esc cancels"));

    assert_eq!(press_key(&mut app, KeyCode::Esc), TuiStep::Stay);

    // Asked, and said to be asked, until the gather has heard it.
    assert!(app.running.as_ref().unwrap().cancelling);
    assert!(tui_drawn(&app).contains("cancelling, before anything is placed"));
    settle_until(&mut app, |app| app.running.is_none());
    // Back in the form it was started from, with why it stopped, as for any
    // gather that did not get as far as a report.
    let Mode::Gathering(form) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(
        form.error.as_deref(),
        Some("cancelled, with nothing put on the shelf")
    );
    assert!(app.transcript().contains(&format!(
        "$ flayer gather git {SLOW}\nerror: cancelled, with nothing put on the shelf\n"
    )));
}

#[test]
fn a_gather_already_placing_says_a_cancel_is_too_late_and_finishes() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (go, gate) = mpsc::channel();
    app.runner = Some(placing_gather(gate));
    start_gather(&mut app, Progress::Placing { done: 0, total: 3 });
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("placing on the shelf 0/3"), "{drawn}");
    // No key offered that would change nothing.
    assert!(!drawn.contains("esc cancels"), "{drawn}");
    assert!(drawn.contains("too late to cancel: placing"), "{drawn}");

    assert_eq!(press_key(&mut app, KeyCode::Esc), TuiStep::Stay);

    // Nor a stop claimed that the gather will not make.
    let drawn = tui_drawn(&app);
    assert!(!drawn.contains("cancelling"), "{drawn}");
    assert!(drawn.contains("too late to cancel: placing"), "{drawn}");
    go.send(()).unwrap();
    settle_until(&mut app, |app| app.running.is_none());
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(reader.lines[0].1, "3 skills: 3 added");
}

#[test]
fn leaving_while_a_gather_runs_cancels_it_and_waits_for_it() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);

    let step = app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(step, TuiStep::Quit);
    // What the loop does before it returns.
    tui::wait(&mut app);
    assert!(app.running.is_none());
    assert!(app
        .transcript()
        .contains("error: cancelled, with nothing put on the shelf"));
}

#[test]
fn the_working_boxes_are_drawn_on_a_terminal_narrower_than_they_are() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());
    let (_go, gate) = mpsc::channel();
    app.runner = Some(slow_gather(gate));
    start_slow_gather(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(20, 30)).unwrap();

    // Drawn on every frame while the gather runs, so a pane split narrow
    // halfway through must not end the process, and the gather with it.
    terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
    app.running.as_mut().unwrap().cancel();
    tui::wait(&mut app);
    // And the box a task that runs in place is drawn with.
    app.busy = Some(format!("flayer gather git {SLOW}"));
    terminal.draw(|frame| tui::ui::draw(&app, frame)).unwrap();
}

#[test]
fn a_gather_refused_for_a_token_in_its_url_shows_the_token_neither_while_it_runs_nor_after() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, "https://me:ghp_t0ken@example.com/acme/skills");
    let task = press_and_run(&mut app, KeyCode::Enter).unwrap();

    // What the box drawn over the screen says while it runs.
    assert_eq!(
        task.describe(),
        "flayer gather git https://example.com/acme/skills"
    );
    let transcript = app.transcript();
    assert!(
        transcript.starts_with("$ flayer gather git https://example.com/acme/skills\n"),
        "{transcript}"
    );
    assert!(transcript.contains("is refused"), "{transcript}");
    assert!(!transcript.contains("ghp_t0ken"), "{transcript}");
}

#[test]
fn the_install_screen_opens_inside_the_tui_and_applies_from_there() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("Skills in collapse"));

    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(task, Some(Task::ApplyInstall));
    assert!(dir.path().join("collapse/skills/deploy/SKILL.md").is_file());
    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(reader
        .lines
        .iter()
        .any(|(_, line)| line == "collapse: installed deploy"));
    assert!(app
        .transcript()
        .contains("$ flayer install\ncollapse: installed deploy"));
}

#[test]
fn the_install_screen_scrubs_the_ledger_when_it_applies_and_not_when_it_opens() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
    let stored = || -> String {
        database
            .query_row("SELECT url FROM sources", [], |row| row.get(0))
            .unwrap()
    };
    let mut app = tui_in(dir.path());

    // Opened and left: a read, which writes nothing and so has nothing to say.
    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    press_key(&mut app, KeyCode::Char('q'));
    assert_eq!(app.transcript(), "");
    assert!(stored().contains("ghp_t0ken"));

    press_and_run(&mut app, KeyCode::Char('i'));
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char(' '));
    press_key(&mut app, KeyCode::Char('a'));
    press_and_run(&mut app, KeyCode::Char('y'));

    let transcript = app.transcript();
    assert!(
        transcript.starts_with("$ flayer install\ncollapse: installed deploy\n"),
        "{transcript}"
    );
    assert!(
        transcript.contains(
            "note: https://example.com/acme/skills: the user name, password or token an older \
             Mindflayer stored in this URL is taken out"
        ),
        "{transcript}"
    );
    assert!(!transcript.contains("ghp_t0ken"), "{transcript}");
    assert_eq!(stored(), "https://example.com/acme/skills");
}

#[test]
fn leaving_the_install_screen_goes_back_rather_than_out() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    let mut app = tui_in(dir.path());
    press_and_run(&mut app, KeyCode::Char('i'));

    assert_eq!(
        press_key(&mut app, KeyCode::Char('q')),
        tui::state::Step::Stay
    );
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn ctrl_c_leaves_even_from_the_middle_of_a_form() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "half");

    let step = app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(step, tui::state::Step::Quit);
}

#[test]
fn every_task_the_tui_runs_is_a_line_the_binaries_accept() {
    // Free text that looks like a flag, carries quotes, and a directory that
    // starts with a dash: the shapes that break a naive command line.
    let awkward = String::from("- the fast way, it's 'quoted'");
    let tasks = vec![
        Task::InitWorkspace,
        Task::Link {
            path: String::from("../collapse"),
        },
        Task::InitAndLink {
            path: String::from("-odd"),
        },
        Task::Unlink {
            entry: String::from("alpha"),
        },
        Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: awkward.clone(),
            template: None,
        },
        Task::Show {
            holder: Holder::Project(String::from("alpha")),
            reference: String::from("rule:git/no-force-push"),
        },
        Task::Validate {
            holder: Holder::Workspace,
        },
        Task::Validate {
            holder: Holder::Project(String::from(".")),
        },
        Task::Shelf,
        Task::Gather {
            url: String::from("https://example.com/skills.git"),
            folder: String::from("agents"),
            reference: Some(String::from("-weird")),
        },
        Task::Add {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            description: String::from("Never"),
            template: Some(String::from("policy")),
        },
        Task::Edit {
            holder: Holder::Project(String::from("-odd")),
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
        },
        Task::Rename {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            from: String::from("deploy"),
            to: String::from("ship"),
        },
        Task::Remove {
            holder: Holder::Project(String::from("alpha")),
            kind: Kind::Skill,
            name: String::from("deploy"),
        },
        Task::Add {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: String::from("commit-style"),
            description: String::from("Shared"),
            template: Some(String::from("full")),
        },
        Task::Edit {
            holder: Holder::Workspace,
            kind: Kind::Skill,
            name: String::from("commit-style"),
        },
        Task::Remove {
            holder: Holder::Project(String::from(".")),
            kind: Kind::Rule,
            name: String::from("style"),
        },
        Task::InitProject,
        Task::LinkHere,
        Task::Load {
            path: String::from("../team"),
        },
        Task::Load {
            path: String::from("-odd"),
        },
        Task::LoadHere {
            path: String::from("../team"),
        },
        Task::Unload {
            entry: String::from("../team"),
        },
        Task::UnloadHere {
            path: String::from("/somewhere/team"),
        },
        Task::Remove {
            holder: Holder::Elsewhere(String::from("../collapse")),
            kind: Kind::Skill,
            name: String::from("deploy"),
        },
        Task::Add {
            holder: Holder::Elsewhere(String::from("-odd")),
            kind: Kind::Skill,
            name: String::from("deploy"),
            description: awkward.clone(),
            template: Some(String::from("-weird")),
        },
    ];

    for task in &tasks {
        for line in task.commands() {
            let parsed = match line[0].as_str() {
                "mind" => Cli::try_parse_from(&line).err(),
                "flayer" => FlayerCli::try_parse_from(&line).err(),
                other => panic!("{task:?} runs `{other}`"),
            };
            assert!(parsed.is_none(), "{line:?} does not parse: {parsed:?}");
        }
    }

    // And the free text arrives word for word.
    let line = tasks[4].commands().remove(0);
    let cli = Cli::try_parse_from(&line).unwrap();
    let Some(mindflayer_cli::Command::Add { description, .. }) = cli.command else {
        panic!("{line:?}");
    };
    assert_eq!(description, awkward);
}

// ---------------------------------------------------------------------------
// The same, from the TUI
// ---------------------------------------------------------------------------

#[test]
fn e_on_an_artifact_is_mind_edit_with_the_terminal_handed_over() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    let step = press_key(&mut app, KeyCode::Char('e'));

    // Not run here: it would open a real editor.
    let tui::state::Step::Run(task) = step else {
        panic!("{step:?}");
    };
    assert!(task.takes_terminal());
    assert_eq!(task.describe(), "mind -C alpha edit skill:alpha");
}

#[test]
fn keys_for_one_artifact_want_the_artifacts_column() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());

    // The cursor over there is dimmed from here, and deleting what a dimmed
    // cursor rests on is deleting the wrong thing.
    for key in ['e', 'r', 'd'] {
        assert_eq!(
            press_key(&mut app, KeyCode::Char(key)),
            tui::state::Step::Stay
        );
        assert!(matches!(app.mode, Mode::Browse));
    }
    assert!(tui_drawn(&app).contains("pick an artifact first"));
}

#[test]
fn d_asks_before_it_runs_mind_remove() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Delete skill alpha?"), "{drawn}");
    assert!(
        drawn.contains("mind -C alpha remove --yes skill:alpha"),
        "{drawn}"
    );
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("alpha/skills/alpha").exists());
    assert!(app
        .transcript()
        .contains("$ mind -C alpha remove --yes skill:alpha\nremoved skill alpha"));
}

#[test]
fn r_renames_from_a_form_that_starts_out_holding_the_old_name() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);

    press_key(&mut app, KeyCode::Char('r'));
    let Mode::Renaming(form) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert_eq!(form.name.text(), "alpha");
    // Ctrl-U clears it, as at a shell prompt.
    app.press(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_text(&mut app, "first");
    assert!(tui_drawn(&app).contains("mind -C alpha rename skill:alpha first"));
    press_and_run(&mut app, KeyCode::Enter);

    assert!(dir.path().join("alpha/skills/first/SKILL.md").is_file());
    assert_eq!(app.current_item().unwrap().name, "first");
}

#[test]
fn a_refused_rename_leaves_the_form_open_with_the_reason() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('r'));

    press_and_run(&mut app, KeyCode::Enter);

    let Mode::Renaming(form) = &app.mode else {
        panic!("the form closed: {:?}", app.mode);
    };
    assert!(form.error.as_deref().unwrap().contains("already called"));
}

#[test]
fn the_add_form_offers_the_templates_the_project_can_use() {
    let dir = workspace_with_two();
    let template = dir.path().join("alpha/.mind/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nFrom the runbook.\n").unwrap();
    let mut app = tui_in(dir.path());

    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    // none, then full (built in), then runbook.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(tui_drawn(&app).contains("--template runbook"));
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(dir.path().join("alpha/skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("From the runbook."), "{manifest}");
}

#[test]
fn the_add_form_offers_what_the_workspace_in_your_home_has_to_a_project_elsewhere() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let project = elsewhere.path().join("collapse");
    fs::create_dir(&project).unwrap();
    mind(&project, &["init"]).unwrap();
    flayer_at_home(elsewhere.path(), home.path(), &["link", "collapse"]).unwrap();
    let template = home.path().join(".mindflayer/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nFrom home.\n").unwrap();
    let mut app = App::opened_on(
        Scope::Project,
        project.clone(),
        tui::load_project(&project, &Lookup::at_home(Some(home.path()))).unwrap(),
    )
    .with_lookup(Lookup::at_home(Some(home.path())));
    app.focus_on(&project);

    // The form runs `mind -C <entry> add`, which consults the workspace in
    // your home about a project nothing above manages; it offers the same.
    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(tui_drawn(&app).contains("--template runbook"));
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(project.join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("From home."), "{manifest}");
}

#[test]
fn mind_calls_the_workspace_in_your_home_the_default_only_when_it_is_not_above() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let far = elsewhere.path().join("far");
    let near = home.path().join("near");
    for project in [&far, &near] {
        fs::create_dir(project).unwrap();
        mind(project, &["init"]).unwrap();
    }
    flayer_at_home(elsewhere.path(), home.path(), &["link", "far"]).unwrap();
    flayer_at_home(home.path(), home.path(), &["link", "near"]).unwrap();
    let default =
        |project: &Path| match tui::load_project(project, &Lookup::at_home(Some(home.path())))
            .unwrap()
        {
            View::Workspace(workspace) => workspace.default,
            View::Nowhere => panic!("no screen for {}", project.display()),
        };

    // Above `near`, the walk up finds it as it would any other workspace.
    assert!(default(&far));
    assert!(!default(&near));
}

/// The TUI `flayer` opens in `dir`, with nothing above it, where the default
/// workspace lives in `home`.
fn tui_at_home(dir: &Path, home: &Path) -> App {
    let lookup = Lookup::at_home(Some(home));
    App::new(dir.to_path_buf(), tui::load(dir, &lookup).unwrap()).with_lookup(lookup)
}

#[test]
fn with_the_default_workspace_not_made_the_tui_opens_on_it_empty() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let app = tui_at_home(elsewhere.path(), home.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("default workspace, not made yet"), "{drawn}");
    assert!(drawn.contains("nothing on the shelf"), "{drawn}");
    assert!(drawn.contains("Projects (0)"), "{drawn}");
    assert!(drawn.contains("nothing here yet"), "{drawn}");
    assert!(app
        .message
        .as_ref()
        .is_some_and(|message| message.text.contains("is made by the first change")));
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn reading_from_the_tui_leaves_the_default_workspace_unmade() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    assert_eq!(
        press_and_run(&mut app, KeyCode::Char('s')),
        Some(Task::Shelf)
    );
    press_key(&mut app, KeyCode::Esc);
    press_and_run(&mut app, KeyCode::Char('v'));
    press_key(&mut app, KeyCode::Esc);
    assert_eq!(
        press_and_run(&mut app, KeyCode::Char('i')),
        Some(Task::OpenInstall)
    );

    assert_eq!(
        app.message.as_ref().map(|message| message.text.as_str()),
        Some("no projects to install into yet — l links one")
    );
    assert!(!home.path().join(FLAYER_DIR).exists());
    assert_eq!(app.transcript(), "");
}

#[test]
fn the_minds_screen_lists_what_is_under_the_default_workspace_before_it_is_made() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    press_key(&mut app, KeyCode::Char('m'));

    assert!(matches!(app.mode, Mode::Minds(_)), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("alpha"));
    assert!(!home.path().join(FLAYER_DIR).exists());
}

/// The paths the minds screen lists, in its order.
fn minds_rows(app: &App) -> Vec<&str> {
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    screen.rows.iter().map(|row| row.path.as_str()).collect()
}

/// A home whose default workspace is not made yet, sitting inside a
/// workspace that is: `workspace_with_two`, with `h/gamma`, a mind project,
/// under it, and `h` the home.
fn home_inside_a_workspace() -> (TempDir, PathBuf) {
    let outer = workspace_with_two();
    let home = outer.path().join("h");
    let gamma = home.join("gamma");
    fs::create_dir_all(&gamma).unwrap();
    mind(&gamma, &["init"]).unwrap();
    (outer, home)
}

#[test]
fn flayer_minds_opens_on_the_default_workspace_without_making_it() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();

    let app = tui::minds(elsewhere.path(), &Lookup::at_home(Some(home.path()))).unwrap();

    assert_eq!(minds_rows(&app), vec!["alpha"]);
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn flayer_minds_with_no_workspace_and_no_default_says_so() {
    let elsewhere = TempDir::new().unwrap();

    let error = tui::minds(elsewhere.path(), &Lookup::default()).unwrap_err();

    assert!(matches!(error, CliError::NotInWorkspace(_)), "{error}");
}

#[test]
fn flayer_minds_opens_on_the_default_workspace_not_on_one_above_its_home() {
    let (_outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();

    let app = tui::minds(elsewhere.path(), &Lookup::at_home(Some(&home))).unwrap();

    // Not `alpha`, `beta` and `h/gamma`: those are the workspace above.
    assert_eq!(minds_rows(&app), vec!["gamma"]);
    assert_eq!(app.base(), home);
    assert!(!home.join(FLAYER_DIR).exists());
}

#[test]
fn the_install_screen_opens_on_the_default_workspace_not_on_one_above_its_home() {
    let (_outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_and_run(&mut app, KeyCode::Char('i'));

    // The workspace above has `alpha` and `beta` to install into; the one on
    // screen has nothing yet.
    assert!(!matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    assert_eq!(
        app.message.as_ref().map(|message| message.text.as_str()),
        Some("no projects to install into yet — l links one")
    );
    assert!(!home.join(FLAYER_DIR).exists());
}

#[test]
fn the_first_change_from_the_tui_makes_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let alpha = home.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = tui_at_home(elsewhere.path(), home.path());

    press_key(&mut app, KeyCode::Char('l'));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("alpha")
        })
    );
    assert!(registry(home.path()).contains("\"alpha\""));
    let View::Workspace(workspace) = &app.view else {
        panic!("{:?}", app.view)
    };
    assert!(workspace.made);
    assert!(workspace.default);
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("default workspace · "), "{drawn}");
    // Made by a line of its own, where the screen showed it.
    assert_eq!(
        app.transcript(),
        format!(
            "$ flayer init\ninitialized flayer workspace `{}` in {}\n\
             $ flayer link alpha\nlinked alpha as alpha\n",
            home.path().file_name().unwrap().to_string_lossy(),
            home.path().join(FLAYER_DIR).display()
        )
    );
}

#[test]
fn the_first_change_from_the_tui_lands_in_the_default_workspace_not_in_one_above_its_home() {
    let (outer, home) = home_inside_a_workspace();
    let before = registry(outer.path());
    let elsewhere = TempDir::new().unwrap();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_key(&mut app, KeyCode::Char('l'));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Link {
            path: String::from("gamma")
        })
    );
    assert!(registry(&home).contains("\"gamma\""));
    assert_eq!(registry(outer.path()), before);
    let View::Workspace(workspace) = &app.view else {
        panic!("{:?}", app.view)
    };
    assert!(workspace.made);
    assert!(workspace.default);
    assert_eq!(workspace.root, home);
}

#[test]
fn a_gather_from_the_tui_runs_apart_after_making_the_default_workspace_where_it_is_shown() {
    let (outer, home) = home_inside_a_workspace();
    let elsewhere = TempDir::new().unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let mut app = tui_at_home(elsewhere.path(), &home);

    press_key(&mut app, KeyCode::Char('g'));
    type_text(&mut app, &url);
    let TuiStep::Run(task) = press_key(&mut app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    assert!(task.runs_apart());
    tui::step(&mut app, task);
    tui::wait(&mut app);

    // Made first, by a line of its own, on the thread the gather ran on: left
    // to the gather, the walk up from home would have found the workspace
    // above it.
    let transcript = app.transcript();
    assert!(transcript.starts_with("$ flayer init\n"), "{transcript}");
    assert!(transcript.contains("$ flayer gather git "), "{transcript}");
    let workspace = app.workspace().unwrap();
    assert!(workspace.made);
    assert_eq!(workspace.root, home);
    assert_eq!(workspace.shelf, 1);
    let above = flayer(outer.path(), &["gather", "list"]).unwrap();
    assert!(
        above.stdout.starts_with("nothing gathered yet"),
        "{}",
        above.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_listed_where_it_can_be_dealt_with() {
    let dir = workspace_with_two();
    write_skill(
        &dir.path().join("alpha"),
        "broken",
        "no front matter at all\n",
    );
    let mut app = tui_in(dir.path());

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("broken"), "{drawn}");
    assert!(drawn.contains("cannot be read"), "{drawn}");
    assert!(
        drawn.contains("front matter fence"),
        "the reason too:\n{drawn}"
    );

    // alpha, then broken: the broken one sits where it would have been.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    assert_eq!(app.current_item().unwrap().name, "broken");
    press_key(&mut app, KeyCode::Char('d'));
    press_and_run(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("alpha/skills/broken").exists());
}

// ---------------------------------------------------------------------------
// Both levels, in the TUI
// ---------------------------------------------------------------------------

#[test]
fn the_workspaces_own_row_sits_above_its_projects() {
    let dir = workspace_with_own();
    let mut app = tui_in(dir.path());

    // The cursor starts on the first project; the workspace's row is above.
    assert_eq!(app.member().unwrap().name, "alpha");
    let drawn = tui_drawn(&app);
    assert!(drawn.contains("Projects (2)"), "{drawn}");
    assert!(drawn.contains("◆"), "{drawn}");

    press_key(&mut app, KeyCode::Up);
    assert!(app.member().unwrap().own);
    assert!(tui_drawn(&app).contains("the workspace's own"));
    press_key(&mut app, KeyCode::Right);
    let step = press_key(&mut app, KeyCode::Char('e'));

    let tui::state::Step::Run(task) = step else {
        panic!("{step:?}");
    };
    assert_eq!(task.describe(), "flayer edit skill:commit-style");
}

#[test]
fn adding_on_the_workspaces_row_is_flayer_add() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "commit-style");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Shared");
    press_and_run(&mut app, KeyCode::Enter);

    assert!(own_directory_for(dir.path(), Kind::Skill)
        .join("commit-style/SKILL.md")
        .is_file());
    assert!(app
        .transcript()
        .contains("$ flayer add skill commit-style Shared\n"));
    assert_eq!(app.current_item().unwrap().name, "commit-style");
}

#[test]
fn the_workspaces_row_has_nothing_to_unlink() {
    let dir = workspace_with_two();
    let mut app = tui_in(dir.path());
    press_key(&mut app, KeyCode::Up);

    assert_eq!(
        press_key(&mut app, KeyCode::Char('u')),
        tui::state::Step::Stay
    );
    assert!(matches!(app.mode, Mode::Browse));
    assert!(tui_drawn(&app).contains("nothing to unlink"));
}

#[test]
fn the_install_screen_offers_the_workspaces_own_skills() {
    let dir = workspace_with_own();
    let mut app = tui_in(dir.path());

    press_and_run(&mut app, KeyCode::Char('i'));

    let drawn = tui_drawn(&app);
    assert!(drawn.contains("commit-style"), "{drawn}");
    assert!(drawn.contains("[workspace]"), "{drawn}");
}

#[test]
fn mind_opens_a_managed_project_inside_its_workspace() {
    let dir = workspace_with_two();

    let app = mind_tui_in(&dir.path().join("beta"));

    assert_eq!(app.scope(), Scope::Workspace);
    assert_eq!(app.member().unwrap().name, "beta");
    assert_eq!(app.focus, tui::state::Focus::Items);
}

#[test]
fn mind_opens_a_project_on_its_own_with_what_only_a_workspace_has_explained() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    mind(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    let mut app = mind_tui_in(dir.path());

    assert_eq!(app.scope(), Scope::Project);
    assert!(tui_drawn(&app).contains("a project no workspace manages"));
    for key in ['g', 's', 'i', 'u'] {
        assert_eq!(
            press_key(&mut app, KeyCode::Char(key)),
            tui::state::Step::Stay
        );
    }
    let tui::state::Step::Run(task) = press_key(&mut app, KeyCode::Char('v')) else {
        panic!("v runs a command");
    };
    assert_eq!(task.describe(), "mind validate");

    press_key(&mut app, KeyCode::Char('a'));
    type_text(&mut app, "review");
    press_key(&mut app, KeyCode::Tab);
    type_text(&mut app, "Review it");
    press_and_run(&mut app, KeyCode::Enter);
    assert!(dir.path().join("skills/review/SKILL.md").is_file());
    assert!(app
        .transcript()
        .contains("$ mind add skill review 'Review it'\n"));
}

#[test]
fn l_on_a_loose_project_links_it_and_the_screen_becomes_the_workspaces() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();
    let mut app = mind_tui_in(&alpha);
    assert_eq!(app.scope(), Scope::Project);

    let task = press_and_run(&mut app, KeyCode::Char('l'));

    assert_eq!(task, Some(Task::LinkHere));
    assert!(registry(dir.path()).contains("\"alpha\""));
    assert_eq!(app.scope(), Scope::Workspace);
    assert_eq!(app.member().unwrap().name, "alpha");
    assert!(app
        .transcript()
        .starts_with("$ mind link\nlinked alpha as alpha\n"));
}

#[test]
fn mind_with_no_project_offers_to_make_one() {
    let dir = TempDir::new().unwrap();
    let mut app = mind_tui_in(dir.path());
    assert!(matches!(app.view, View::Nowhere));
    assert!(tui_drawn(&app).contains("No mind project here"));

    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(task, Some(Task::InitProject));
    assert!(dir.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert_eq!(app.scope(), Scope::Project);
}

// ---------------------------------------------------------------------------
// A gather that takes its time
// ---------------------------------------------------------------------------

/// The URL the slow gather is asked for, which it never fetches.
const SLOW: &str = "https://example.com/skills.git";

/// A stand-in for `flayer gather git` that takes as long as the test wants:
/// it reports reading each of three folders, and goes on from one to the
/// next only when `go` says so. A cancel stops it before it says it is done,
/// as one stops a real gather before anything is placed.
fn slow_gather(go: mpsc::Receiver<()>) -> Runner {
    let go = Mutex::new(go);
    Runner::new(move |words, _, _, watch| {
        let go = go.lock().unwrap();
        let command = tui::state::shell(words);
        for done in 0..=3 {
            watch.report(Progress::Harvesting { done, total: 3 });
            loop {
                if watch.is_cancelled() {
                    return Done::failed(command, "cancelled, with nothing put on the shelf");
                }
                match go.recv_timeout(Duration::from_millis(5)) {
                    // A test that has gone is not waited for.
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
        Done {
            command,
            stdout: String::from("3 skills: 3 added\n"),
            stderr: Vec::new(),
            error: None,
            ok: true,
        }
    })
}

/// A stand-in for a gather past its last check: it says it is placing
/// three, and finishes when `go` says so, whatever cancel comes first, as a
/// real gather does once it has started placing.
fn placing_gather(go: mpsc::Receiver<()>) -> Runner {
    let go = Mutex::new(go);
    Runner::new(move |words, _, _, watch| {
        let go = go.lock().unwrap();
        watch.report(Progress::Placing { done: 0, total: 3 });
        // A test that has gone is not waited for.
        let _ = go.recv();
        watch.report(Progress::Placing { done: 3, total: 3 });
        Done {
            command: tui::state::shell(words),
            stdout: String::from("3 skills: 3 added\n"),
            stderr: Vec::new(),
            error: None,
            ok: true,
        }
    })
}

/// Ask for the slow gather from the gather form, and let it get as far as
/// reading its first folder.
fn start_slow_gather(app: &mut App) {
    start_gather(app, Progress::Harvesting { done: 0, total: 3 });
}

/// Ask for a gather from the gather form, and let it get as far as `step`.
fn start_gather(app: &mut App, step: Progress) {
    press_key(app, KeyCode::Char('g'));
    type_text(app, SLOW);
    let TuiStep::Run(task) = press_key(app, KeyCode::Enter) else {
        panic!("{:?}", app.mode);
    };
    tui::step(app, task);
    settle_until(app, |app| reported(app) == Some(step));
}

/// The step the task running apart last reported, as the screen has it.
fn reported(app: &App) -> Option<Progress> {
    app.running.as_ref().and_then(|running| running.progress)
}

/// Take in what the task running apart says, as the loop does before each
/// frame, until `until` holds.
fn settle_until(app: &mut App, until: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !until(app) {
        assert!(Instant::now() < deadline, "never came: {:?}", app.running);
        tui::settle(app);
        thread::sleep(Duration::from_millis(1));
    }
}

// ---------------------------------------------------------------------------
// A workspace named for the TUI
// ---------------------------------------------------------------------------

/// Under one directory: `outer`, a workspace, and `hub` beside it, which
/// links `collapse`, a project under neither, and has installed its own
/// `hub-skill` there.
fn hub_beside_outer() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let (outer, hub, collapse) = (
        dir.path().join("outer"),
        dir.path().join("hub"),
        dir.path().join("collapse"),
    );
    for root in [&outer, &hub, &collapse] {
        fs::create_dir(root).unwrap();
    }
    flayer(&outer, &["init"]).unwrap();
    flayer(&hub, &["init"]).unwrap();
    mind(&collapse, &["init"]).unwrap();
    flayer(&hub, &["link", "../collapse"]).unwrap();
    flayer(&hub, &["add", "skill", "hub-skill", "The hub's"]).unwrap();
    flayer(&hub, &["install", "-p", "collapse", "hub-skill"]).unwrap();
    (dir, outer, hub, collapse)
}

/// What `hub`'s ledger says it installed into `collapse`.
fn recorded_in_hub(hub: &Path) -> Vec<String> {
    FlayerWorkspace::open(hub)
        .unwrap()
        .ledger()
        .unwrap()
        .installations("../collapse", Kind::Skill)
        .unwrap()
}

#[test]
fn opened_on_a_named_workspace_the_tui_runs_every_line_against_it() {
    let (_dir, outer, hub, collapse) = hub_beside_outer();
    let lookup = Lookup {
        flag: Some(hub.clone()),
        ..Lookup::default()
    };
    // Opened inside `outer`, which walking up would find instead.
    let mut app = App::new(outer.clone(), tui::load(&outer, &lookup).unwrap()).with_lookup(lookup);
    assert_eq!(app.base(), hub);

    // `collapse`, then its one skill.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('d'));
    press_and_run(&mut app, KeyCode::Char('y'));

    // The line consulted the workspace on screen, not whatever is above the
    // project or in a home, so that workspace's record went with the copy.
    assert!(
        app.transcript()
            .starts_with("$ mind -C ../collapse remove --yes skill:hub-skill\n"),
        "{}",
        app.transcript()
    );
    assert!(!collapse.join("skills/hub-skill").exists());
    assert!(recorded_in_hub(&hub).is_empty());
}

#[test]
fn a_w_among_a_lines_words_is_typed_from_the_workspace_root() {
    let (_dir, outer, _hub, _collapse) = hub_beside_outer();
    let words: Vec<String> = ["flayer", "-W", "../hub", "list"]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();

    let done = tui::execute(&words, &outer, &Lookup::default(), &Watch::new());

    assert_eq!(done.error, None);
    assert!(done.stdout.contains("hub-skill"), "{}", done.stdout);
}

// ---------------------------------------------------------------------------
// A project the workspace on screen manages from elsewhere
// ---------------------------------------------------------------------------

#[test]
fn a_project_linked_from_beside_runs_its_lines_against_the_workspace_on_screen() {
    let (_dir, _outer, hub, collapse) = hub_beside_outer();
    // Nothing named: the screen is the workspace found where it was opened.
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Char('d'));
    let drawn = tui_drawn(&app);
    assert!(
        drawn.contains("mind -W . -C ../collapse remove --yes skill:hub-skill"),
        "{drawn}"
    );
    press_and_run(&mut app, KeyCode::Char('y'));

    // Without `-W .`, walking up from `collapse` finds no workspace at all,
    // and the hub would keep a record of a copy that is gone.
    assert!(!collapse.join("skills/hub-skill").exists());
    assert!(recorded_in_hub(&hub).is_empty());
}

#[test]
fn a_project_linked_from_beside_is_offered_the_screens_templates() {
    let (_dir, _outer, hub, collapse) = hub_beside_outer();
    let template = hub.join(".mindflayer/templates/runbook/SKILL.md");
    fs::create_dir_all(template.parent().unwrap()).unwrap();
    fs::write(&template, "---\n---\n\n# {{name}}\n\nThe hub's runbook.\n").unwrap();
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Char('a'));
    press_key(&mut app, KeyCode::Up);
    // none, then full (built in), then runbook.
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Right);
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "deploy");
    press_key(&mut app, KeyCode::Down);
    type_text(&mut app, "Ship it");
    assert!(
        tui_drawn(&app).contains("mind -W . -C ../collapse add --template runbook"),
        "{}",
        tui_drawn(&app)
    );
    press_and_run(&mut app, KeyCode::Enter);

    let manifest = fs::read_to_string(collapse.join("skills/deploy/SKILL.md")).unwrap();
    assert!(manifest.contains("The hub's runbook."), "{manifest}");
}

#[test]
fn the_keys_help_says_which_rows_run_their_lines_with_w() {
    let (_dir, _outer, hub, _collapse) = hub_beside_outer();
    let mut app = tui_in(&hub);

    press_key(&mut app, KeyCode::Char('?'));

    let Mode::Reading(reader) = &app.mode else {
        panic!("{:?}", app.mode);
    };
    assert!(
        reader
            .lines
            .iter()
            .any(|(_, line)| line.contains("`mind -W . -C <project> <cmd>`")),
        "{:?}",
        reader.lines
    );
}

#[test]
fn a_project_under_the_workspace_on_screen_keeps_the_plain_line() {
    let dir = workspace_with_two();
    let app = tui_in(dir.path());

    assert_eq!(
        app.member().map(|member| member.holder()),
        Some(Holder::Project(String::from("alpha")))
    );
}
