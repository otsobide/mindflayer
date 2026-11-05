use super::*;
use crate::mother::{random, McpMother};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// What each agent cannot be given of `text`'s definition, in report
/// order, each agent by its name.
fn by_agent(text: &str) -> Vec<(&'static str, Vec<Inexpressible>)> {
    let definition = parsed(text);
    Agent::ALL
        .into_iter()
        .map(|agent| (agent.name(), definition.inexpressible(agent)))
        .collect()
}

fn named(text: &str) -> InputName {
    InputName::from(text)
}

#[test]
fn a_program_with_no_folder_and_a_secret_environment_is_given_whole_to_every_agent() {
    assert_eq!(
        by_agent(&McpMother::without_cwd(&random::kebab())),
        [("claude", vec![]), ("codex", vec![]), ("opencode", vec![])]
    );
}

#[test]
fn claude_is_given_a_program_without_the_folder_it_is_told_to_start_in() {
    let report = by_agent(&McpMother::stdio(&random::kebab()));

    assert_eq!(
        report,
        [
            ("claude", vec![Inexpressible::WorkingDirectory]),
            ("codex", vec![]),
            ("opencode", vec![])
        ]
    );
    assert!(!Inexpressible::WorkingDirectory.refuses());
}

#[test]
fn codex_cannot_be_given_a_secret_argument() {
    let report = by_agent(&McpMother::secret_argument(&random::kebab()));

    assert_eq!(
        report[1],
        (
            "codex",
            vec![Inexpressible::SecretInArgument {
                input: named("workspace")
            }]
        )
    );
    assert_eq!(report[2], ("opencode", vec![]));
    assert_eq!(report[0], ("claude", vec![Inexpressible::WorkingDirectory]));
}

#[test]
fn codex_cannot_be_given_a_secret_inside_a_url() {
    assert_eq!(
        by_agent(&McpMother::secret_in_url(&random::kebab())),
        [
            ("claude", vec![]),
            (
                "codex",
                vec![Inexpressible::SecretInUrl {
                    input: named("tenant")
                }]
            ),
            ("opencode", vec![])
        ]
    );
}

#[test]
fn codex_cannot_be_given_a_secret_header_beside_other_text_but_bearer() {
    assert_eq!(
        by_agent(&McpMother::composite_secret_header(&random::kebab())),
        [
            ("claude", vec![]),
            (
                "codex",
                vec![Inexpressible::SecretHeaderComposite {
                    header: String::from("Authorization"),
                    input: named("token"),
                }]
            ),
            ("opencode", vec![])
        ]
    );
    // `Authorization: Bearer` and the token, in either case, and a header
    // that is the secret whole, are each a variable Codex reads.
    let name = random::kebab();
    for whole in [
        McpMother::http(&name),
        McpMother::whole_secret_header(&name),
        McpMother::http(&name).replacen(
            "Authorization = \"Bearer {token}\"",
            "authorization = \"bearer {token}\"",
            1,
        ),
    ] {
        assert_eq!(parsed(&whole).inexpressible(Agent::Codex), [], "{whole}");
    }
}

#[test]
fn neither_codex_nor_opencode_can_be_given_server_sent_events() {
    assert_eq!(
        by_agent(&McpMother::sse(&random::kebab())),
        [
            ("claude", vec![]),
            ("codex", vec![Inexpressible::ServerSentEvents]),
            ("opencode", vec![Inexpressible::ServerSentEvents])
        ]
    );
}

#[test]
fn claude_cannot_be_given_an_address_read_from_a_variable_it_keeps_from_one() {
    let name = random::kebab();
    for (why, variable) in McpMother::blanked_variables() {
        let blanked = |input: &str| {
            vec![Inexpressible::VariableBlanked {
                input: named(input),
                variable: VariableName::from(variable.as_str()),
            }]
        };
        for (text, input) in [
            (McpMother::header_read_from(&name, &variable), "token"),
            (McpMother::url_read_from(&name, &variable), "tenant"),
        ] {
            assert_eq!(
                by_agent(&text),
                [
                    ("claude", blanked(input)),
                    ("codex", vec![]),
                    ("opencode", vec![])
                ],
                "{why}: {text}"
            );
        }
    }
}

