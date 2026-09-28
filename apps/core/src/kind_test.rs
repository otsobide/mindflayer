//! Unit tests for `kind.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_kind_is_named_by_its_slug_or_its_folder() {
    for kind in Kind::ALL {
        assert_eq!(kind.slug().parse::<Kind>(), Ok(kind));
        assert_eq!(kind.folder().parse::<Kind>(), Ok(kind));
    }
}

#[test]
fn a_word_that_is_not_a_kind_says_what_was_expected() {
    let error = "prompts".parse::<Kind>().unwrap_err();
    assert!(error.to_string().contains("skills"));
    assert!(error.to_string().contains("rules"));
}

#[test]
fn every_kind_has_its_own_folder_and_slug() {
    // Two kinds sharing either would make discovery and `show` ambiguous.
    let folders: Vec<&str> = Kind::ALL.iter().map(|k| k.folder()).collect();
    let slugs: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
    for (index, kind) in Kind::ALL.iter().enumerate() {
        assert_eq!(folders.iter().filter(|f| **f == kind.folder()).count(), 1);
        assert_eq!(slugs.iter().filter(|s| **s == kind.slug()).count(), 1);
        let _ = index;
    }
}
