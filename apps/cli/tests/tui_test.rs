//! The TUI, driven key by key through the state machine and drawn into an
//! in-memory terminal.

mod common;

use common::*;

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

    assert!(dir.path().join("skills/commit-style/SKILL.md").is_file());
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