#[test]
fn claude_is_given_an_address_read_from_a_variable_it_passes_on_and_a_program_from_any() {
    let name = random::kebab();
    for (why, variable) in McpMother::passed_variables() {
        let text = McpMother::header_read_from(&name, &variable);
        assert_eq!(
            parsed(&text).inexpressible(Agent::Claude),
            [],
            "{why}: {text}"
        );
    }
    // A program's environment is no remote server's.
    for (why, variable) in McpMother::blanked_variables() {
        let text = McpMother::env_read_from(&name, &variable).replacen("cwd = \".\"\n", "", 1);
        assert_eq!(
            parsed(&text).inexpressible(Agent::Claude),
            [],
            "{why}: {text}"
        );
    }
}

#[test]
fn a_shape_an_agent_cannot_express_is_nothing_validate_says() {
    let name = random::kebab();
    for text in [
        McpMother::stdio(&name),
        McpMother::secret_argument(&name),
        McpMother::secret_in_url(&name),
        McpMother::composite_secret_header(&name),
        McpMother::sse(&name),
        McpMother::header_read_from(&name, "NPM_TOKEN"),
    ] {
        let definition = parsed(&text);

        assert!(
            Agent::ALL
                .into_iter()
                .any(|agent| !definition.inexpressible(agent).is_empty()),
            "{text}"
        );
        assert_eq!(definition.issues(), [], "{text}");
    }
}

#[test]
fn every_shape_but_a_folder_left_out_refuses_the_agent_the_definition() {
    let input = named("token");
    let shapes = [
        Inexpressible::SecretInArgument {
            input: input.clone(),
        },
        Inexpressible::SecretInUrl {
            input: input.clone(),
        },
        Inexpressible::SecretHeaderComposite {
            header: String::from("X-Key"),
            input,
        },
        Inexpressible::ServerSentEvents,
        Inexpressible::WorkingDirectory,
        Inexpressible::VariableBlanked {
            input: named("token"),
            variable: VariableName::from("NPM_TOKEN"),
        },
    ];
    assert_eq!(
        shapes.len(),
        Inexpressible::CODES.len(),
        "one of each shape"
    );

    let refused: Vec<(&str, bool)> = shapes
        .iter()
        .map(|shape| (shape.code(), shape.refuses()))
        .collect();
    assert_eq!(
        refused,
        [
            ("secret_in_argument", true),
            ("secret_in_url", true),
            ("secret_header_composite", true),
            ("sse_unsupported", true),
            ("cwd_unsupported", false),
            ("variable_blanked", true),
        ]
    );
}

#[test]
fn the_report_holds_every_agent_in_report_order_what_refuses_it_apart_from_what_is_dropped() {
    let definition = parsed(&McpMother::secret_argument(&random::kebab()));

    let report = definition.expressibility();

    let agents: Vec<Agent> = report.iter().map(|one| one.agent).collect();
    assert_eq!(agents, Agent::ALL);
    let codes = |said: &[Said]| said.iter().map(|said| said.code).collect::<Vec<_>>();
    assert_eq!(codes(&report[0].refused), [] as [&str; 0]);
    assert_eq!(codes(&report[0].dropped), ["cwd_unsupported"]);
    assert_eq!(codes(&report[1].refused), ["secret_in_argument"]);
    assert_eq!(codes(&report[1].dropped), [] as [&str; 0]);
    assert!(report[2].refused.is_empty() && report[2].dropped.is_empty());
    assert_eq!(
        report[1].refused[0].message,
        Inexpressible::SecretInArgument {
            input: named("workspace")
        }
        .to_string()
        .as_str()
    );
}

/// What `show --json` hands a reader is a contract, each field by its name.
#[test]
fn an_agents_report_serializes_its_agent_by_name_and_each_shape_by_its_code() {
    let report = parsed(&McpMother::sse(&random::kebab())).expressibility();

    let serialized = serde_json::to_value(&report).unwrap();

    let refused = serde_json::json!([{
        "code": "sse_unsupported",
        "message": "the sse transport, server-sent events, is not one taken there",
    }]);
    assert_eq!(
        serialized,
        serde_json::json!([
            {"agent": "claude", "refused": [], "dropped": []},
            {"agent": "codex", "refused": refused, "dropped": []},
            {"agent": "opencode", "refused": refused, "dropped": []},
        ])
    );
}
