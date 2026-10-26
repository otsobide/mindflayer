//! Unit tests for `hash.rs`, kept beside it rather than inside it.

use super::*;

use tempfile::TempDir;

/// A tree holding these entries, written in the order given: a path ending
/// in `/` is a folder, anything else a file with those contents.
fn tree_of(entries: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    write_into(dir.path(), entries);
    dir
}

fn write_into(root: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(root).unwrap();
    for (path, contents) in entries {
        match path.strip_suffix('/') {
            Some(folder) => fs::create_dir_all(root.join(folder)).unwrap(),
            None => {
                let file = root.join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, contents).unwrap();
            }
        }
    }
}

fn hash_of(entries: &[(&str, &str)]) -> String {
    tree(tree_of(entries).path()).unwrap()
}

const DEPLOY: &[(&str, &str)] = &[
    (
        "SKILL.md",
        "---\nname: deploy\ndescription: Ship the service to staging\n---\n\nRun the deploy script.\n",
    ),
    ("assets/", ""),
    ("scripts/", ""),
    ("scripts/deploy.sh", "#!/bin/sh\nmake deploy\n"),
    ("scripts.md", "What the scripts do.\n"),
];

#[test]
fn a_fixed_tree_hashes_to_the_same_digest_on_every_platform() {
    // Pinned, so the macOS and Windows runs of CI check the recipe against
    // the value Linux gives, and against one worked out apart from this code
    // from the recipe in `tree`'s documentation. `scripts.md` sorts between
    // the folder `scripts` and the file inside it, as bytes compare.
    assert_eq!(
        hash_of(DEPLOY),
        "14a8f44865061157f7da083177226354431181da771625e5ca607b29e632a530"
    );
}

#[test]
fn an_empty_folder_hashes_as_nothing_at_all() {
    // The SHA-256 of no bytes: the root's own name is not part of it.
    assert_eq!(
        hash_of(&[]),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn neither_the_order_things_were_written_in_nor_the_folder_matters() {
    let mut backwards = DEPLOY.to_vec();
    backwards.reverse();
    let dir = TempDir::new().unwrap();
    let filed = dir.path().join(".claude").join("skills").join("deploy");
    write_into(&filed, DEPLOY);

    assert_eq!(hash_of(&backwards), hash_of(DEPLOY));
    assert_eq!(tree(&filed).unwrap(), hash_of(DEPLOY));
}

#[test]
fn every_change_a_copy_can_show_changes_its_hash() {
    let base = &[("SKILL.md", "body\n"), ("scripts/run.sh", "run\n")];
    let changes: &[&[(&str, &str)]] = &[
        // A byte.
        &[("SKILL.md", "body!\n"), ("scripts/run.sh", "run\n")],
        // A name.
        &[("SKILL.md", "body\n"), ("scripts/go.sh", "run\n")],
        // A folder.
        &[("SKILL.md", "body\n"), ("bin/run.sh", "run\n")],
        // A file taken out of its folder.
        &[
            ("SKILL.md", "body\n"),
            ("scripts/", ""),
            ("run.sh", "run\n"),
        ],
        // An empty folder more.
        &[
            ("SKILL.md", "body\n"),
            ("scripts/run.sh", "run\n"),
            ("assets/", ""),
        ],
        // An empty file more.
        &[
            ("SKILL.md", "body\n"),
            ("scripts/run.sh", "run\n"),
            ("NOTES", ""),
        ],
    ];

    let mut seen = vec![hash_of(base)];
    for change in changes {
        let hash = hash_of(change);
        assert!(!seen.contains(&hash), "{change:?} hashes like another tree");
        seen.push(hash);
    }
}

#[test]
fn where_one_file_ends_and_the_next_begins_counts() {
    // The same bytes in the same order, split differently between two files.
    assert_ne!(
        hash_of(&[("a", "xy"), ("b", "")]),
        hash_of(&[("a", "x"), ("b", "y")])
    );
    // And a name that runs on into what follows it.
    assert_ne!(hash_of(&[("ab", "c")]), hash_of(&[("a", "bc")]));
}

#[test]
fn two_trees_hash_alike_exactly_when_copy_calls_them_the_same() {
    let trees = [
        tree_of(DEPLOY),
        tree_of(&[("SKILL.md", "body\n")]),
        tree_of(&[("SKILL.md", "body\n"), ("assets/", "")]),
        tree_of(&[("SKILL.md", "Body\n")]),
        tree_of(&[("SKILL.md", "body\n")]),
    ];
    for one in &trees {
        for other in &trees {
            assert_eq!(
                tree(one.path()).unwrap() == tree(other.path()).unwrap(),
                crate::copy::same(one.path(), other.path()).unwrap(),
                "{} and {}",
                one.path().display(),
                other.path().display()
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_refused_rather_than_followed() {
    let dir = tree_of(&[("SKILL.md", "body\n")]);
    std::os::unix::fs::symlink("/dev/zero", dir.path().join("endless")).unwrap();

    let error = tree(dir.path()).unwrap_err();

    assert!(
        error.to_string().contains("neither a file nor a folder"),
        "{error}"
    );
}
