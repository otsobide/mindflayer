//! `flayer load` and `unload`: another workspace's own skills offered to this
//! one's projects, read live from where it is — at both levels, and from the
//! TUI.

mod common;

use common::*;

/// Under one directory: `target`, a workspace linking its project
/// `collapse`, and `team`, a workspace beside it with a skill of its own.
fn target_and_team() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target");
    let team = dir.path().join("team");
    fs::create_dir_all(target.join("collapse")).unwrap();
    fs::create_dir_all(&team).unwrap();
    flayer(&target, &["init"]).unwrap();
    mind(&target.join("collapse"), &["init"]).unwrap();
    flayer(&target, &["link", "collapse"]).unwrap();
    flayer(&team, &["init"]).unwrap();
    flayer(&team, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    (dir, target, team)
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

#[test]
fn load_says_what_it_loaded_what_it_offers_and_where_it_went() {
    let (_dir, target, _team) = target_and_team();

    let outcome = flayer(&target, &["load", "../team"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "loaded team as ../team: 1 skill to install\n  into the workspace at {}\n",
            target.display()
        )
    );
    assert!(registry(&target).contains("loaded = [\"../team\"]"));
}

#[test]
fn load_from_inside_the_workspace_being_loaded_goes_into_the_one_above() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    flayer(&team, &["init"]).unwrap();

    let outcome = flayer(&team, &["load", "."]).unwrap();

    assert!(outcome
        .stdout
        .starts_with("loaded team as team: nothing to install yet\n"));
    assert!(registry(dir.path()).contains("loaded = [\"team\"]"));
    assert!(registry(&team).contains("loaded = []"), "not into itself");
}

#[test]
fn with_nothing_above_the_source_load_goes_into_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let (_dir, _target, team) = target_and_team();

    let outcome = flayer_at_home(&team, home.path(), &["load", "."]).unwrap();

    assert!(
        outcome.stdout.ends_with(", the default one\n"),
        "{}",
        outcome.stdout
    );
    // The one entry, pointing at team from the home workspace.
    let home_workspace = mindflayer_core::FlayerWorkspace::open(home.path()).unwrap();
    let loads = home_workspace.loads();
    assert_eq!(loads.len(), 1);
    assert_eq!(
        loads[0].root.canonicalize().unwrap(),
        team.canonicalize().unwrap()
    );
}

#[test]
fn with_no_default_either_there_is_nowhere_to_load_into() {
    let (_dir, _target, team) = target_and_team();

    let error = flayer(&team, &["load", "."]).unwrap_err();

    assert!(
        matches!(*error.error, CliError::NowhereToLoad { .. }),
        "{}",
        error.error
    );
}

#[test]
fn load_loads_none_when_one_path_is_not_a_workspace() {
    let (dir, target, _team) = target_and_team();
    fs::create_dir(dir.path().join("plain")).unwrap();

    let error = flayer(&target, &["load", "../team", "../plain"]).unwrap_err();

    assert!(
        error.error.to_string().contains("not a flayer workspace"),
        "{}",
        error.error
    );
    assert!(
        registry(&target).contains("loaded = []"),
        "half a load happened"
    );
}

