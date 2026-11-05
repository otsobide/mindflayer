use super::*;
use crate::agent::Agent;
use crate::mother::{random, McpMother, SkillMother};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn input<'a>(definition: &'a McpDefinition, name: &str) -> &'a Input {
    definition
        .inputs
        .iter()
        .find(|input| input.name == name)
        .unwrap_or_else(|| panic!("no input {name}"))
}

#[test]
fn a_program_is_read_with_its_arguments_environment_folder_and_inputs() {
    let name = random::kebab();

    let definition = parsed(&McpMother::stdio(&name));

    assert_eq!(definition.name, name.as_str());
    assert!(!definition.description.is_empty());
    let Transport::Stdio(stdio) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert_eq!(stdio.command, "npx");
    assert_eq!(
        stdio.args,
        ["-y", "@example/server", "--workspace", "{workspace}"]
    );
    assert_eq!(
        stdio.env,
        BTreeMap::from([
            (String::from("API_KEY"), String::from("{api_key}")),
            (String::from("LOG_LEVEL"), String::from("info")),
        ])
    );
    assert_eq!(stdio.cwd.as_deref(), Some("."));
    assert_eq!(
        input(&definition, "api_key"),
        &Input {
            name: InputName::from("api_key"),
            kind: InputKind::Env,
            description: Some(String::from("The key the server signs in with")),
            required: true,
            secret: true,
            env_var: Some(VariableName::from("EXAMPLE_API_KEY")),
        }
    );
    let workspace = input(&definition, "workspace");
    assert_eq!(workspace.kind, InputKind::Arg);
    assert!(!workspace.required && !workspace.secret);
    assert_eq!(workspace.description, None);
    assert_eq!(definition.issues(), []);
}

#[test]
fn an_address_over_http_is_read_with_its_headers_inputs_and_where_it_came_from() {
    let definition = parsed(&McpMother::http(&random::kebab()));

    let Transport::Http(endpoint) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert_eq!(endpoint.url, "https://{tenant}.example.com/mcp");
    assert_eq!(
        endpoint.headers,
        BTreeMap::from([(
            String::from("Authorization"),
            String::from("Bearer {token}")
        )])
    );
    assert_eq!(input(&definition, "tenant").kind, InputKind::UrlVar);
    assert_eq!(input(&definition, "token").kind, InputKind::Header);
    assert!(input(&definition, "token").secret);
    assert_eq!(
        definition.provenance,
        Some(Upstream {
            registry: Some(Redacted::new("https://registry.modelcontextprotocol.io")),
            upstream_name: Some(String::from("com.example/server")),
            version: Some(String::from("1.2.0")),
            fetched_at: Some(String::from("2026-11-04T09:30:00Z")),
        })
    );
    assert_eq!(definition.issues(), []);
}

#[test]
fn an_address_over_server_sent_events_is_read_as_its_own_transport() {
    let definition = parsed(&McpMother::sse(&random::kebab()));

    let Transport::Sse(endpoint) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert_eq!(endpoint.url, "https://example.com/sse");
    assert_eq!(endpoint.headers["X-Api-Key"], "{key}");
    assert_eq!(definition.transport.slug(), "sse");
    assert_eq!(definition.issues(), []);
}

#[test]
fn every_definition_the_mother_draws_reads_with_nothing_wrong() {
    for _ in 0..100 {
        let (name, text) = McpMother::random();

        let definition = parsed(&text);
        assert_eq!(definition.name, name.as_str());
        assert_eq!(definition.issues(), [], "{text}");
    }
}

#[test]
fn a_definition_in_a_newer_format_is_refused_whatever_else_it_holds() {
    let newer = i64::from(MCP_FORMAT) + 1;
    let head = McpMother::head(&random::kebab(), &SkillMother::description());
    let text = format!(
        "{}\n[transport]\ntype = \"websocket\"\n",
        head.replacen("format = 1\n", &format!("format = {newer}\n"), 1)
    );

    let error = McpDefinition::parse(&text).unwrap_err();

    assert!(
        matches!(error, McpError::FormatTooNew { found } if found == newer),
        "{error}"
    );
    assert_eq!(error.code(), "mcp_format_too_new");
    assert_eq!(
        error.to_string(),
        format!("written in format {newer}, by a newer Mindweaver; this one reads format 1")
    );
}

