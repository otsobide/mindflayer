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