#[test]
fn loading_twice_says_so() {
    let (_dir, target, _team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();

    let outcome = flayer(&target, &["load", "../team"]).unwrap();

    assert!(outcome
        .stdout
        .starts_with("team is already loaded as ../team\n"));
}

#[test]
fn unload_forgets_it_and_refuses_what_was_never_loaded() {
    let (dir, target, _team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();

    let error = flayer(&target, &["unload", "../team", "../nowhere"]).unwrap_err();
    assert!(
        error.error.to_string().contains("not loaded"),
        "{}",
        error.error
    );
    assert!(registry(&target).contains("\"../team\""), "all or nothing");

    let outcome = flayer(&target, &["unload", "../team"]).unwrap();
    assert_eq!(
        outcome.stdout,
        format!(
            "unloaded ../team\n  from the workspace at {}\n",
            target.display()
        )
    );
    assert!(registry(&target).contains("loaded = []"));
    drop(dir);
}

#[test]
fn a_loaded_skill_is_on_the_shelf_listing_and_installs_like_any_other() {
    let (_dir, target, team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();

    let shelf = flayer(&target, &["gather", "list"]).unwrap();
    assert_eq!(shelf.stdout, "deploy  load:../team  Ship the service\n");

    let outcome = flayer(&target, &["install", "-p", "collapse", "deploy"]).unwrap();
    assert_eq!(
        outcome.stdout,
        "installed deploy into collapse, from load:../team\n"
    );
    assert!(target.join("collapse/skills/deploy/SKILL.md").is_file());

    // Read live: changed there, updated here by installing again.
    let source = team.join("skills/deploy/SKILL.md");
    let edited = fs::read_to_string(&source).unwrap() + "\nOne more step.\n";
    fs::write(&source, edited).unwrap();
    let again = flayer(
        &target,
        &["install", "-p", "collapse", "deploy", "--from", "team"],
    )
    .unwrap();
    assert_eq!(
        again.stdout,
        "updated deploy in collapse, from load:../team\n"
    );
}

#[test]
fn a_loaded_workspace_that_has_gone_is_a_warning_on_the_shelf_listing() {
    let (_dir, target, team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();
    fs::remove_dir_all(&team).unwrap();

    let shelf = flayer(&target, &["gather", "list"]).unwrap();

    assert!(!shelf.ok);
    assert!(
        shelf.stdout.starts_with("nothing gathered yet"),
        "{}",
        shelf.stdout
    );
    assert!(
        shelf.stderr[0].starts_with("warning: "),
        "{:?}",
        shelf.stderr
    );
    // And it is still the one worth unloading.
    assert!(flayer(&target, &["unload", "../team"])
        .unwrap()
        .stdout
        .starts_with("unloaded ../team\n"));
}

#[test]
fn an_empty_shelf_mentions_load_too() {
    let (_dir, target, _team) = target_and_team();

    let shelf = flayer(&target, &["gather", "list"]).unwrap();

    assert!(
        shelf.stdout.contains("flayer load <path>"),
        "{}",
        shelf.stdout
    );
}

#[test]
fn mind_load_goes_into_the_workspace_above_the_project() {
    let (_dir, target, _team) = target_and_team();
    let collapse = target.join("collapse");

    let loaded = mind(&collapse, &["load", "../../team"]).unwrap();
    assert!(
        loaded.stdout.starts_with("loaded team as ../team: 1 skill"),
        "{}",
        loaded.stdout
    );
    assert!(registry(&target).contains("\"../team\""));

    let unloaded = mind(&collapse, &["unload", "../../team"]).unwrap();
    assert!(unloaded.stdout.starts_with("unloaded ../team\n"));
}

#[test]
fn load_is_reachable_the_long_way_round_too() {
    let (_dir, target, _team) = target_and_team();

    let nested = mind(&target, &["flayer", "load", "../team"]).unwrap();

    assert!(nested.stdout.starts_with("loaded team as ../team"));
}

// ---------------------------------------------------------------------------
// From the TUI
// ---------------------------------------------------------------------------

#[test]
fn capital_l_loads_a_workspace_from_the_tui() {
    let (_dir, target, _team) = target_and_team();
    let mut app = tui_in(&target);

    press_key(&mut app, KeyCode::Char('L'));
    assert!(matches!(app.mode, Mode::Loading(_)), "{:?}", app.mode);
    type_text(&mut app, "../team");
    assert!(tui_drawn(&app).contains("flayer load ../team"));
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::Load {
            path: String::from("../team")
        })
    );
    assert!(registry(&target).contains("\"../team\""));
    assert!(matches!(app.mode, Mode::Browse), "{:?}", app.mode);
    assert!(tui_drawn(&app).contains("1 loaded"));
    assert!(app
        .transcript()
        .contains("$ flayer load ../team\nloaded team as ../team"));
}

#[test]
fn the_load_form_lists_what_is_loaded_and_unloads_one_once_asked() {
    let (_dir, target, _team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();
    let mut app = tui_in(&target);

    press_key(&mut app, KeyCode::Char('L'));
    assert!(tui_drawn(&app).contains("team · 1 to install"));
    press_key(&mut app, KeyCode::Enter);
    assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    assert_eq!(
        task,
        Some(Task::Unload {
            entry: String::from("../team")
        })
    );
    assert!(registry(&target).contains("loaded = []"));
}

#[test]
fn a_path_that_is_not_a_workspace_keeps_the_load_form_open_with_why() {
    let (dir, target, _team) = target_and_team();
    fs::create_dir(dir.path().join("plain")).unwrap();
    let mut app = tui_in(&target);

    press_key(&mut app, KeyCode::Char('L'));
    press_and_run(&mut app, KeyCode::Enter);
    let Mode::Loading(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert!(form.error.is_some(), "an empty form says what it wants");

    type_text(&mut app, "../plain");
    press_and_run(&mut app, KeyCode::Enter);
    let Mode::Loading(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert!(
        form.error
            .as_deref()
            .is_some_and(|error| error.contains("not a flayer workspace")),
        "{:?}",
        form.error
    );
}

#[test]
fn the_shelf_and_the_install_screen_offer_a_loaded_skill() {
    let (_dir, target, _team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();
    let mut app = tui_in(&target);

    press_and_run(&mut app, KeyCode::Char('s'));
    let Mode::Reading(_) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert!(tui_drawn(&app).contains("deploy  load:../team"));

    press_key(&mut app, KeyCode::Esc);
    press_and_run(&mut app, KeyCode::Char('i'));
    assert!(matches!(app.mode, Mode::Installing(_)), "{:?}", app.mode);
    assert!(
        tui_drawn(&app).contains("[load:../team]"),
        "{}",
        tui_drawn(&app)
    );
}

#[test]
fn on_a_project_on_its_own_capital_l_is_mind_load() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("collapse");
    let team = dir.path().join("team");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&team).unwrap();
    mind(&project, &["init"]).unwrap();
    flayer(&team, &["init"]).unwrap();
    let mut app = App::opened_on(
        Scope::Project,
        project.clone(),
        tui::load_project(&project, Some(home.path())).unwrap(),
    )
    .at_home(Some(home.path()));
    app.focus_on(&project);

    press_key(&mut app, KeyCode::Char('L'));
    type_text(&mut app, "../team");
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(
        task,
        Some(Task::LoadHere {
            path: String::from("../team")
        })
    );
    assert!(
        registry(home.path()).contains("\"../"),
        "{}",
        registry(home.path())
    );
}

// ---------------------------------------------------------------------------
// What the review found
// ---------------------------------------------------------------------------

#[test]
fn load_on_its_own_lists_what_is_loaded() {
    let (dir, target, _team) = target_and_team();
    fs::create_dir(dir.path().join("gone")).unwrap();
    flayer(&dir.path().join("gone"), &["init"]).unwrap();
    flayer(&target, &["load", "../team", "../gone"]).unwrap();
    fs::remove_dir_all(dir.path().join("gone")).unwrap();

    let listed = flayer(&target, &["load"]).unwrap();

    assert!(
        listed
            .stdout
            .starts_with("../team  team  1 skill to install\n../gone  -     cannot be opened\n"),
        "{}",
        listed.stdout
    );
    assert!(listed.stdout.contains("  in the workspace at "));
    assert!(!listed.ok, "the one that has gone is a warning");
}

#[test]
fn nothing_loaded_says_how_to_load() {
    let (_dir, target, _team) = target_and_team();

    let listed = flayer(&target, &["load"]).unwrap();

    assert!(
        listed.stdout.starts_with("nothing loaded\n"),
        "{}",
        listed.stdout
    );
}

#[test]
fn a_failed_load_or_any_unload_never_makes_the_default_workspace() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("plain")).unwrap();

    flayer_at_home(dir.path(), home.path(), &["load", "plain"]).unwrap_err();
    flayer_at_home(dir.path(), home.path(), &["unload", "plain"]).unwrap_err();
    flayer_at_home(dir.path(), home.path(), &["load"]).unwrap_err();

    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn install_says_a_loaded_workspace_has_gone_rather_than_only_that_nothing_is_offered() {
    let (_dir, target, team) = target_and_team();
    flayer(&target, &["load", "../team"]).unwrap();
    fs::remove_dir_all(&team).unwrap();

    let error = flayer(&target, &["install", "-p", "collapse", "deploy"]).unwrap_err();

    assert!(error.error.to_string().contains("nothing called `deploy`"));
    assert!(
        error
            .warnings
            .iter()
            .any(|warning| warning.contains("not a flayer workspace")),
        "{:?}",
        error.warnings
    );
}

#[test]
fn a_broken_skill_in_a_loaded_workspace_is_a_warning() {
    let (_dir, target, team) = target_and_team();
    fs::create_dir_all(team.join("skills/broken")).unwrap();
    fs::write(team.join("skills/broken/SKILL.md"), "no front matter").unwrap();

    let loaded = flayer(&target, &["load", "../team"]).unwrap();
    let shelf = flayer(&target, &["gather", "list"]).unwrap();

    assert!(!loaded.ok, "{:?}", loaded.stderr);
    assert!(!shelf.ok);
    assert!(shelf
        .stderr
        .iter()
        .any(|warning| warning.contains("broken")));
    assert!(shelf.stdout.contains("deploy"), "the rest is still listed");
}

#[test]
fn from_takes_the_path_given_to_load_typed_from_the_project() {
    let (_dir, target, _team) = target_and_team();
    let collapse = target.join("collapse");
    // Two places offer `deploy`, so --from is needed.
    flayer(&target, &["add", "skill", "deploy", "Ours"]).unwrap();
    mind(&collapse, &["load", "../../team"]).unwrap();

    let installed = mind(&collapse, &["install", "deploy", "--from", "../../team"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed deploy into collapse, from load:../team\n"
    );
}

#[test]
fn the_tui_will_not_load_the_workspace_on_screen_into_somewhere_else() {
    let home = TempDir::new().unwrap();
    let (_dir, target, _team) = target_and_team();
    let mut app = App::new(
        target.clone(),
        tui::load(&target, Some(home.path())).unwrap(),
    )
    .at_home(Some(home.path()));

    press_key(&mut app, KeyCode::Char('L'));
    type_text(&mut app, ".");
    let task = press_and_run(&mut app, KeyCode::Enter);

    assert_eq!(task, None);
    let Mode::Loading(form) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    assert!(form
        .error
        .as_deref()
        .is_some_and(|error| error.contains("cannot load itself")));
    assert!(!home.path().join(FLAYER_DIR).exists());
}

#[test]
fn a_project_on_its_own_sees_and_unloads_what_the_workspace_above_loads() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("collapse");
    let team = dir.path().join("team");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&team).unwrap();
    mind(&project, &["init"]).unwrap();
    flayer(&team, &["init"]).unwrap();
    flayer_at_home(&project, home.path(), &["load", "../team"]).unwrap();
    let mut app = App::opened_on(
        Scope::Project,
        project.clone(),
        tui::load_project(&project, Some(home.path())).unwrap(),
    )
    .at_home(Some(home.path()));
    app.focus_on(&project);

    press_key(&mut app, KeyCode::Char('L'));
    assert!(tui_drawn(&app).contains("team · nothing to install yet"));
    press_key(&mut app, KeyCode::Enter);
    let task = press_and_run(&mut app, KeyCode::Char('y'));

    let Some(Task::UnloadHere { path }) = task else {
        panic!("{task:?}")
    };
    assert!(Path::new(&path).is_absolute(), "{path}");
    assert!(registry(home.path()).contains("loaded = []"));
}

#[test]
fn switching_to_a_loaded_copy_of_an_installed_skill_replaces_it() {
    use mindflayer_cli::install::{self as install_screen, state::Pending};
    let (_dir, target, _team) = target_and_team();
    flayer(&target, &["add", "skill", "deploy", "Ours"]).unwrap();
    flayer(&target, &["load", "../team"]).unwrap();
    flayer(
        &target,
        &["install", "-p", "collapse", "deploy", "--from", "workspace"],
    )
    .unwrap();
    let workspace = mindflayer_core::FlayerWorkspace::open(&target).unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);

    // The row of the copy that is there starts ticked, the loaded one not:
    // ticking the loaded one replaces ours, rather than removing the folder.
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "{drawn}");
    assert!(drawn.contains("[ ] deploy"), "{drawn}");
    press(
        &mut screen,
        &[KeyCode::Right, KeyCode::Down, KeyCode::Char(' ')],
    );
    let pending: Vec<Pending> = screen
        .pending()
        .into_iter()
        .map(|(_, _, what)| what)
        .collect();
    assert_eq!(pending, [Pending::Install]);
    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(
        outcome.stdout.contains("installed deploy"),
        "{}",
        outcome.stdout
    );
    let copied = fs::read_to_string(target.join("collapse/skills/deploy/SKILL.md")).unwrap();
    assert!(copied.contains("Ship the service"), "{copied}");
}

