use super::*;
use crate::mother::{random, McpMother, SkillMother};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// The entry OpenCode is given for `text`'s definition in `shape`.
fn entry_of(text: &str, shape: Shape) -> Entry {
    entry(&parsed(text), shape).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// The keys of `entry`, in the order written.
fn keys_of(entry: &Entry) -> Vec<&str> {
    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    members.iter().map(|(key, _)| key.as_str()).collect()
}

fn codes(warnings: &[Warning]) -> Vec<&'static str> {
    warnings.iter().map(Warning::code).collect()
}

#[test]
fn a_program_is_its_type_command_environment_and_folder_each_input_a_reference_to_its_variable() {
    let name = McpMother::name();
    let entry = entry_of(&McpMother::stdio(&name), Shape::V2);

    assert_eq!(entry.name, name.as_str());
    assert_eq!(entry.shape, Shape::V2);
    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"command":["npx","-y","@example/server","--workspace","{env:EXAMPLE_WORKSPACE}"],"#,
            r#""cwd":".","environment":{"API_KEY":"{env:EXAMPLE_API_KEY}","LOG_LEVEL":"info"},"#,
            r#""type":"local"}"#
        )
    );
    // Written in the order a person reads it: what it is, then how it starts.
    assert_eq!(keys_of(&entry), ["type", "command", "environment", "cwd"]);
    assert_eq!(entry.warnings, []);
}

#[test]
fn a_secret_argument_is_a_reference_opencode_fills_in_the_command() {
    let entry = entry_of(&McpMother::secret_argument(&random::kebab()), Shape::V2);
    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    assert_eq!(
        members[1],
        (
            String::from("command"),
            Value::array([
                "npx",
                "-y",
                "@example/server",
                "--workspace",
                "{env:EXAMPLE_WORKSPACE}"
            ])
        )
    );
}

#[test]
fn an_address_is_its_type_url_and_headers_a_secret_header_inside_a_longer_value() {
    let entry = entry_of(&McpMother::http(&random::kebab()), Shape::V2);

    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"headers":{"Authorization":"Bearer {env:EXAMPLE_TOKEN}"},"#,
            r#""type":"remote","url":"https://{env:EXAMPLE_TENANT}.example.com/mcp"}"#
        )
    );
    assert_eq!(keys_of(&entry), ["type", "url", "headers"]);
    assert_eq!(entry.warnings, []);
}

#[test]
fn in_v1_s_shape_a_program_is_enabled_and_written_without_its_folder() {
    let name = McpMother::name();
    let entry = entry_of(&McpMother::stdio(&name), Shape::V1);

    assert_eq!(entry.shape, Shape::V1);
    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"command":["npx","-y","@example/server","--workspace","{env:EXAMPLE_WORKSPACE}"],"#,
            r#""enabled":true,"environment":{"API_KEY":"{env:EXAMPLE_API_KEY}","LOG_LEVEL":"info"},"#,
            r#""type":"local"}"#
        )
    );
    assert_eq!(
        keys_of(&entry),
        ["type", "command", "enabled", "environment"]
    );
    assert_eq!(codes(&entry.warnings), ["opencode_mcp_v1_cwd"]);
    assert_eq!(
        entry.warnings[0].to_string(),
        format!(
            "`{name}`: the transport's `cwd` is left out, since OpenCode's V1 shape, which the \
             file's servers are written in, has no key for it"
        )
    );

    // With no folder to start in, nothing is left out.
    let entry = entry_of(&McpMother::without_cwd(&name), Shape::V1);
    assert_eq!(entry.warnings, []);
}

#[test]
fn in_v1_s_shape_an_address_is_enabled() {
    let entry = entry_of(&McpMother::http(&random::kebab()), Shape::V1);

    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"enabled":true,"headers":{"Authorization":"Bearer {env:EXAMPLE_TOKEN}"},"#,
            r#""type":"remote","url":"https://{env:EXAMPLE_TENANT}.example.com/mcp"}"#
        )
    );
    assert_eq!(keys_of(&entry), ["type", "url", "enabled", "headers"]);
}

