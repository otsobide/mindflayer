//! Unit tests for `resolve.rs`: each step of the order, each reason, and what
//! a step refuses rather than passes over.

use std::fs;

use tempfile::TempDir;

use super::*;

/// Four workspaces in one temporary directory, none of them above another:
/// `named` and `activated` to be chosen, `outer` with `start` somewhere
/// below it, and `home` where the default lives, made.
struct Fixture {
    _dir: TempDir,
    named: PathBuf,
    activated: PathBuf,
    outer: PathBuf,
    start: PathBuf,
    home: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let path = |name: &str| dir.path().join(name);
    let (named, activated, outer, home) = (
        path("named"),
        path("activated"),
        path("outer"),
        path("home"),
    );
    for root in [&named, &activated, &outer, &home] {
        FlayerWorkspace::init(root).unwrap();
    }
    let start = outer.join("deep/er");
    fs::create_dir_all(&start).unwrap();
    Fixture {
        _dir: dir,
        named,
        activated,
        outer,
        start,
        home,
    }
}

/// Every input given, as a front end reads them; a test takes away what it
/// wants absent.
fn inputs(fixture: &Fixture) -> Inputs<'_> {
    Inputs {
        flag: Some(&fixture.named),
        activated: Some(&fixture.activated),
        start: &fixture.start,
        home: Some(&fixture.home),
        intent: Intent::Read,
    }
}

/// What `inputs` resolve to: the workspace's root and the reason.
fn resolved(inputs: &Inputs<'_>) -> (PathBuf, Reason) {
    let resolved = resolve(inputs).unwrap().expect("a workspace");
    (resolved.workspace.root().to_path_buf(), resolved.reason)
}

#[test]
fn a_named_workspace_wins_over_every_other() {
    let fixture = fixture();

    for intent in [Intent::Read, Intent::Write] {
        let inputs = Inputs {
            intent,
            ..inputs(&fixture)
        };
        assert_eq!(resolved(&inputs), (fixture.named.clone(), Reason::Explicit));
    }
}

#[test]
fn an_activated_workspace_wins_over_one_found_above() {
    let fixture = fixture();
    let inputs = Inputs {
        flag: None,
        ..inputs(&fixture)
    };

    assert_eq!(
        resolved(&inputs),
        (fixture.activated.clone(), Reason::Activated)
    );
}

#[test]
fn a_workspace_above_wins_over_the_default_one() {
    let fixture = fixture();
    let inputs = Inputs {
        flag: None,
        activated: None,
        ..inputs(&fixture)
    };

    assert_eq!(resolved(&inputs), (fixture.outer.clone(), Reason::Above));
}

#[test]
fn with_nothing_chosen_and_nothing_above_the_default_one_is_used() {
    let fixture = fixture();
    let elsewhere = TempDir::new().unwrap();
    let inputs = Inputs {
        flag: None,
        activated: None,
        start: elsewhere.path(),
        ..inputs(&fixture)
    };

    assert_eq!(resolved(&inputs), (fixture.home.clone(), Reason::Home));
}

#[test]
fn with_no_default_either_there_is_no_workspace() {
    let elsewhere = TempDir::new().unwrap();

    for intent in [Intent::Read, Intent::Write] {
        let inputs = Inputs {
            flag: None,
            activated: None,
            start: elsewhere.path(),
            home: None,
            intent,
        };
        assert!(resolve(&inputs).unwrap().is_none());
    }
}

#[test]
fn an_empty_named_workspace_switches_the_activated_one_off_too() {
    // What the test helpers pass: anything a developer's shell activated
    // must not reach a command they run.
    let fixture = fixture();
    let inputs = Inputs {
        flag: Some(Path::new("")),
        ..inputs(&fixture)
    };

    assert_eq!(resolved(&inputs), (fixture.outer.clone(), Reason::Above));
}

#[test]
fn an_empty_activated_workspace_counts_as_none() {
    let fixture = fixture();
    let inputs = Inputs {
        flag: None,
        activated: Some(Path::new("")),
        ..inputs(&fixture)
    };

    assert_eq!(resolved(&inputs), (fixture.outer.clone(), Reason::Above));
}