#[test]
fn a_definition_naming_no_format_or_one_never_written_is_refused() {
    let name = random::kebab();
    let without = McpMother::stdio(&name).replacen("format = 1\n", "", 1);

    let error = McpDefinition::parse(&without).unwrap_err();
    assert!(matches!(error, McpError::NoFormat), "{error}");
    assert_eq!(error.code(), "mcp_format_missing");

    for (format, said) in [("0", "0"), ("\"1\"", "\"1\""), ("1.0", "1.0")] {
        let error = McpDefinition::parse(&McpMother::in_format(&name, format)).unwrap_err();
        assert!(
            matches!(&error, McpError::FormatUnknown { found } if found == said),
            "{format}: {error}"
        );
        assert_eq!(error.code(), "mcp_format_unknown");
    }
}

#[test]
fn a_file_that_is_no_toml_is_refused_as_unparsed() {
    let error = McpDefinition::parse("format = 1\nname = \n").unwrap_err();

    assert!(matches!(error, McpError::Unparsed(_)), "{error}");
    assert_eq!(error.code(), "mcp_unparsed");
}

#[test]
fn a_definition_missing_what_the_format_requires_or_typed_otherwise_is_refused() {
    let name = random::kebab();
    let description = SkillMother::description();
    let head = McpMother::head(&name, &description);
    let refused = [
        ("no transport", head.clone()),
        (
            "no description",
            format!(
                "format = 1\nname = {}\n\n[transport]\ntype = \"stdio\"\ncommand = \"npx\"\n",
                toml::Value::String(name.clone())
            ),
        ),
        (
            "a transport of no type the format has",
            format!("{head}\n[transport]\ntype = \"websocket\"\nurl = \"wss://example.com\"\n"),
        ),
        (
            "a transport with no type",
            format!("{head}\n[transport]\ncommand = \"npx\"\n"),
        ),
        (
            "an argument that is no string",
            format!("{head}\n[transport]\ntype = \"stdio\"\ncommand = \"npx\"\nargs = [1]\n"),
        ),
        (
            "an input of no kind the format has",
            format!(
                "{head}\n[transport]\ntype = \"stdio\"\ncommand = \"npx\"\n\n\
                 [[inputs]]\nname = \"x\"\nkind = \"file\"\n"
            ),
        ),
        (
            "extras that are no table",
            format!("{head}\nextras = {{ claude = 3 }}\n\n[transport]\ntype = \"sse\"\nurl = \"https://example.com\"\n"),
        ),
        (
            "a time of fetching that is a number",
            format!(
                "{head}\n[transport]\ntype = \"sse\"\nurl = \"https://example.com\"\n\n\
                 [provenance]\nfetched_at = 2026\n"
            ),
        ),
    ];

    for (why, text) in refused {
        let error = McpDefinition::parse(&text).unwrap_err();
        assert!(matches!(error, McpError::Malformed(_)), "{why}: {error}");
        assert_eq!(error.code(), "mcp_malformed", "{why}");
    }
}

#[test]
fn a_key_the_format_does_not_define_is_read_past() {
    let text = format!(
        "homepage = \"https://example.com\"\n{}",
        McpMother::sse(&random::kebab())
    )
    .replacen("type = \"sse\"\n", "type = \"sse\"\ntimeout = 30\n", 1);

    assert_eq!(parsed(&text).issues(), []);
}

#[test]
fn a_transport_naming_nothing_to_start_or_reach_is_read_and_said_to_be_incomplete() {
    let name = random::kebab();
    for (transport, field) in [("stdio", "command"), ("http", "url"), ("sse", "url")] {
        let definition = parsed(&McpMother::incomplete(&name, transport));

        assert_eq!(
            definition.issues(),
            [ValidationIssue::TransportIncomplete {
                transport: transport.to_owned(),
                field: field.to_owned(),
            }],
            "{transport}"
        );
    }
    let blank = McpMother::stdio(&name).replacen("command = \"npx\"", "command = \"  \"", 1);
    assert!(matches!(
        &parsed(&blank).issues()[..],
        [ValidationIssue::TransportIncomplete { field, .. }] if field == "command"
    ));
}