#[test]
fn a_key_that_would_hold_nothing_is_not_written() {
    let name = random::kebab();
    let bare = McpMother::without_cwd(&name)
        .replacen(
            "args = [\"-y\", \"@example/server\", \"--workspace\", \"{workspace}\"]\n",
            "",
            1,
        )
        .replacen(
            "env = { API_KEY = \"{api_key}\", LOG_LEVEL = \"info\" }\n",
            "",
            1,
        );
    let bare = bare[..bare.find("\n[[inputs]]").unwrap()].to_owned();
    let address = McpMother::plain_address(&name).replacen(
        "headers = { X-Client = \"mindweaver\" }\n",
        "",
        1,
    );

    for shape in [Shape::V2, Shape::V1] {
        let enabled = if shape == Shape::V1 {
            r#""enabled":true,"#
        } else {
            ""
        };
        assert_eq!(
            entry_of(&bare, shape).canonical(),
            format!(r#"{{"command":["npx"],{enabled}"type":"local"}}"#),
            "{shape:?}"
        );
        assert_eq!(
            entry_of(&address, shape).canonical(),
            format!(r#"{{{enabled}"type":"remote","url":"https://example.com/mcp"}}"#),
            "{shape:?}"
        );
    }
}

#[test]
fn a_brace_naming_no_input_is_written_as_it_is_and_a_reference_never_read_again() {
    let a = InputName::from("a");
    let b = InputName::from("B");
    let (to_b, to_c) = (VariableName::from("B"), VariableName::from("C"));
    let read = [(&a, &to_b), (&b, &to_c)];

    for (text, written) in [
        ("{a}", "{env:B}"),
        ("{a}{B}", "{env:B}{env:C}"),
        ("--x={a}-{B}.", "--x={env:B}-{env:C}."),
        (r#"{"debug":true}"#, r#"{"debug":true}"#),
        ("{unknown} {a}", "{unknown} {env:B}"),
        ("{{a}}", "{{env:B}}"),
        ("{a", "{a"),
        ("}{", "}{"),
        ("${HOME}/{a}", "${HOME}/{env:B}"),
    ] {
        assert_eq!(referring(text, &read), written, "{text}");
    }
}

#[test]
fn each_shape_opencode_cannot_be_given_refuses_the_entry_by_its_code_and_reason() {
    let name = McpMother::name();
    let (args, at) = McpMother::with_text(&name, "args", "{env:HOME}");
    for (text, code, reason) in [
        (
            McpMother::sse(&name),
            "sse_unsupported",
            String::from("the sse transport, server-sent events, is not one taken there"),
        ),
        (
            args,
            "text_substituted",
            format!(
                "`{at}` holds `{{env:` as it is, which is filled in there over the file's text \
                 before it is read, so the text written in it cannot be given"
            ),
        ),
        (
            McpMother::with_opencode_extras(&name, "environment = { A = \"b\" }\n"),
            "extra_repeated",
            String::from(
                "its extras for the agent give `environment`, a key the entry there is written \
                 with from the transport, so one of the two would be lost",
            ),
        ),
    ] {
        for shape in [Shape::V2, Shape::V1] {
            let error = entry(&parsed(&text), shape).unwrap_err();
            assert_eq!(error.code(), code, "{error}");
            assert!(
                matches!(error, OpenCodeMcpError::Inexpressible { .. }),
                "{error}"
            );
            assert_eq!(
                error.to_string(),
                format!("`{name}` cannot be given to OpenCode: {reason}"),
                "{code}"
            );
        }
    }
}

#[test]
fn extras_for_opencode_are_passed_on_beside_the_entry_a_date_as_its_text() {
    let text = McpMother::with_opencode_extras(
        &random::kebab(),
        "timeout = 5000\nratio = 0.5\nflag = true\nsince = 2026-11-05\n\
         nested = { list = [1, \"two\"] }\n",
    );
    let entry = entry_of(&text, Shape::V2);

    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    assert_eq!(
        members[4..].to_vec(),
        [
            (String::from("flag"), Value::Bool(true)),
            (
                String::from("nested"),
                Value::object([(
                    "list",
                    Value::Array(vec![Value::Integer(1), Value::from("two")])
                )])
            ),
            (String::from("ratio"), Value::Float(0.5)),
            (String::from("since"), Value::from("2026-11-05")),
            (String::from("timeout"), Value::Integer(5000)),
        ]
    );
}

#[test]
fn an_extra_holding_a_key_the_entry_may_write_in_either_shape_is_refused_by_its_shape() {
    let name = McpMother::name();
    let programs = keys(TransportType::Stdio).iter().map(|key| {
        (
            McpMother::with_opencode_extras(&name, &format!("{key} = \"other\"\n")),
            *key,
        )
    });
    let addresses = keys(TransportType::Http).iter().map(|key| {
        (
            format!(
                "{}\n[extras.opencode]\n{key} = \"other\"\n",
                McpMother::http(&name)
            ),
            *key,
        )
    });
    for (text, key) in programs.chain(addresses) {
        for shape in [Shape::V2, Shape::V1] {
            let error = entry(&parsed(&text), shape).unwrap_err();
            assert!(
                matches!(&error, OpenCodeMcpError::Inexpressible { shape: Inexpressible::ExtraRepeated { key: repeated }, .. } if repeated.as_str() == key),
                "{key}: {error}"
            );
        }
    }
    // A key of a program's entry is none of an address's, which takes it
    // beside its own.
    let text = format!(
        "{}\n[extras.opencode]\ncommand = \"other\"\n",
        McpMother::http(&name)
    );
    assert_eq!(parsed(&text).inexpressible(Agent::OpenCode), []);
}

#[test]
fn the_keys_an_entry_is_written_with_are_the_ones_its_extras_are_held_to() {
    let name = McpMother::name();
    for (text, kind) in [
        (McpMother::stdio(&name), TransportType::Stdio),
        (McpMother::http(&name), TransportType::Http),
    ] {
        for shape in [Shape::V2, Shape::V1] {
            let entry = entry_of(&text, shape);
            let written = keys_of(&entry);
            assert!(
                written.iter().all(|key| keys(kind).contains(key)),
                "{kind:?} {shape:?}: {written:?}"
            );
            // In the order `keys` gives.
            let order: Vec<usize> = written
                .iter()
                .map(|key| keys(kind).iter().position(|known| known == key).unwrap())
                .collect();
            assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        }
    }
}

#[test]
fn an_input_read_from_no_variable_is_refused_as_the_other_agents_entries_refuse_it() {
    let name = McpMother::name();
    let error = entry(&parsed(&McpMother::unread_input(&name)), Shape::V2).unwrap_err();

    assert_eq!(error.code(), "opencode_mcp_input_unread");
    assert_eq!(
        error.to_string(),
        format!(
            "`{name}` cannot be given to OpenCode: input `workspace` is read from no environment \
             variable, and its entry could be given it only as its value"
        )
    );
}

#[test]
fn a_definition_validate_calls_invalid_is_refused_without_its_token() {
    let name = McpMother::name();
    for place in ["args", "env", "headers", "extras"] {
        let (prefix, token) = McpMother::token();
        let (text, _) = McpMother::with_literal(&name, place, &token);
        let error = entry(&parsed(&text), Shape::V2).unwrap_err();

        assert_eq!(error.code(), "opencode_mcp_unfit", "{place}");
        let message = error.to_string();
        assert!(
            message.starts_with(&format!(
                "`{name}` is written for no agent while it is invalid: "
            )),
            "{message}"
        );
        assert!(message.contains(prefix), "{message}");
        assert!(!message.contains(&token), "{place}: {message}");
    }
    let (_, unkeyed) = random::pick(&McpMother::unkeyed());
    let text = McpMother::create(&unkeyed, &SkillMother::description(), "http");
    assert_eq!(
        entry(&parsed(&text), Shape::V2).unwrap_err().code(),
        "opencode_mcp_unfit"
    );
}

#[test]
fn a_server_s_key_is_under_servers_in_v2_s_shape_and_under_mcp_in_v1_s() {
    let name = McpMother::name();
    assert_eq!(Shape::V2.key(&name), ["mcp", "servers", name.as_str()]);
    assert_eq!(Shape::V1.key(&name), ["mcp", name.as_str()]);
}

#[test]
fn what_opencode_runs_is_read_off_its_one_line_in_either_shape() {
    let name = McpMother::name();
    for shape in [Shape::V2, Shape::V1] {
        let program = entry_of(&McpMother::stdio(&name), shape);
        let address = entry_of(&McpMother::http(&name), shape);

        assert_eq!(
            program.runs().to_string(),
            "npx -y @example/server --workspace {env:EXAMPLE_WORKSPACE}",
            "{shape:?}"
        );
        assert_eq!(
            address.runs().to_string(),
            "https://{env:EXAMPLE_TENANT}.example.com/mcp",
            "{shape:?}"
        );
    }
}
