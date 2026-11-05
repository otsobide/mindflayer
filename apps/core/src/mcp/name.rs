//! Which names an MCP server can be given: the key each agent's config
//! names it by, and so its definition's file (D-10).
//!
//! Every agent keys a server by letters, digits, hyphens and underscores
//! (`^[A-Za-z0-9_-]+$`), so a definition is held to that, wider than the
//! kebab-case every other kind's name is held to: a server's name is no
//! skill an agent is asked to invoke, and kebab-case alone, as #86 left it,
//! would call `my_server` or `GitHub` invalid for a rule no agent has. The
//! 64 characters every name is held to hold here too, one rule of length
//! for every kind.
//!
//! Two names that differ only by case are one name here, though an agent's
//! config keys them apart: a file system that folds case (macOS's and
//! Windows' by default) cannot hold `GitHub.toml` beside `github.toml`, so
//! a library holding both is a different library on a teammate's machine.
//! So a name an agent keeps for itself is kept in every case, and two
//! definitions beside each other whose names differ only by case clash
//! (`artifact`, which knows what is beside a definition).
//!
//! What each agent keeps for itself is a table matched on the agent with no
//! wildcard, so a new agent says what it keeps before it compiles. Each
//! fact is docs-only, from #87, and `docs/agents.md`'s "MCP servers" says
//! what #18's run on the binaries would move.

use crate::agent::Agent;
use crate::artifact::ValidationIssue;
use crate::skill::MAX_NAME_SEGMENT_LEN;
use crate::value::ArtifactName;

/// Everything wrong with `name` as an MCP server's, in the order it is
/// worth fixing: nothing at all, a character no agent keys a server by, a
/// length past the one every name is held to, a name an agent keeps for
/// itself, and, only worth a warning, one an agent uses for one of its own
/// without refusing a server's.
pub(crate) fn issues(name: &str) -> Vec<ValidationIssue> {
    if name.is_empty() {
        return vec![ValidationIssue::NameEmpty];
    }
    let mut issues = Vec::new();
    if !is_keyed(name) {
        issues.push(ValidationIssue::McpNameNotAKey {
            name: name.to_owned(),
        });
    }
    // Characters, as every name's length is counted.
    let length = name.chars().count();
    if length > MAX_NAME_SEGMENT_LEN {
        issues.push(ValidationIssue::NameSegmentTooLong {
            segment: name.to_owned(),
            length,
        });
    }
    for agent in Agent::ALL {
        for reserved in reserved(agent) {
            if collides(name, reserved) {
                issues.push(ValidationIssue::McpNameReserved {
                    name: name.to_owned(),
                    agent,
                    reserved: (*reserved).to_owned(),
                });
            }
        }
    }
    for agent in Agent::ALL {
        for claimed in claimed(agent) {
            if collides(name, claimed) {
                issues.push(ValidationIssue::McpNameClaimed {
                    name: name.to_owned(),
                    agent,
                    claimed: (*claimed).to_owned(),
                });
            }
        }
    }
    issues
}

/// Whether `name` is one every agent's config can key a server by, and so
/// one a definition's file can be named by: letters, digits, hyphens and
/// underscores, at least one, and no more than every name may be long.
///
/// Not whether an agent keeps it for itself, which is `validate`'s to say
/// ([`issues`]): a name is filed safely either way.
pub fn is_key(name: &str) -> bool {
    !name.is_empty() && is_keyed(name) && name.chars().count() <= MAX_NAME_SEGMENT_LEN
}

/// Every name among `beside` that is `name` in another case: what a folder
/// holding `name`'s definition cannot hold beside it everywhere, since a
/// file system that folds case reads the two as one file.
pub(crate) fn twins<'a>(
    name: &str,
    beside: impl IntoIterator<Item = &'a str>,
) -> Vec<ArtifactName> {
    beside
        .into_iter()
        .filter(|other| *other != name && other.eq_ignore_ascii_case(name))
        .map(ArtifactName::from)
        .collect()
}

/// Whether every character of `name` is one an agent keys a server by.
fn is_keyed(name: &str) -> bool {
    name.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The names `agent` keeps for itself, each as it spells it, which no
/// server of somebody's can be keyed by.
///
/// Claude's own servers, and the two keys OpenCode reads in its `mcp`
/// table beside the servers, where its V1 shape keys each server
/// (`mcp.<name>`), so a server named so would be read as that key there.
const fn reserved(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &[
            "workspace",
            "claude-in-chrome",
            "computer-use",
            "Claude Preview",
            "Claude Browser",
        ],
        Agent::Codex => &[],
        Agent::OpenCode => &["servers", "timeout"],
    }
}

/// The names `agent` uses for one of its own and does not refuse a
/// server's: the skills Claude is given by Anthropic, which a server of the
/// same name would be read beside.
const fn claimed(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &["anthropic-skills"],
        Agent::Codex | Agent::OpenCode => &[],
    }
}

/// Whether `name` stands for `word`, in any case, since two names that
/// differ by case alone are one here.
///
/// A word with a space in it, as two of Claude's are, is one no key can
/// hold, so it is taken to collide with each spelling of it a key can:
/// the space written as an underscore, a hyphen, or left out
/// (`claude_preview`, `claude-preview`, `claudepreview`). Which of those
/// Claude folds a space into before it compares is not known without #18's
/// run, and holding all three costs no name anybody would give a server of
/// their own.
fn collides(name: &str, word: &str) -> bool {
    if word.contains(' ') {
        ["_", "-", ""]
            .into_iter()
            .any(|space| name.eq_ignore_ascii_case(&word.replace(' ', space)))
    } else {
        name.eq_ignore_ascii_case(word)
    }
}

#[cfg(test)]
#[path = "name_test.rs"]
mod tests;
