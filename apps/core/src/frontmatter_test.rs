//! Unit tests for `frontmatter.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn splits_a_document_at_its_fences() {
    let doc = split("---\nname: a\n---\nbody\n").unwrap();
    assert_eq!(doc.front_matter, "name: a\n");
    assert_eq!(doc.body, "body\n");
}

#[test]
fn keeps_inner_dashes_that_are_not_fences() {
    let doc = split("---\nname: a\n---\nchapter\n---\nnext\n").unwrap();
    assert_eq!(doc.front_matter, "name: a\n");
    assert_eq!(doc.body, "chapter\n---\nnext\n");
}

#[test]
fn rejects_a_document_without_an_opening_fence() {
    assert_eq!(split("name: a\n"), Err(FrontMatterError::Missing));
    assert_eq!(split(""), Err(FrontMatterError::Missing));
}

#[test]
fn rejects_front_matter_that_is_never_closed() {
    assert_eq!(split("---\nname: a\n"), Err(FrontMatterError::Unterminated));
}

#[test]
fn a_key_is_only_a_key_at_the_start_of_a_line() {
    assert!(is_key("name: a\n", "name"));
    assert!(is_key("name : a\n", "name"));
    assert!(!is_key("names: a\n", "name"));
    assert!(!is_key("  name: a\n", "name"));
    assert!(!is_key("# name: a\n", "name"));
}

#[test]
fn dropping_a_key_takes_its_continuation_lines_with_it() {
    let front = "# kept\nname: a\ndescription: >\n  long\n  text\nlicense: MIT\n";
    assert_eq!(
        without_keys(front, &["name", "description"]),
        "# kept\nlicense: MIT\n"
    );
}

#[test]
fn replacing_a_key_keeps_every_other_byte() {
    let front = "# who\nname: old # was\r\nallowed-tools: Read\n";
    assert_eq!(
        with_key(front, "name", "new").as_deref(),
        Some("# who\nname: new\r\nallowed-tools: Read\n")
    );
}

#[test]
fn a_value_that_goes_on_below_its_key_is_not_replaced() {
    assert_eq!(with_key("name: >\n  a\n", "name", "b"), None);
    assert_eq!(with_key("name:\n  - a\n", "name", "b"), None);
    assert_eq!(with_key("name: a\nname: b\n", "name", "c"), None);
    assert_eq!(with_key("other: a\n", "name", "c"), None);
}
