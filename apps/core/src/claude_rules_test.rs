//! Unit tests for `claude_rules.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_rules_file_is_filed_by_its_route_in_mindflayers_own_folder() {
    let folder = Path::new(".claude").join("rules").join("mindflayer");
    assert_eq!(super::folder(), folder);
    assert_eq!(file_for("commit-style"), folder.join("commit-style.md"));
    assert_eq!(
        file_for("git/no-force-push"),
        folder.join("git").join("no-force-push.md")
    );
}

/// What [`plan`] decides, by name.
fn decided(asked: Option<&str>, recorded: Option<&str>, found: Found) -> &'static str {
    let recorded = recorded.map(|bytes| hash::bytes(bytes.as_bytes()));
    match plan(asked.map(str::as_bytes), recorded.as_deref(), found) {
        Plan::Create(_) => "create",
        Plan::Replace(_) => "replace",
        Plan::Keep(_) => "keep",
        Plan::Remove => "remove",
        Plan::Gone => "gone",
        Plan::Edited => "edited",
        Plan::Foreign => "foreign",
        Plan::Unlooked(_) => "unlooked",
    }
}

fn file(bytes: &str) -> Found {
    Found::File(bytes.as_bytes().to_vec())
}

#[test]
fn a_file_is_mindflayers_only_while_it_holds_what_was_recorded() {
    // Asked for.
    assert_eq!(decided(Some("new"), None, Found::Absent), "create");
    assert_eq!(decided(Some("new"), Some("old"), Found::Absent), "create");
    assert_eq!(decided(Some("new"), Some("old"), file("old")), "replace");
    assert_eq!(decided(Some("new"), Some("old"), file("new")), "keep");
    assert_eq!(decided(Some("new"), Some("old"), file("theirs")), "edited");
    assert_eq!(decided(Some("new"), Some("old"), Found::Other), "edited");
    assert_eq!(decided(Some("new"), None, file("theirs")), "foreign");
    // Holding the rule is not enough to be Mindflayer's.
    assert_eq!(decided(Some("new"), None, file("new")), "foreign");
    assert_eq!(decided(Some("new"), None, Found::Other), "foreign");

    // No longer asked for.
    assert_eq!(decided(None, Some("old"), file("old")), "remove");
    assert_eq!(decided(None, Some("old"), Found::Absent), "gone");
    assert_eq!(decided(None, Some("old"), file("theirs")), "edited");
    assert_eq!(decided(None, Some("old"), Found::Other), "edited");

    let unreadable = || Found::Unreadable(io::Error::other("no"));
    assert_eq!(decided(Some("new"), None, unreadable()), "unlooked");
    assert_eq!(decided(None, Some("old"), unreadable()), "unlooked");
}