#[test]
fn a_workspace_is_named_by_its_root_or_by_its_mindflayer_folder() {
    let fixture = fixture();
    let folder = fixture.named.join(FLAYER_DIR);
    let inputs = Inputs {
        flag: Some(&folder),
        ..inputs(&fixture)
    };

    assert_eq!(resolved(&inputs), (fixture.named.clone(), Reason::Explicit));
}

#[test]
fn a_workspace_named_as_an_argument_takes_the_spellings_w_takes() {
    let fixture = fixture();
    let plain = fixture.outer.join("plain");
    fs::create_dir(&plain).unwrap();

    for spelled in [fixture.named.clone(), fixture.named.join(FLAYER_DIR)] {
        assert_eq!(named(&spelled).unwrap().root(), fixture.named);
    }
    assert!(matches!(
        named(&plain),
        Err(WorkspaceError::NotAWorkspace { .. })
    ));
    assert!(!plain.join(FLAYER_DIR).exists());
}

#[test]
fn a_named_directory_that_is_no_workspace_is_refused_and_never_made() {
    let fixture = fixture();
    let plain = fixture.outer.join("plain");
    fs::create_dir(&plain).unwrap();

    // A write least of all: it may make the default, never what was named.
    for intent in [Intent::Read, Intent::Write] {
        let inputs = Inputs {
            flag: Some(&plain),
            intent,
            ..inputs(&fixture)
        };
        let error = resolve(&inputs).unwrap_err();

        assert!(
            matches!(
                error,
                ResolveError::Chosen {
                    reason: Reason::Explicit,
                    source: WorkspaceError::NotAWorkspace { .. },
                }
            ),
            "{error}"
        );
        assert!(!plain.join(FLAYER_DIR).exists());
    }
}

#[test]
fn an_activated_directory_that_is_no_workspace_is_refused_rather_than_passed_over() {
    // The workspace above would answer, and acting on it would act on one
    // nobody chose.
    let fixture = fixture();
    let gone = fixture.outer.join("gone");
    let inputs = Inputs {
        flag: None,
        activated: Some(&gone),
        intent: Intent::Write,
        ..inputs(&fixture)
    };

    let error = resolve(&inputs).unwrap_err();

    assert!(
        matches!(
            error,
            ResolveError::Chosen {
                reason: Reason::Activated,
                ..
            }
        ),
        "{error}"
    );
    assert!(!gone.exists());
}

#[test]
fn a_read_never_makes_the_default_workspace() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let elsewhere = TempDir::new().unwrap();
    let inputs = Inputs {
        flag: None,
        activated: None,
        start: elsewhere.path(),
        home: Some(&home),
        intent: Intent::Read,
    };

    let resolved = resolve(&inputs).unwrap().expect("the default, unmade");

    assert_eq!(resolved.reason, Reason::Home);
    assert!(!resolved.workspace.is_made());
    assert_eq!(resolved.workspace.root(), home);
    assert!(!home.exists());
}

#[test]
fn a_write_makes_the_default_workspace_the_first_time() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let elsewhere = TempDir::new().unwrap();
    let inputs = Inputs {
        flag: None,
        activated: None,
        start: elsewhere.path(),
        home: Some(&home),
        intent: Intent::Write,
    };

    let resolved = resolve(&inputs).unwrap().expect("the default, made");

    assert_eq!(resolved.reason, Reason::Home);
    assert!(resolved.workspace.is_made());
    assert!(home.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file());
}

#[test]
fn passing_over_a_workspace_carries_the_walk_on_above_it() {
    let fixture = fixture();
    let inner = fixture.outer.join("deep");
    FlayerWorkspace::init(&inner).unwrap();
    let inputs = Inputs {
        flag: None,
        activated: None,
        ..inputs(&fixture)
    };

    let resolved = resolve_except(&inputs, &[inner]).unwrap().unwrap();

    assert_eq!(resolved.workspace.root(), fixture.outer);
    assert_eq!(resolved.reason, Reason::Above);
}

