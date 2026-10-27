//! Unit tests for `agent.rs`, kept beside it rather than inside it.

use super::*;

use std::path::Component;

#[test]
fn each_agents_default_folder_is_its_own_in_the_users_home() {
    let user = Path::new("/home/user");

    assert_eq!(
        AgentHomes::defaults_under(user),
        AgentHomes {
            claude_config_dir: Some(user.join(".claude")),
            codex_home: Some(user.join(".codex")),
            opencode_config_dir: Some(user.join(".config").join("opencode")),
            agents_dir: Some(user.join(".agents")),
        }
    );
}

#[test]
fn an_agent_is_named_by_its_binary_and_shown_the_same_way() {
    let names: Vec<&str> = Agent::ALL.iter().map(|agent| agent.name()).collect();
    assert_eq!(names, ["claude", "codex", "opencode"]);
    for agent in Agent::ALL {
        assert_eq!(agent.name().parse::<Agent>(), Ok(agent));
        assert_eq!(agent.to_string(), agent.name());
    }
}

#[test]
fn a_word_that_is_not_an_agent_is_refused_with_every_name_there_is() {
    let error = "cursor".parse::<Agent>().unwrap_err();

    assert_eq!(error, UnknownAgent("cursor".to_owned()));
    assert_eq!(
        error.to_string(),
        "unknown agent `cursor`; expected claude, codex or opencode"
    );
}

#[test]
fn an_agent_is_taken_by_its_exact_name_only() {
    // One spelling per agent, so a settings file and a command line never
    // hold two that mean the same.
    for word in ["Claude", "OpenCode", "open-code", " codex", ""] {
        assert_eq!(word.parse::<Agent>(), Err(UnknownAgent(word.to_owned())));
    }
}

#[test]
fn every_agent_is_listed_once_in_report_order() {
    // What sorts agents sorts them by the derived order, so `ALL` must be in
    // it too, or two reports would list them two ways.
    assert!(Agent::ALL.windows(2).all(|pair| pair[0] < pair[1]));
}

/// Where the standard table puts a skill for Claude, and for Codex and
/// OpenCode, joined a folder at a time as the table joins them.
fn claude_skills() -> PathBuf {
    Path::new(".claude").join("skills")
}

fn agents_skills() -> PathBuf {
    Path::new(".agents").join("skills")
}

#[test]
fn all_three_agents_take_a_skill_in_two_places() {
    assert_eq!(
        TargetTable::standard().targets(Kind::Skill, &Agent::ALL),
        [
            AgentTarget {
                path: agents_skills(),
                agents: vec![Agent::Codex, Agent::OpenCode],
            },
            AgentTarget {
                path: claude_skills(),
                agents: vec![Agent::Claude],
            },
        ]
    );
}

