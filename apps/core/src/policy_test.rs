//! Unit tests for `policy.rs`: what a request from outside may name, and the
//! code each thing it may not name is refused with.

use std::fs;
use std::path::MAIN_SEPARATOR_STR as SEPARATOR;

use tempfile::TempDir;

use super::*;
use crate::known;
use crate::workspace::MindProject;

/// A workspace linking `alpha` under it and `beside` next to it, and loading
/// `library`, a workspace next to it too.
fn fixture() -> (TempDir, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("work");
    let (mut workspace, _) = FlayerWorkspace::init(&root).unwrap();
    for project in [root.join("alpha"), dir.path().join("beside")] {
        let (project, _) = MindProject::init(project).unwrap();
        workspace.link(&project).unwrap();
    }
    let (library, _) = FlayerWorkspace::init(dir.path().join("library")).unwrap();
    workspace.load(&library).unwrap();
    let workspace = FlayerWorkspace::open(&root).unwrap();
    (dir, workspace)
}

/// `parts` joined under `base` with the platform's separator, as given.
fn spelled_under(base: &Path, parts: &[&str]) -> String {
    let mut text = base.to_str().unwrap().to_owned();
    for part in parts {
        text.push_str(SEPARATOR);
        text.push_str(part);
    }
    text
}

fn code<T: std::fmt::Debug>(result: Result<T, PolicyError>) -> &'static str {
    result.expect_err("it was taken").code()
}

#[test]
fn an_absolute_normalized_path_outside_any_workspace_folder_is_taken_as_given() {
    let (dir, _workspace) = fixture();
    let given = spelled_under(dir.path(), &["new", "project"]);

    assert_eq!(path(&given).unwrap(), PathBuf::from(&given));
}

#[test]
fn a_relative_path_is_refused_whatever_it_starts_with() {
    for given in ["beta", "./beta", "../beta", "-rf", "--yes", "", "~/beta"] {
        assert_eq!(code(path(given)), "path_relative", "{given:?}");
    }
}

#[test]
fn a_url_is_refused_as_one() {
    for given in [
        "file:///etc",
        "https://example.com/repo",
        "ssh://git@host/x",
    ] {
        assert_eq!(code(path(given)), "path_url", "{given:?}");
    }
}

#[test]
fn a_path_not_spelled_as_its_folder_is_refused() {
    let (dir, _workspace) = fixture();
    let base = dir.path();
    let doubled = format!("{}{SEPARATOR}{SEPARATOR}alpha", base.to_str().unwrap());

    for given in [
        spelled_under(base, &["work", "..", "beside"]),
        spelled_under(base, &["work", ".", "alpha"]),
        spelled_under(base, &["..", "..", "etc"]),
        format!("{}{SEPARATOR}", spelled_under(base, &["beside"])),
        doubled,
    ] {
        assert_eq!(code(path(&given)), "path_unnormalized", "{given:?}");
    }
}

#[test]
fn a_path_inside_a_mindflayer_is_refused() {
    let (dir, workspace) = fixture();
    let flayer = workspace.flayer_dir();

    for given in [
        flayer.to_str().unwrap().to_owned(),
        spelled_under(&flayer, &["shelf", "github.com"]),
        // One that does not exist yet is judged on its spelling.
        spelled_under(dir.path(), &["elsewhere", FLAYER_DIR, "skills"]),
    ] {
        assert_eq!(code(path(&given)), "path_in_mindflayer", "{given:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_path_that_leads_into_a_mindflayer_through_a_link_is_refused() {
    let (dir, workspace) = fixture();
    let link = dir.path().join("innocent");
    std::os::unix::fs::symlink(workspace.flayer_dir(), &link).unwrap();

    assert_eq!(code(path(link.to_str().unwrap())), "path_in_mindflayer");
}

/// Followed as far as it exists: `mind init` would make the new folder
/// through the link, inside the workspace's own.
#[cfg(unix)]
#[test]
fn a_new_folder_under_a_link_into_a_mindflayer_is_refused() {
    let (dir, workspace) = fixture();
    let link = dir.path().join("innocent");
    std::os::unix::fs::symlink(workspace.flayer_dir(), &link).unwrap();

    for parts in [&["new"][..], &["new", "deeper"]] {
        let given = spelled_under(&link, parts);
        assert_eq!(code(path(&given)), "path_in_mindflayer", "{given}");
    }
}

#[test]
fn a_linked_project_is_named_by_its_entry_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();

    let alpha = linked(&workspace, "alpha").unwrap();
    assert_eq!(alpha.root, workspace.root().join("alpha"));
    let beside = linked(&workspace, "../beside").unwrap();
    assert_eq!(beside.root, dir.path().join("beside"));
}

#[test]
fn anything_but_an_entry_the_registry_holds_names_no_linked_project() {
    let (dir, workspace) = fixture();
    let absolute = workspace.root().join("alpha");

    for given in [
        "beta",
        "./alpha",
        "alpha/",
        "-C",
        "--home=/etc",
        absolute.to_str().unwrap(),
        // Its name, which another project could declare too.
        "beside",
        dir.path().join("beside").to_str().unwrap(),
    ] {
        assert_eq!(
            code(linked(&workspace, given)),
            "not_registered",
            "{given:?}"
        );
    }
}

#[test]
fn a_loaded_workspace_is_named_by_its_entry_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();

    assert_eq!(
        loaded(&workspace, "../library").unwrap().root,
        dir.path().join("library")
    );
    for given in ["library", "../library/", "-x", "../beside"] {
        assert_eq!(code(loaded(&workspace, given)), "not_loaded", "{given:?}");
    }
}

