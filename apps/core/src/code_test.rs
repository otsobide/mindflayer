use super::*;

use std::path::PathBuf;

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