#[test]
fn codex_and_opencode_share_one_place_for_a_skill() {
    assert_eq!(
        TargetTable::standard().targets(Kind::Skill, &[Agent::Codex, Agent::OpenCode]),
        [AgentTarget {
            path: agents_skills(),
            agents: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
}

#[test]
fn claude_alone_takes_a_skill_in_its_own_place() {
    assert_eq!(
        TargetTable::standard().targets(Kind::Skill, &[Agent::Claude]),
        [AgentTarget {
            path: claude_skills(),
            agents: vec![Agent::Claude],
        }]
    );
}

#[test]
fn no_agents_take_nothing() {
    let table = TargetTable::standard();
    for kind in Kind::ALL {
        assert_eq!(table.targets(kind, &[]), []);
    }
}

#[test]
fn targets_come_sorted_and_once_whatever_order_the_agents_are_named_in() {
    let table = TargetTable::standard();

    assert_eq!(
        table.targets(
            Kind::Skill,
            &[
                Agent::OpenCode,
                Agent::Claude,
                Agent::Codex,
                Agent::OpenCode
            ]
        ),
        table.targets(Kind::Skill, &Agent::ALL)
    );
}

#[test]
fn a_rule_reaches_no_agent_through_a_folder_yet() {
    // D-03 gives rules a section of a block in `AGENTS.md` instead, which is
    // M2's to add; until then the table has no entry for one.
    assert_eq!(TargetTable::standard().targets(Kind::Rule, &Agent::ALL), []);
}

#[test]
fn agents_sharing_a_location_another_table_names_get_one_target() {
    // What a project's settings could make of the table (#40): one folder
    // for every agent is one copy, for all three, whether or not any of them
    // reads it.
    let table = TargetTable {
        locations: Agent::ALL
            .into_iter()
            .map(|agent| ((Kind::Skill, agent), PathBuf::from(".skills")))
            .collect(),
    };

    assert_eq!(
        table.targets(Kind::Skill, &Agent::ALL),
        [AgentTarget {
            path: PathBuf::from(".skills"),
            agents: Agent::ALL.to_vec(),
        }]
    );
}

#[test]
fn the_standard_table_never_targets_codexs_old_folders_or_a_users_own() {
    let table = TargetTable::standard();
    let codex_skills = Path::new(".codex").join("skills");
    for kind in Kind::ALL {
        for target in table.targets(kind, &Agent::ALL) {
            let path = &target.path;
            // Inside the project, never in a home: `~/.claude/skills`, with
            // whatever Claude keeps under it, `~/.codex/skills` and
            // `~/.agents/skills` are user scope, which D-22 leaves for later,
            // behind an explicit `--user`. A `~` is checked for by name, since
            // to a path it is a folder like any other.
            assert!(
                path.components()
                    .all(|part| matches!(part, Component::Normal(name) if name != "~")),
                "{kind} reaches {} outside the project",
                path.display()
            );
            // Codex reads a project's `.codex/skills` only when the project is
            // trusted, and `$CODEX_HOME/skills` is deprecated (D-07,
            // docs/agents.md).
            assert!(
                !path.starts_with(&codex_skills),
                "{kind} reaches {}, which D-07 never targets",
                path.display()
            );
        }
    }
}

#[test]
fn claude_and_codex_each_read_skills_in_one_folder_of_their_own() {
    assert_eq!(Agent::Claude.reads(Kind::Skill), [claude_skills()]);
    assert_eq!(Agent::Codex.reads(Kind::Skill), [agents_skills()]);
}

#[test]
fn opencode_reads_claudes_skills_as_well_as_the_folder_it_is_sent_a_copy_in() {
    // More than the table sends it: what it reads is not where it is
    // written for.
    assert_eq!(
        Agent::OpenCode.reads(Kind::Skill),
        [
            agents_skills(),
            claude_skills(),
            Path::new(".opencode").join("skills")
        ]
    );
    assert_eq!(
        TargetTable::standard().targets(Kind::Skill, &[Agent::OpenCode]),
        [AgentTarget {
            path: agents_skills(),
            agents: vec![Agent::OpenCode],
        }]
    );
}

#[test]
fn every_agent_reads_each_place_the_standard_table_sends_it() {
    let table = TargetTable::standard();
    for kind in Kind::ALL {
        for agent in Agent::ALL {
            for target in table.targets(kind, &[agent]) {
                assert!(
                    agent.reads(kind).contains(&target.path),
                    "{agent} is sent {kind} copies in {}, which it does not read",
                    target.path.display()
                );
            }
        }
    }
}

#[test]
fn no_agent_is_said_to_read_outside_the_project_or_codexs_trusted_folder() {
    let codex_skills = Path::new(".codex").join("skills");
    for kind in Kind::ALL {
        for agent in Agent::ALL {
            for path in agent.reads(kind) {
                assert!(
                    path.components()
                        .all(|part| matches!(part, Component::Normal(name) if name != "~")),
                    "{agent} reads {kind} at {}, outside the project",
                    path.display()
                );
                assert_ne!(path, codex_skills, "{agent} reads {kind} only when trusted");
            }
        }
    }
}

#[test]
fn no_agent_is_said_to_read_a_folder_of_rules_yet() {
    for agent in Agent::ALL {
        assert_eq!(agent.reads(Kind::Rule), Vec::<PathBuf>::new());
    }
}
