//! Unit tests for `reapply.rs`, kept beside it rather than inside it.

use super::*;
use crate::settings::Settings;
use crate::workspace::Directories;

fn settings(agents: &[Agent]) -> Settings {
    Settings::new(
        "collapse",
        agents.to_vec(),
        Share::Local,
        Directories::default(),
    )
}

#[test]
fn naming_agents_names_each_once_in_the_order_given() {
    let before = settings(&[Agent::Claude]);
    let after =
        Change::Agents(vec![Agent::OpenCode, Agent::Codex, Agent::OpenCode]).applied(&before);
    assert_eq!(after.agents, [Agent::OpenCode, Agent::Codex]);
    assert_eq!(after.share, before.share);
}

#[test]
fn adding_agents_puts_those_not_named_after_the_rest() {
    let before = settings(&[Agent::Codex]);
    let after = Change::AddAgents(vec![Agent::Claude, Agent::Codex]).applied(&before);
    assert_eq!(after.agents, [Agent::Codex, Agent::Claude]);
}

#[test]
fn removing_agents_keeps_the_others_in_their_order() {
    let before = settings(&Agent::ALL);
    let after = Change::RemoveAgents(vec![Agent::Codex]).applied(&before);
    assert_eq!(after.agents, [Agent::Claude, Agent::OpenCode]);
    let none = Change::RemoveAgents(vec![Agent::Codex]).applied(&settings(&[Agent::Claude]));
    assert_eq!(none.agents, [Agent::Claude]);
}

#[test]
fn a_share_change_sets_both_its_settings_and_no_agent() {
    let mut before = settings(&[Agent::Claude]);
    before.edit_tracked = true;
    let after = Change::Share {
        share: Share::Shared,
        edit_tracked: false,
    }
    .applied(&before);
    assert_eq!(after.share, Share::Shared);
    assert!(!after.edit_tracked);
    assert_eq!(after.agents, before.agents);
    assert_eq!(after.id, before.id);
}

/// A folder holding one `SKILL.md` saying `body`, in `parent` under `name`.
fn folder(parent: &Path, name: &str, body: &str) -> PathBuf {
    let path = parent.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), body).unwrap();
    path
}

#[test]
fn a_copy_recorded_without_a_hash_is_untouched_while_it_matches_an_offer() {
    let dir = tempfile::TempDir::new().unwrap();
    let copy = folder(dir.path(), "copy", "Ship it");
    let other = folder(dir.path(), "other", "Something else");
    let offer = folder(dir.path(), "offer", "Ship it");
    // What an install leaves out of every copy does not make it differ.
    fs::create_dir(offer.join(".claude-plugin")).unwrap();

    assert!(matches!(
        compare(&copy, None, Some(&[other, offer])),
        Compared::Untouched
    ));
}

#[test]
fn a_copy_recorded_without_a_hash_is_edited_once_it_matches_no_offer() {
    let dir = tempfile::TempDir::new().unwrap();
    let copy = folder(dir.path(), "copy", "Ship it, edited");
    let offer = folder(dir.path(), "offer", "Ship it");

    assert!(matches!(
        compare(&copy, None, Some(&[offer])),
        Compared::Edited
    ));
}

#[test]
fn a_copy_recorded_without_a_hash_and_nothing_to_hold_it_to_cannot_be_compared() {
    let dir = tempfile::TempDir::new().unwrap();
    let copy = folder(dir.path(), "copy", "Ship it");

    let Compared::Unknown(unlooked) = compare(&copy, None, None) else {
        panic!("compared without an offer");
    };
    assert_eq!(unlooked, "it was recorded without a hash");
    let Compared::Unknown(unoffered) = compare(&copy, None, Some(&[])) else {
        panic!("compared with no offer");
    };
    assert_eq!(
        unoffered,
        "it was recorded without a hash, and nothing offers it to hold it to"
    );
}

#[test]
fn a_recorded_hash_decides_whatever_is_offered() {
    let dir = tempfile::TempDir::new().unwrap();
    let copy = folder(dir.path(), "copy", "Ship it");
    let offer = folder(dir.path(), "offer", "Ship it");
    let hash = hash::tree(&copy).unwrap();

    assert!(matches!(
        compare(&copy, Some(&hash), Some(&[])),
        Compared::Untouched
    ));
    assert!(matches!(
        compare(&copy, Some("0000"), Some(&[offer])),
        Compared::Edited
    ));
}