#[test]
fn an_input_nothing_uses_and_one_declared_twice_are_said_once_each() {
    let definition = parsed(&McpMother::unreferenced(&random::kebab()));

    assert_eq!(
        definition.issues(),
        [
            ValidationIssue::InputRepeated {
                name: String::from("workspace"),
            },
            ValidationIssue::InputUnreferenced {
                name: String::from("unused"),
                kind: InputKind::Env,
            },
        ]
    );
}

#[test]
fn an_input_is_used_only_where_its_kind_says_it_may_be_written() {
    let name = random::kebab();
    let head = McpMother::head(&name, &SkillMother::description());
    // Each kind written somewhere its kind does not say: an environment
    // input in an argument, an argument in the environment, and a header
    // and a part of an address on a program, which has neither.
    let program = |kind: &str| {
        format!(
            "{head}\n[transport]\ntype = \"stdio\"\ncommand = \"npx\"\n\
             args = [\"{{x}}\"]\nenv = {{ X = \"{{y}}\" }}\n\n\
             [[inputs]]\nname = \"x\"\nkind = \"env\"\n\n\
             [[inputs]]\nname = \"y\"\nkind = \"arg\"\n\n\
             [[inputs]]\nname = \"z\"\nkind = \"{kind}\"\n"
        )
    };
    for kind in [InputKind::Header, InputKind::UrlVar] {
        let issues = parsed(&program(kind.slug())).issues();
        let unused: Vec<(&str, InputKind)> = issues
            .iter()
            .filter_map(|issue| match issue {
                ValidationIssue::InputUnreferenced { name, kind } => Some((name.as_str(), *kind)),
                _ => None,
            })
            .collect();
        assert_eq!(
            unused,
            [("x", InputKind::Env), ("y", InputKind::Arg), ("z", kind)],
            "{issues:?}"
        );
    }
    // And an address, which has no environment and takes no arguments.
    let address = format!(
        "{head}\n[transport]\ntype = \"http\"\nurl = \"https://{{a}}.example.com\"\n\
         headers = {{ X = \"{{b}}\" }}\n\n\
         [[inputs]]\nname = \"a\"\nkind = \"env\"\n\n\
         [[inputs]]\nname = \"b\"\nkind = \"arg\"\n"
    );
    assert_eq!(
        parsed(&address).issues(),
        [
            ValidationIssue::InputUnreferenced {
                name: String::from("a"),
                kind: InputKind::Env,
            },
            ValidationIssue::InputUnreferenced {
                name: String::from("b"),
                kind: InputKind::Arg,
            },
        ]
    );
}

#[test]
fn an_input_written_in_an_addresss_userinfo_is_said_to_reach_no_agent_rather_than_go_unused() {
    let definition = parsed(&McpMother::in_userinfo(&random::kebab()));

    let Transport::Http(endpoint) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    // Kept without it, as every address is kept without its userinfo.
    assert_eq!(endpoint.url, "https://example.com/mcp");
    assert_eq!(endpoint.withheld, [InputName::from("tenant")]);
    let issues = definition.issues();
    assert_eq!(
        issues,
        [ValidationIssue::InputInUserinfo {
            name: String::from("tenant"),
        }]
    );
    let said = issues[0].to_string();
    assert!(said.contains("written in the user and password"), "{said}");
    assert!(!said.contains("used nowhere"), "{said}");

    // Written after the host as well, it is used there.
    let twice = McpMother::in_userinfo(&random::kebab()).replacen("/mcp\"", "/{tenant}\"", 1);
    assert_eq!(parsed(&twice).issues(), []);
    // And only a part of an address is: a header written there is written
    // where its kind does not say, as it would be anywhere in the address.
    let header = McpMother::in_userinfo(&random::kebab()).replacen(
        "kind = \"url_var\"",
        "kind = \"header\"",
        1,
    );
    assert!(matches!(
        &parsed(&header).issues()[..],
        [ValidationIssue::InputUnreferenced {
            kind: InputKind::Header,
            ..
        }]
    ));
}

