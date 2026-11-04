//! A copy, its hash and the comparison that tells two apart, held to agree
//! on trees drawn afresh.
//!
//! D-04 rests on three functions saying one thing: an install writes a tree
//! with [`copy::tree`], records [`hash::tree`] of it, and later calls the
//! copy current while its hash is the one recorded, as [`copy::same`] would
//! call the two trees the same. A change to any of the three that made them
//! disagree would call an untouched copy edited, or an edited one current,
//! so they are held here to agree on every tree, not only the ones named.

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::mother::random;
use mindweaver_core::{copy, hash};
use tempfile::TempDir;

const ROUNDS: usize = 60;

/// A tree of folders and files with names and bytes nobody chose, an empty
/// folder and an empty file among them now and then. Every path it wrote,
/// files and folders, relative to `root`.
fn tree(root: &Path) -> Vec<PathBuf> {
    let mut written = Vec::new();
    let mut folders = vec![PathBuf::new()];
    for _ in 0..random::between(1, 12) {
        let parent = random::pick(&folders);
        let name = random::pick(&[
            random::word(),
            random::kebab(),
            format!("{}.md", random::word()),
            format!(".{}", random::word()),
        ]);
        let path = parent.join(name);
        if root.join(&path).exists() {
            continue;
        }
        if random::between(0, 3) == 0 {
            fs::create_dir_all(root.join(&path)).unwrap();
            folders.push(path.clone());
        } else {
            fs::create_dir_all(root.join(&parent)).unwrap();
            fs::write(root.join(&path), random::bytes(random::between(0, 200))).unwrap();
        }
        written.push(path);
    }
    written
}

fn files(root: &Path, written: &[PathBuf]) -> Vec<PathBuf> {
    written
        .iter()
        .filter(|path| root.join(path).is_file())
        .cloned()
        .collect()
}

#[test]
fn a_copy_is_the_same_tree_and_hashes_as_its_source_does() {
    for _ in 0..ROUNDS {
        let (from, to) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        tree(from.path());
        let copied = to.path().join("copy");

        copy::tree(from.path(), &copied, from.path()).unwrap();

        assert!(copy::same(from.path(), &copied).unwrap());
        assert_eq!(
            hash::tree(from.path()).unwrap(),
            hash::tree(&copied).unwrap()
        );
    }
}

#[test]
fn a_tree_hashes_alike_wherever_it_is_and_whatever_its_folder_is_called() {
    for _ in 0..ROUNDS {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join(random::word()).join("deploy");
        fs::create_dir_all(&first).unwrap();
        tree(&first);
        let second = dir.path().join("elsewhere").join(random::kebab());

        copy::tree(&first, &second, &first).unwrap();

        assert_eq!(hash::tree(&first).unwrap(), hash::tree(&second).unwrap());
    }
}

#[test]
fn a_byte_changed_anywhere_changes_the_hash_and_the_comparison_alike() {
    for _ in 0..ROUNDS {
        let (from, to) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let written = tree(from.path());
        let files = files(from.path(), &written);
        if files.is_empty() {
            continue;
        }
        let copied = to.path().join("copy");
        copy::tree(from.path(), &copied, from.path()).unwrap();
        let recorded = hash::tree(&copied).unwrap();

        let edited = copied.join(random::pick(&files));
        let mut bytes = fs::read(&edited).unwrap();
        match (bytes.is_empty(), random::between(0, 2)) {
            (true, _) | (false, 0) => bytes.push(random::number() as u8),
            (false, 1) => {
                bytes.pop();
            }
            (false, _) => {
                let at = random::between(0, bytes.len() - 1);
                bytes[at] = bytes[at].wrapping_add(1);
            }
        }
        fs::write(&edited, &bytes).unwrap();

        assert_ne!(hash::tree(&copied).unwrap(), recorded, "{edited:?}");
        assert!(!copy::same(from.path(), &copied).unwrap(), "{edited:?}");
    }
}

#[test]
fn a_file_or_a_folder_added_or_taken_away_changes_both_alike() {
    for _ in 0..ROUNDS {
        let (from, to) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let written = tree(from.path());
        let copied = to.path().join("copy");
        copy::tree(from.path(), &copied, from.path()).unwrap();
        let recorded = hash::tree(&copied).unwrap();

        let changed = if random::chance() {
            let added = copied.join(format!("added-{}", random::word()));
            if random::chance() {
                fs::create_dir(&added).unwrap();
            } else {
                fs::write(&added, random::bytes(random::between(0, 20))).unwrap();
            }
            added
        } else {
            let taken = copied.join(random::pick(&written));
            if taken.is_dir() {
                fs::remove_dir_all(&taken).unwrap();
            } else {
                fs::remove_file(&taken).unwrap();
            }
            taken
        };

        assert_ne!(hash::tree(&copied).unwrap(), recorded, "{changed:?}");
        assert!(!copy::same(from.path(), &copied).unwrap(), "{changed:?}");
    }
}

#[test]
fn a_file_moved_into_another_folder_with_its_bytes_is_a_change() {
    for _ in 0..ROUNDS {
        let dir = TempDir::new().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        let bytes = random::bytes(random::between(0, 50));
        let name = random::word();
        fs::create_dir_all(a.join("one")).unwrap();
        fs::create_dir_all(a.join("two")).unwrap();
        fs::write(a.join("one").join(&name), &bytes).unwrap();
        fs::create_dir_all(b.join("one")).unwrap();
        fs::create_dir_all(b.join("two")).unwrap();
        fs::write(b.join("two").join(&name), &bytes).unwrap();

        assert_ne!(hash::tree(&a).unwrap(), hash::tree(&b).unwrap());
        assert!(!copy::same(&a, &b).unwrap());
    }
}