#[test]
fn a_loaded_skill_that_vanished_does_not_stop_the_rest_of_the_install_screen() {
    use mindflayer_cli::install as install_screen;
    let (_dir, target, team) = target_and_team();
    flayer(&team, &["add", "skill", "review", "Review it"]).unwrap();
    flayer(&target, &["load", "../team"]).unwrap();
    let workspace = mindflayer_core::FlayerWorkspace::open(&target).unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );
    fs::remove_dir_all(team.join("skills/deploy")).unwrap();

    let outcome = install_screen::apply(&workspace, &ledger, &screen).unwrap();

    assert!(!outcome.ok);
    assert!(
        outcome.stdout.contains("collapse: installed review"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome
            .stderr
            .iter()
            .any(|warning| warning.contains("deploy")),
        "{:?}",
        outcome.stderr
    );
}

#[test]
fn ticking_the_installed_copy_off_and_on_again_changes_nothing() {
    let (_dir, target, _team) = target_and_team();
    flayer(&target, &["add", "skill", "deploy", "Ours"]).unwrap();
    flayer(&target, &["load", "../team"]).unwrap();
    flayer(
        &target,
        &[
            "install",
            "-p",
            "collapse",
            "deploy",
            "--from",
            "load:../team",
        ],
    )
    .unwrap();
    let workspace = mindflayer_core::FlayerWorkspace::open(&target).unwrap();
    let ledger = workspace.ledger().unwrap();

    for keys in [
        vec![KeyCode::Right, KeyCode::Char(' '), KeyCode::Char(' ')],
        vec![
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Char(' '),
        ],
    ] {
        let mut screen = screen_for(&workspace, &ledger);
        press(&mut screen, &keys);
        assert!(screen.pending().is_empty(), "{keys:?}");
    }
}