#[test]
fn an_input_no_placeholder_can_be_written_of_and_a_variable_no_agent_can_name_are_said() {
    let definition = parsed(&McpMother::misnamed(&random::kebab()));

    assert_eq!(
        definition.issues(),
        [
            ValidationIssue::InputNameMalformed {
                name: String::from("api}key"),
            },
            ValidationIssue::EnvVarMalformed {
                name: String::from("api}key"),
                variable: String::from("EXAMPLE-API-KEY"),
            },
        ]
    );
    assert!(definition.issues().iter().all(|issue| !issue.is_warning()));
    assert_eq!(definition.issues()[0].code(), "input_name_malformed");
    assert_eq!(definition.issues()[1].code(), "env_var_malformed");
}

#[test]
fn a_secret_input_is_one_only_when_it_names_the_variable_it_is_read_from() {
    let name = random::kebab();
    let definition = parsed(&McpMother::secret_without_variable(&name));

    assert_eq!(input(&definition, "api_key").env_var, None);
    assert_eq!(
        definition.issues(),
        [ValidationIssue::SecretWithoutEnvVar {
            name: String::from("api_key"),
        }]
    );
    assert_eq!(definition.issues()[0].code(), "secret_without_env_var");
    assert!(!definition.issues()[0].is_warning());

    // One that is no secret may name none: its value is somebody's to
    // give when it is installed.
    let plain = McpMother::stdio(&name).replacen("env_var = \"EXAMPLE_WORKSPACE\"\n", "", 1);
    let plain = parsed(&plain);
    assert_eq!(input(&plain, "workspace").env_var, None);
    assert_eq!(plain.issues(), []);
}

#[test]
fn every_input_name_and_variable_the_rules_refuse_is_said_once() {
    let name = random::kebab();
    for (why, input) in crate::mother::InputNameMother::unwritable() {
        let text = McpMother::stdio(&name)
            .replacen(
                "name = \"api_key\"",
                &format!("name = {}", toml::Value::String(input.clone())),
                1,
            )
            .replacen(
                "\"{api_key}\"",
                &toml::Value::String(format!("{{{input}}}")).to_string(),
                1,
            );

        let issues = parsed(&text).issues();

        let said: Vec<&ValidationIssue> = issues
            .iter()
            .filter(|issue| matches!(issue, ValidationIssue::InputNameMalformed { .. }))
            .collect();
        assert_eq!(
            said,
            [&ValidationIssue::InputNameMalformed {
                name: input.clone()
            }],
            "{why}: {issues:?}"
        );
    }
    for (why, variable) in crate::mother::VariableNameMother::unportable() {
        let text = McpMother::stdio(&name).replacen(
            "\"EXAMPLE_API_KEY\"",
            &toml::Value::String(variable.clone()).to_string(),
            1,
        );

        assert_eq!(
            parsed(&text).issues(),
            [ValidationIssue::EnvVarMalformed {
                name: String::from("api_key"),
                variable: variable.clone(),
            }],
            "{why}"
        );
    }
}

#[test]
fn a_token_written_as_it_is_in_any_value_of_a_transport_is_said_where_it_is_and_never_whole() {
    let name = random::kebab();
    for (prefix, token) in McpMother::tokens() {
        for place in ["args", "env", "headers", "url", "userinfo"] {
            let (text, at) = McpMother::with_literal(&name, place, &token);

            let issues = parsed(&text).issues();

            assert_eq!(
                issues,
                [ValidationIssue::SecretLiteral {
                    at: at.clone(),
                    prefix: prefix.to_owned(),
                }],
                "{place}: {text}"
            );
            let issue = &issues[0];
            assert_eq!(issue.code(), "secret_literal");
            assert!(!issue.is_warning());
            // What it starts with, and none of the rest, in the message or
            // in what a front end is handed.
            let rest = token[prefix.len()..].trim_start();
            let said = issue.to_string();
            let sent = serde_json::to_string(issue).unwrap();
            assert!(said.contains(&format!("`{at}`")), "{said}");
            assert!(
                !said.contains(rest) && !sent.contains(rest),
                "{said}\n{sent}"
            );
        }
    }
}