#[test]
fn passing_over_the_default_refuses_a_write_and_finds_nothing_for_a_read() {
    let fixture = fixture();
    let inputs = Inputs {
        flag: None,
        activated: None,
        start: &fixture.home,
        ..inputs(&fixture)
    };
    let except = [fixture.home.clone()];

    let write = resolve_except(
        &Inputs {
            intent: Intent::Write,
            ..inputs
        },
        &except,
    )
    .unwrap_err();
    let read = resolve_except(&inputs, &except).unwrap();

    assert!(
        matches!(
            write,
            ResolveError::Workspace(WorkspaceError::LoadsItself { .. })
        ),
        "{write}"
    );
    assert!(read.is_none());
}

#[test]
fn a_chosen_workspace_is_never_passed_over() {
    let fixture = fixture();

    let resolved = resolve_except(&inputs(&fixture), std::slice::from_ref(&fixture.named))
        .unwrap()
        .unwrap();

    assert_eq!(resolved.workspace.root(), fixture.named);
    assert_eq!(resolved.reason, Reason::Explicit);
}

// ---------------------------------------------------------------------------
// A project's workspace
// ---------------------------------------------------------------------------

/// `fixture`, with `collapse`, a project beside the four workspaces rather
/// than under any of them, linked by `named` and `activated`.
fn with_a_project() -> (Fixture, MindProject) {
    let fixture = fixture();
    let (project, _) = MindProject::init(fixture.named.parent().unwrap().join("collapse")).unwrap();
    for root in [&fixture.named, &fixture.activated] {
        FlayerWorkspace::open(root).unwrap().link(&project).unwrap();
    }
    (fixture, project)
}

#[test]
fn a_project_is_managed_by_a_workspace_chosen_for_it_that_links_it() {
    // Neither is above the project: chosen, a workspace manages wherever
    // the projects it links are.
    let (fixture, project) = with_a_project();

    for (flag, expected) in [
        (Some(fixture.named.as_path()), &fixture.named),
        (None, &fixture.activated),
    ] {
        let inputs = Inputs {
            flag,
            intent: Intent::Write,
            ..inputs(&fixture)
        };
        let workspace = managing(&inputs, &project).unwrap().unwrap();
        assert_eq!(workspace.root(), expected);
    }
}

#[test]
fn a_project_is_not_managed_by_a_workspace_that_does_not_link_it() {
    let (fixture, project) = with_a_project();
    let inputs = Inputs {
        flag: Some(&fixture.outer),
        ..inputs(&fixture)
    };

    let error = managing(&inputs, &project).unwrap_err();

    assert!(
        matches!(error, ResolveError::NotManaged { ref workspace, .. } if *workspace == fixture.outer),
        "{error}"
    );
}

#[test]
fn a_projects_workspace_is_resolved_from_the_project_not_from_where_a_command_starts() {
    let (fixture, project) = with_a_project();
    let inputs = Inputs {
        flag: None,
        activated: None,
        // Below `outer`, which does not link the project; the project has
        // no workspace above it, so the default is the one asked.
        home: Some(&fixture.named),
        ..inputs(&fixture)
    };

    let workspace = managing(&inputs, &project).unwrap().unwrap();

    assert_eq!(workspace.root(), fixture.named);
}

#[test]
fn a_change_to_a_project_consults_the_workspace_chosen_for_it() {
    let (fixture, project) = with_a_project();
    let inputs = Inputs {
        flag: None,
        ..inputs(&fixture)
    };

    let consulted = consulted(&inputs, &project).unwrap().unwrap();

    assert_eq!(consulted.workspace.root(), fixture.activated);
    assert_eq!(consulted.reason, Reason::Activated);
}

#[test]
fn a_change_to_a_project_consults_the_default_only_once_it_is_made() {
    let (fixture, project) = with_a_project();
    let dir = TempDir::new().unwrap();
    let unmade = dir.path().join("you");
    let inputs = |home| Inputs {
        flag: None,
        activated: None,
        home: Some(home),
        intent: Intent::Write,
        ..inputs(&fixture)
    };

    assert!(consulted(&inputs(&unmade), &project).unwrap().is_none());
    assert!(!unmade.exists());
    let made = consulted(&inputs(&fixture.home), &project)
        .unwrap()
        .unwrap();
    assert_eq!(made.reason, Reason::Home);
}

// ---------------------------------------------------------------------------
// Which of a workspace's projects a command names
// ---------------------------------------------------------------------------