#[test]
fn a_shelf_source_and_a_loaded_workspace_at_one_path_are_each_picked_by_their_own_origin() {
    let repo = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Gathered"))]);
    flayer(repo.path(), &["init"]).unwrap();
    fs::write(
        repo.path().join("skills/deploy/SKILL.md"),
        skill_file("deploy", "Loaded"),
    )
    .unwrap();
    let (_dir, target, _team) = target_and_team();
    let url = repo.path().to_string_lossy().into_owned();
    flayer(&target, &["gather", "git", &url]).unwrap();
    flayer(&target, &["load", &url]).unwrap();
    let skill = target.join("collapse/skills/deploy/SKILL.md");

    let shelf = flayer(
        &target,
        &["install", "-p", "collapse", "deploy", "--from", &url],
    )
    .unwrap();
    assert!(
        shelf.stdout.ends_with(&format!("from {url}\n")),
        "{}",
        shelf.stdout
    );
    assert!(fs::read_to_string(&skill).unwrap().contains("Gathered"));

    let loaded = flayer(
        &target,
        &[
            "install",
            "-p",
            "collapse",
            "deploy",
            "--from",
            &format!("load:{url}"),
        ],
    )
    .unwrap();
    assert!(loaded.stdout.contains("from load:"), "{}", loaded.stdout);
    assert!(fs::read_to_string(&skill).unwrap().contains("Loaded"));
}

