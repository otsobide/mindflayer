use super::*;

use std::fs;
use std::path::{Path, PathBuf};

/// A code reaches the panel without the type it came from, so two alike
/// anywhere in core would be two conditions a front end cannot tell apart.
#[test]
fn no_two_codes_are_alike_anywhere_in_core() {
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let mut twice: Vec<String> = Vec::new();
    for (owner, codes) in ALL {
        for code in *codes {
            if let Some((first, _)) = seen.iter().find(|(_, seen)| seen == code) {
                twice.push(format!("`{code}` in {first} and {owner}"));
            }
            seen.push((owner, code));
        }
    }

    assert!(twice.is_empty(), "codes given twice: {twice:?}");
}

#[test]
fn every_code_is_snake_case() {
    let unfit: Vec<&str> = ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .filter(|code| {
            code.is_empty()
                || !code.split('_').all(|word| {
                    !word.is_empty()
                        && word
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                })
        })
        .collect();

    assert!(unfit.is_empty(), "codes not in snake_case: {unfit:?}");
}

#[test]
fn every_type_is_listed_once() {
    let mut owners: Vec<&str> = ALL.iter().map(|(owner, _)| *owner).collect();
    owners.sort_unstable();
    let listed = owners.len();
    owners.dedup();

    assert_eq!(owners.len(), listed);
}

#[test]
fn a_variant_that_carries_an_error_takes_its_code() {
    let busy = LockError::Busy {
        path: PathBuf::from("/work/.mindflayer/lock"),
        pid: Some(7),
        waited: std::time::Duration::from_secs(10),
    };

    assert_eq!(busy.code(), "workspace_busy");
    assert_eq!(
        LedgerError::Lock(busy).code(),
        "workspace_busy",
        "the ledger's lock is the workspace's"
    );
}

/// Every public type in core that derives `Error`, read off its source, has
/// its codes in `ALL`: one that never joined would give codes no uniqueness
/// test sees. The compiler holds each listed type's variants to a code; only
/// a type left out of the list goes unseen, so this is the test that finds it.
/// It finds only what derives `Error`, as every public error type in core
/// does: one written without the derive would go unseen here.
#[test]
fn every_public_error_type_in_core_is_listed() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found: Vec<String> = Vec::new();
    errors_in(&source, &source, &mut found);
    let listed: Vec<&str> = ALL.iter().map(|(owner, _)| *owner).collect();

    let unlisted: Vec<&String> = found
        .iter()
        .filter(|owner| !listed.contains(&owner.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "error types with no codes in ALL: {unlisted:?}"
    );

    // What `validate` finds is no error, and takes codes from the same list
    // so that the panel tells one from another the same way.
    let strays: Vec<&&str> = listed
        .iter()
        .filter(|owner| **owner != "artifact::ValidationIssue")
        .filter(|owner| !found.iter().any(|found| found == **owner))
        .collect();
    assert!(
        strays.is_empty(),
        "listed under a path that holds no such error type: {strays:?}"
    );
}

/// The path, as `ALL` spells it, of every public type deriving `Error` in
/// the `.rs` files under `folder`, unit tests left out.
fn errors_in(root: &Path, folder: &Path, found: &mut Vec<String>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            errors_in(root, &path, found);
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap();
        if !name.ends_with(".rs") || name.ends_with("_test.rs") {
            continue;
        }
        let module = path
            .strip_prefix(root)
            .unwrap()
            .with_extension("")
            .components()
            .map(|component| component.as_os_str().to_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .join("::");
        let text = fs::read_to_string(&path).unwrap();
        let mut deriving = false;
        for line in text.lines().map(str::trim_start) {
            if let Some(derived) = line.strip_prefix("#[derive(") {
                deriving = derived
                    .trim_end_matches(")]")
                    .split(',')
                    .map(str::trim)
                    .any(|derive| derive == "Error" || derive.ends_with("::Error"));
                continue;
            }
            if !deriving {
                continue;
            }
            // Attributes and doc comments between the derive and the type,
            // an `#[error(...)]` written over several lines among them, are
            // passed over; the first item after them is the type.
            let item = ["pub enum ", "pub struct "]
                .iter()
                .find_map(|keyword| line.strip_prefix(keyword));
            if let Some(item) = item {
                let name: String = item
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                found.push(format!("{module}::{name}"));
                deriving = false;
            } else if line.starts_with("enum ")
                || line.starts_with("struct ")
                || line.starts_with("pub(")
            {
                deriving = false;
            }
        }
    }
}