#[test]
fn what_starts_as_a_token_does_and_is_none_is_not_said_to_be_one() {
    let name = random::kebab();
    for (why, text) in McpMother::not_tokens() {
        for place in ["args", "env", "headers"] {
            let (definition, _) = McpMother::with_literal(&name, place, &text);

            assert_eq!(
                parsed(&definition).issues(),
                [],
                "{why} in {place}: {definition}"
            );
        }
    }
}

#[test]
fn a_key_that_is_no_bare_key_is_quoted_where_a_token_is_said_to_be() {
    let (prefix, token) = McpMother::token();
    let text = McpMother::stdio(&random::kebab()).replacen(
        "LOG_LEVEL = \"info\"",
        &format!(
            "LOG_LEVEL = \"info\", \"service.key\" = {}",
            toml::Value::String(token)
        ),
        1,
    );

    assert_eq!(
        parsed(&text).issues(),
        [ValidationIssue::SecretLiteral {
            at: String::from("transport.env.\"service.key\""),
            prefix: prefix.to_owned(),
        }]
    );
}

#[test]
fn a_brace_naming_no_input_is_left_as_it_is_written() {
    let text = McpMother::stdio(&random::kebab()).replacen(
        "\"-y\",",
        "\"-y\", \"{\\\"debug\\\":true}\", \"{nobody}\",",
        1,
    );

    let definition = parsed(&text);

    let Transport::Stdio(stdio) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert!(stdio.args.contains(&String::from("{\"debug\":true}")));
    assert!(stdio.args.contains(&String::from("{nobody}")));
    assert_eq!(definition.issues(), []);
}

#[test]
fn a_description_with_nothing_in_it_is_said_to_be_empty() {
    for blank in ["", " ", "\n\t"] {
        let text = format!(
            "{}\n[transport]\ntype = \"sse\"\nurl = \"https://example.com\"\n",
            McpMother::head(&random::kebab(), blank)
        );

        assert_eq!(
            parsed(&text).issues(),
            [ValidationIssue::DescriptionEmpty],
            "{blank:?}"
        );
    }
}

#[test]
fn a_credential_typed_into_an_address_is_not_kept() {
    let text = McpMother::sse(&random::kebab()).replacen(
        "https://example.com/sse",
        "https://me:hunter2@example.com/sse",
        1,
    );

    let definition = parsed(&text);

    let Transport::Sse(endpoint) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert!(!endpoint.url.contains("hunter2"), "{}", endpoint.url);
    assert_eq!(endpoint.url, "https://example.com/sse");
}

#[test]
fn extras_are_kept_as_written_for_the_agent_they_are_keyed_by() {
    let definition = parsed(&McpMother::stdio(&random::kebab()));

    let claude = definition
        .extras_for(Agent::Claude)
        .expect("Claude's extras");
    assert_eq!(claude.get("timeout"), Some(&toml::Value::Integer(30000)));
    assert_eq!(definition.extras_for(Agent::Codex), None);

    let later = format!(
        "{}\n[extras.someday]\nkey = \"value\"\n",
        McpMother::sse(&random::kebab())
    );
    assert!(parsed(&later).extras.contains_key("someday"));
}

#[test]
fn each_input_kind_is_written_as_the_format_spells_it() {
    for (kind, slug) in [
        (InputKind::Env, "env"),
        (InputKind::Header, "header"),
        (InputKind::Arg, "arg"),
        (InputKind::UrlVar, "url_var"),
    ] {
        assert_eq!(kind.slug(), slug);
        let read: InputKind = toml::Value::String(slug.to_owned()).try_into().unwrap();
        assert_eq!(read, kind);
    }
}