#[cfg(unix)]
#[test]
fn unlink_of_two_spellings_of_one_project_is_one_unlink() {
    let dir = workspace_with_two();
    std::os::unix::fs::symlink(dir.path().join("alpha"), dir.path().join("alias")).unwrap();

    let outcome = flayer(dir.path(), &["unlink", "alpha", "alias"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\n");
    assert!(!registry(dir.path()).contains("\"alpha\""));
    assert!(registry(dir.path()).contains("\"beta\""));
}

#[test]
fn a_shelf_skill_with_a_name_that_is_not_one_folder_is_a_warning_not_a_row() {
    let repo = repository(&[
        ("skills/good/SKILL.md", &skill_file("good", "Fine")),
        ("skills/evil/SKILL.md", &skill_file("../skills", "Evil")),
    ]);
    let (_dir, target, _team) = target_and_team();
    let url = repo.path().to_string_lossy().into_owned();
    flayer(&target, &["gather", "git", &url]).unwrap();

    let shelf = flayer(&target, &["gather", "list"]).unwrap();

    assert!(!shelf.stdout.contains("../skills"), "{}", shelf.stdout);
    assert!(shelf.stdout.contains("good"));
    assert!(shelf
        .stderr
        .iter()
        .any(|warning| warning.contains("not one folder name")));
}

#[test]
fn two_spellings_of_one_loaded_workspace_are_listed_as_the_same() {
    let (_dir, target, _team) = target_and_team();
    fs::write(
        target.join(FLAYER_DIR).join(FLAYER_CONFIG),
        registry(&target).replace("loaded = []", "loaded = [\"../team\", \"./../team\"]"),
    )
    .unwrap();

    let listed = flayer(&target, &["load"]).unwrap();

    assert!(
        listed
            .stdout
            .contains("../team    team  1 skill to install"),
        "{}",
        listed.stdout
    );
    assert!(
        listed.stdout.contains("the same as ../team"),
        "{}",
        listed.stdout
    );
}

#[test]
fn nowhere_to_load_reads_as_a_sentence() {
    // Outside every workspace, with no default either.
    let dir = TempDir::new().unwrap();

    let unload = flayer(dir.path(), &["unload", "nowhere"])
        .unwrap_err()
        .error
        .to_string();
    let list = flayer(dir.path(), &["load"]).unwrap_err().error.to_string();

    assert!(
        unload.contains("but the ones named at or above"),
        "{unload}"
    );
    assert!(
        unload.ends_with("to unload from, and no default one"),
        "{unload}"
    );
    assert!(!list.contains("the ones named"), "{list}");
    assert!(
        list.ends_with("to list the loads of, and no default one"),
        "{list}"
    );
}