/// `workspace`, managing `alpha` and `beta` under it, and `gone`, an entry
/// whose directory has been removed.
fn picking() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta", "gone"] {
        let (project, _) = MindProject::init(dir.path().join(name)).unwrap();
        workspace.link(&project).unwrap();
    }
    fs::remove_dir_all(dir.path().join("gone")).unwrap();
    (dir, workspace)
}

#[test]
fn a_project_is_picked_by_its_name_its_entry_or_its_path() {
    let (dir, workspace) = picking();

    for wanted in [
        "alpha",
        "./alpha",
        &dir.path().join("alpha").to_string_lossy(),
    ] {
        let project = workspace.pick(wanted).unwrap();
        assert_eq!(project.root(), dir.path().join("alpha"), "{wanted}");
    }
}

#[test]
fn a_project_nobody_manages_is_named_with_the_ones_that_are() {
    let (_dir, workspace) = picking();

    let error = workspace.pick("gamma").unwrap_err();

    assert!(
        matches!(error, ResolveError::UnknownProject { ref known, .. } if *known == ["alpha", "beta"]),
        "{error}"
    );
}

#[test]
fn a_project_named_exactly_that_will_not_open_says_why() {
    let (_dir, workspace) = picking();

    let error = workspace.pick("gone").unwrap_err();

    assert!(
        matches!(
            error,
            ResolveError::Workspace(WorkspaceError::NotAProject { .. })
        ),
        "{error}"
    );
}

#[test]
fn a_name_two_projects_answer_to_is_refused_with_each_entry() {
    let (dir, mut workspace) = picking();
    // `beta`'s directory, but declaring the name `alpha`.
    let (twin, _) = MindProject::init(dir.path().join("twin")).unwrap();
    let marker = twin.mind_dir().join(crate::workspace::MIND_CONFIG);
    let text = fs::read_to_string(&marker)
        .unwrap()
        .replace("name = \"twin\"", "name = \"alpha\"");
    fs::write(&marker, text).unwrap();
    workspace.link(&twin).unwrap();

    let error = workspace.pick("alpha").unwrap_err();

    assert!(
        matches!(error, ResolveError::AmbiguousProject { ref matches, .. } if *matches == ["alpha", "twin"]),
        "{error}"
    );
}

#[test]
fn a_project_is_named_by_its_entry_which_picks_it_back() {
    let (dir, workspace) = picking();
    let beta = MindProject::open(dir.path().join("beta")).unwrap();

    let named = workspace.naming(&beta);

    assert_eq!(named, "beta");
    assert_eq!(workspace.pick(&named).unwrap().root(), beta.root());
}

#[test]
fn a_project_whose_entry_another_declares_as_its_name_is_named_from_the_root() {
    let (dir, mut workspace) = picking();
    // In `twin`, but declaring the name `alpha`, which is `alpha`'s entry.
    let (twin, _) = MindProject::init(dir.path().join("twin")).unwrap();
    let marker = twin.mind_dir().join(crate::workspace::MIND_CONFIG);
    let text = fs::read_to_string(&marker)
        .unwrap()
        .replace("name = \"twin\"", "name = \"alpha\"");
    fs::write(&marker, text).unwrap();
    workspace.link(&twin).unwrap();
    let alpha = MindProject::open(dir.path().join("alpha")).unwrap();
    let twin = MindProject::open(dir.path().join("twin")).unwrap();

    let (alpha_named, twin_named) = (workspace.naming(&alpha), workspace.naming(&twin));

    assert_eq!(alpha_named, "./alpha");
    assert_eq!(workspace.pick(&alpha_named).unwrap().root(), alpha.root());
    // Nobody declares `twin`, so its entry is enough.
    assert_eq!(twin_named, "twin");
    assert_eq!(workspace.pick(&twin_named).unwrap().root(), twin.root());
}

#[test]
fn with_no_project_named_the_holder_is_the_workspaces_own() {
    let (dir, workspace) = picking();

    assert_eq!(workspace.holder(None).unwrap(), workspace.own());
    assert_eq!(
        workspace.holder(Some("beta")).unwrap().root(),
        dir.path().join("beta")
    );
}
