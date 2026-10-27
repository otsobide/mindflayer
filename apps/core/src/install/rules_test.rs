//! Unit tests for `install/rules.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn the_agents_a_withheld_block_misses_are_said_in_one_clause() {
    assert_eq!(missing(&[]), "");
    assert_eq!(missing(&[Agent::Codex]), ", and codex reads none of them");
    assert_eq!(
        missing(&[Agent::Codex, Agent::OpenCode]),
        ", and codex and opencode read none of them"
    );
    assert_eq!(
        missing(&Agent::ALL),
        ", and claude, codex and opencode read none of them"
    );
}

#[test]
fn a_rules_file_for_claude_is_named_by_its_route_with_slashes() {
    assert_eq!(
        claude_file("git/no-force-push"),
        ".claude/rules/mindflayer/git/no-force-push.md"
    );
    assert_eq!(claude_file("review"), ".claude/rules/mindflayer/review.md");
}
