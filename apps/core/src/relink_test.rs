//! Unit tests for `relink.rs`, kept beside it rather than inside it.
//!
//! The integration suite moves projects on the disk; these hold the rule a
//! copy is told by, every id held by more than one directory and never two
//! spellings of one, and the words each refusal says, to what they are now.

use super::*;

use std::collections::BTreeMap;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::ProjectIdMother;
use crate::workspace::MarkerFile;

const DRAWS: usize = 200;

fn at(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[test]
fn nothing_held_is_no_duplicate() {
    assert!(duplicates(&[]).is_empty());
}

#[test]
fn ids_each_held_once_are_no_duplicate() {
    let held: Vec<(ProjectId, PathBuf)> = (0..random::between(1, 8))
        .map(|n| (ProjectIdMother::random(), at(&format!("/p/{n}"))))
        .collect();

    assert!(duplicates(&held).is_empty(), "{held:?}");
}

#[test]
fn an_id_held_by_two_directories_is_one_duplicate_naming_both_in_order() {
    let id = ProjectIdMother::random();
    let held = [(id.clone(), at("/p/app")), (id.clone(), at("/p/copy"))];

    assert_eq!(
        duplicates(&held),
        [Duplicate {
            id,
            paths: vec![at("/p/app"), at("/p/copy")],
        }]
    );
}

/// A workspace listing one project under two entries, or a scan meeting
/// the project through a link, reads it twice: that is the one directory,
/// never a copy of it.
#[test]
fn two_spellings_of_one_directory_are_no_duplicate() {
    let id = ProjectIdMother::random();
    let held = [
        (id.clone(), at("/p/app")),
        (id.clone(), at("/p/./app")),
        (id.clone(), at("/p/x/../app")),
    ];

    assert!(duplicates(&held).is_empty());
}

#[cfg(unix)]
#[test]
fn a_directory_and_a_link_to_it_are_no_duplicate() {
    let dir = TempDir::new().unwrap();
    let app = dir.path().join("app");
    fs_make(&app);
    std::os::unix::fs::symlink(&app, dir.path().join("linked")).unwrap();
    let id = ProjectIdMother::random();

    let found = duplicates(&[(id.clone(), app.clone()), (id, dir.path().join("linked"))]);

    assert!(found.is_empty(), "{found:?}");
}

#[cfg(unix)]
fn fs_make(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
}

/// A spelling of a directory already named is left out of the duplicate,
/// so each copy is named once, by the spelling met first.
#[test]
fn a_duplicate_names_each_directory_once_by_its_first_spelling() {
    let id = ProjectIdMother::random();
    let held = [
        (id.clone(), at("/p/app")),
        (id.clone(), at("/p/copy")),
        (id.clone(), at("/p/./app")),
    ];

    assert_eq!(duplicates(&held)[0].paths, [at("/p/app"), at("/p/copy")]);
}

#[test]
fn duplicates_are_listed_in_the_order_their_ids_were_first_held() {
    let (first, second, single) = (
        ProjectIdMother::random(),
        ProjectIdMother::random(),
        ProjectIdMother::random(),
    );
    let held = [
        (first.clone(), at("/a")),
        (second.clone(), at("/b")),
        (single, at("/c")),
        (second.clone(), at("/d")),
        (first.clone(), at("/e")),
        (first.clone(), at("/f")),
    ];

    let found = duplicates(&held);

    assert_eq!(
        found,
        [
            Duplicate {
                id: first,
                paths: vec![at("/a"), at("/e"), at("/f")],
            },
            Duplicate {
                id: second,
                paths: vec![at("/b"), at("/d")],
            },
        ]
    );
}

/// Whatever is held, a duplicate is an id with every distinct directory
/// holding it, and an id held in one directory is none.
#[test]
fn any_ids_held_give_a_duplicate_for_each_id_in_more_than_one_directory() {
    for _ in 0..DRAWS {
        let ids: Vec<ProjectId> = (0..random::between(1, 4))
            .map(|_| ProjectIdMother::random())
            .collect();
        let held: Vec<(ProjectId, PathBuf)> = (0..random::between(0, 12))
            .map(|_| {
                (
                    random::pick(&ids),
                    at(&format!("/p/{}", random::between(0, 5))),
                )
            })
            .collect();
        let mut expected: BTreeMap<&ProjectId, Vec<&PathBuf>> = BTreeMap::new();
        for (id, path) in &held {
            let paths = expected.entry(id).or_default();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        expected.retain(|_, paths| paths.len() > 1);

        let found = duplicates(&held);

        assert_eq!(found.len(), expected.len(), "{held:?}");
        for duplicate in &found {
            let paths: Vec<&PathBuf> = duplicate.paths.iter().collect();
            assert_eq!(Some(&paths), expected.get(&duplicate.id), "{held:?}");
        }
    }
}

#[test]
fn a_duplicate_of_two_says_both_are_one_project_and_how_to_part_them() {
    let id = ProjectIdMother::random();
    let duplicate = Duplicate {
        id: id.clone(),
        paths: vec![at("/p/app"), at("/p/copy")],
    };

    assert_eq!(
        duplicate.to_string(),
        format!(
            "/p/app and /p/copy are both project {id}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own"
        )
    );
}

#[test]
fn a_duplicate_of_more_than_two_says_all_are_one_project() {
    let id = ProjectIdMother::random();
    let duplicate = Duplicate {
        id: id.clone(),
        paths: vec![at("/a"), at("/b"), at("/c"), at("/d")],
    };

    assert_eq!(
        duplicate.to_string(),
        format!(
            "/a, /b, /c and /d are all project {id}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own"
        )
    );
}

#[test]
fn a_duplicate_is_one_condition_with_one_code() {
    let duplicate = Duplicate {
        id: ProjectIdMother::random(),
        paths: vec![at("/a"), at("/b")],
    };

    assert_eq!(duplicate.code(), "duplicate_id");
    assert_eq!(Duplicate::CODES, ["duplicate_id"]);
}

/// One of each variant with a code of its own, beside the code it gives.
fn every_own_error() -> Vec<(RelinkError, &'static str)> {
    vec![
        (
            RelinkError::NoId {
                path: at("/p/new"),
                marker: MarkerFile::MindToml,
            },
            "relink_no_id",
        ),
        (
            RelinkError::Linked {
                path: at("/p/new"),
                entry: at("apps/new"),
            },
            "relink_already_linked",
        ),
        (
            RelinkError::Present {
                path: at("/p/old"),
                name: String::from("app"),
            },
            "relink_entry_present",
        ),
        (
            RelinkError::Unread {
                path: at("/p/old"),
                source: Box::new(WorkspaceError::NotAProject { path: at("/p/old") }),
            },
            "relink_entry_unreadable",
        ),
        (
            RelinkError::Mismatch {
                old: at("/p/old"),
                was: ProjectIdMother::random(),
                new: at("/p/new"),
                id: ProjectIdMother::random(),
            },
            "relink_mismatch",
        ),
        (
            RelinkError::Elsewhere {
                path: at("/p/new"),
                id: ProjectIdMother::random(),
                at: at("other"),
            },
            "relink_elsewhere",
        ),
        (
            RelinkError::Unknown { path: at("/p/old") },
            "relink_unknown",
        ),
    ]
}

#[test]
fn each_relink_error_of_its_own_gives_its_own_code() {
    for (error, code) in every_own_error() {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

#[test]
fn the_codes_a_relink_error_gives_of_its_own_are_listed_in_order() {
    let given: Vec<&str> = every_own_error()
        .into_iter()
        .map(|(_, code)| code)
        .collect();

    assert_eq!(RelinkError::CODES, given.as_slice());
}

/// A copy refused as a move is the duplicate's condition, and the panel
/// tells it by the duplicate's code wherever it was met.
#[test]
fn a_relink_refused_for_a_copy_says_what_the_duplicate_says_with_its_code() {
    let duplicate = Duplicate {
        id: ProjectIdMother::random(),
        paths: vec![at("/p/old"), at("/p/new")],
    };
    let error = RelinkError::Copied(duplicate.clone());

    assert_eq!(error.code(), "duplicate_id");
    assert_eq!(error.to_string(), duplicate.to_string());
}

#[test]
fn a_relink_error_carrying_another_of_core_s_gives_its_code() {
    let workspace = RelinkError::from(WorkspaceError::NotRegistered {
        path: at("/p/old"),
        workspace: at("/w"),
    });
    let ledger = RelinkError::from(LedgerError::Unmade {
        path: at("/w/.mindflayer/mindflayer.db"),
    });

    assert_eq!(workspace.code(), "not_registered");
    assert_eq!(ledger.code(), "ledger_unmade");
}

#[test]
fn a_directory_linked_already_is_named_by_the_entry_as_the_marker_spells_it() {
    let error = RelinkError::Linked {
        path: at("/w/apps/new"),
        entry: at("apps/new"),
    };

    assert_eq!(
        error.to_string(),
        "/w/apps/new: the workspace links it already, as apps/new; `mindweaver unlink` that entry first"
    );
}

#[test]
fn a_directory_holding_another_project_names_both_ids() {
    let (was, id) = (ProjectIdMother::random(), ProjectIdMother::random());
    let error = RelinkError::Mismatch {
        old: at("/p/old"),
        was: was.clone(),
        new: at("/p/new"),
        id: id.clone(),
    };

    assert_eq!(
        error.to_string(),
        format!(
            "/p/new: holds project {id}, and /p/old was project {was}, so it is not where that \
             one moved; nothing was changed"
        )
    );
}

#[test]
fn a_project_last_seen_elsewhere_says_the_relink_that_follows_it() {
    let id = ProjectIdMother::random();
    let error = RelinkError::Elsewhere {
        path: at("/p/new"),
        id: id.clone(),
        at: at("apps/old"),
    };

    assert_eq!(
        error.to_string(),
        format!(
            "/p/new: holds project {id}, which the workspace last saw at apps/old; \
             `mindweaver relink apps/old /p/new` follows it from there"
        )
    );
}

#[test]
fn a_project_with_no_id_names_the_marker_that_marks_it() {
    let error = RelinkError::NoId {
        path: at("/p/new"),
        marker: MarkerFile::MindToml,
    };

    assert!(
        error.to_string().starts_with(&format!(
            "/p/new: a {} marks it, which names it by no id",
            MarkerFile::MindToml
        )),
        "{error}"
    );
}

/// An older marker has no id to change: refused before anything is read
/// or written, whether or not a workspace links the project.
#[test]
fn a_new_id_is_refused_for_a_project_a_mind_toml_marks_and_nothing_is_written() {
    let dir = TempDir::new().unwrap();
    let project = crate::mother::ProjectMother::older(&dir.path().join("app"));
    let marker = project.marker_path();
    let before = std::fs::read(&marker).unwrap();

    let error = new_id(None, &project).unwrap_err();

    assert!(matches!(error, RelinkError::Workspace(_)), "{error:?}");
    assert_eq!(std::fs::read(&marker).unwrap(), before);
}

#[test]
fn a_new_id_differs_from_the_one_it_replaces_and_is_one_the_project_then_holds() {
    for _ in 0..10 {
        let dir = TempDir::new().unwrap();
        let project = crate::mother::ProjectMother::random(dir.path());

        let renewed = new_id(None, &project).unwrap();

        assert_ne!(renewed.id, renewed.was);
        assert_eq!(renewed.name, project.name());
        let reopened = MindProject::open(project.root()).unwrap();
        assert_eq!(
            reopened.settings().map(|settings| &settings.id),
            Some(&renewed.id)
        );
        assert_eq!(
            renewed.excluded.is_some(),
            project.settings().unwrap().share == Share::Local,
            "a shared project's copies are the team's"
        );
        assert!(renewed.scrub.is_empty(), "no workspace, no ledger to scrub");
    }
}