#[test]
fn a_path_to_a_linked_project_or_a_loaded_workspace_is_one_an_entry_points_at() {
    let (dir, workspace) = fixture();
    let beside = dir.path().join("beside");
    let library = dir.path().join("library");

    assert_eq!(
        linked_path(&workspace, beside.to_str().unwrap()).unwrap(),
        beside
    );
    assert_eq!(
        loaded_path(&workspace, library.to_str().unwrap()).unwrap(),
        library
    );
    assert_eq!(
        code(linked_path(&workspace, library.to_str().unwrap())),
        "not_registered"
    );
    assert_eq!(
        code(loaded_path(&workspace, beside.to_str().unwrap())),
        "not_loaded"
    );
    // Held to the path rules first.
    assert_eq!(code(linked_path(&workspace, "../beside")), "path_relative");
}

#[test]
fn a_resolved_path_is_spelled_as_it_is() {
    let (dir, _workspace) = fixture();

    assert_eq!(spelled(dir.path()).unwrap(), dir.path().to_str().unwrap());
}

#[cfg(unix)]
#[test]
fn a_resolved_path_that_is_not_utf8_cannot_be_spelled() {
    use std::os::unix::ffi::OsStrExt as _;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"/work/\xff"));

    assert_eq!(code(spelled(path)), "path_not_utf8");
}

#[test]
fn nothing_named_here_touches_the_disk_but_to_read() {
    let (dir, workspace) = fixture();
    let before = fs::read_to_string(workspace.flayer_dir().join(crate::FLAYER_CONFIG)).unwrap();

    let _ = path(&spelled_under(dir.path(), &["new"]));
    let _ = linked(&workspace, "alpha");
    let _ = loaded(&workspace, "../library");

    assert!(!dir.path().join("new").exists());
    assert_eq!(
        fs::read_to_string(workspace.flayer_dir().join(crate::FLAYER_CONFIG)).unwrap(),
        before
    );
}

/// What a front end launched on `launched` may switch to, the default in
/// `home` knowing `remembered`, while it is on `in_use`.
fn listed(home: &Path, launched: &FlayerWorkspace, in_use: &FlayerWorkspace) -> Vec<Switchable> {
    let launch = known::Launch {
        root: launched.root(),
        reason: Reason::Above,
    };
    known::switchable(Some(home), &launch, in_use)
}

/// A default workspace in `home` remembering `writing` beside it, and
/// `elsewhere`, a workspace nothing remembers, the one launched on.
fn machine() -> (TempDir, PathBuf, FlayerWorkspace, FlayerWorkspace) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    FlayerWorkspace::init(&home).unwrap();
    let (writing, _) = FlayerWorkspace::init(dir.path().join("writing")).unwrap();
    known::remember(Some(&home), &writing).unwrap();
    let (elsewhere, _) = FlayerWorkspace::init(dir.path().join("elsewhere")).unwrap();
    (dir, home, writing, elsewhere)
}

#[test]
fn a_listed_workspace_is_switched_to_by_its_path_as_listed_with_its_reason() {
    let (_dir, home, writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);

    let (to, reason) = switched(&rows, writing.root().to_str().unwrap()).unwrap();
    assert_eq!(to.root(), writing.root());
    assert_eq!(reason, Reason::Explicit);

    // Back to the launch, by the rule that chose it then.
    let rows = listed(&home, &elsewhere, &writing);
    let (to, reason) = switched(&rows, elsewhere.root().to_str().unwrap()).unwrap();
    assert_eq!(to.root(), elsewhere.root());
    assert_eq!(reason, Reason::Above);
}

#[test]
fn a_path_no_list_holds_is_refused_whatever_it_leads_to() {
    let (dir, home, writing, elsewhere) = machine();
    let (unknown, _) = FlayerWorkspace::init(dir.path().join("unknown")).unwrap();
    let rows = listed(&home, &elsewhere, &elsewhere);
    let writing = writing.root().to_str().unwrap();
    let other = [
        // A workspace, but none this machine knows.
        unknown.root().to_str().unwrap().to_owned(),
        // Listed ones, spelled otherwise: through `.mindflayer`, with a
        // trailing separator, or with a `.` part.
        format!("{writing}{SEPARATOR}.mindflayer"),
        format!("{writing}{SEPARATOR}"),
        spelled_under(dir.path(), &[".", "writing"]),
        String::from("writing"),
        String::from("-W"),
        String::from("--workspace=/"),
        String::new(),
    ];
    for given in other {
        assert_eq!(
            code(switched(&rows, &given)),
            "switch_unlisted",
            "{given:?}"
        );
    }
}

#[test]
fn a_listed_workspace_that_does_not_open_is_refused_and_never_made() {
    let (_dir, home, writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);
    fs::remove_dir_all(writing.root().join(FLAYER_DIR)).unwrap();

    let refused = switched(&rows, writing.root().to_str().unwrap());

    assert_eq!(code(refused), "not_a_workspace");
    assert!(!writing.root().join(FLAYER_DIR).exists(), "it was made");
}

#[test]
fn the_one_in_use_is_taken_as_a_switch_that_changes_nothing() {
    let (_dir, home, _writing, elsewhere) = machine();
    let rows = listed(&home, &elsewhere, &elsewhere);

    let (to, reason) = switched(&rows, elsewhere.root().to_str().unwrap()).unwrap();

    assert_eq!(to.root(), elsewhere.root());
    assert_eq!(reason, Reason::Above);
}
