use super::*;
use crate::mother::{random, McpMother, SkillMother};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// The entry Claude is given for `text`'s definition.
fn entry_of(text: &str) -> Entry {
    entry(&parsed(text)).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn codes(warnings: &[Warning]) -> Vec<&'static str> {
    warnings.iter().map(Warning::code).collect()
}

#[test]
fn a_program_is_its_type_command_args_and_env_each_input_a_reference_to_its_variable() {
    let name = McpMother::name();
    let entry = entry_of(&McpMother::stdio(&name));

    assert_eq!(entry.name, name.as_str());
    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"args":["-y","@example/server","--workspace","${EXAMPLE_WORKSPACE}"],"#,
            r#""command":"npx","env":{"API_KEY":"${EXAMPLE_API_KEY}","LOG_LEVEL":"info"},"#,
            r#""timeout":30000,"type":"stdio"}"#
        )
    );
    // Written in the order a person reads it: what it is, then how it starts.
    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    let keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["type", "command", "args", "env", "timeout"]);
    // Its folder is what Claude's entry has no key for.
    assert_eq!(codes(&entry.warnings), ["cwd_unsupported"]);
    assert_eq!(
        entry.warnings[0].to_string(),
        format!(
            "`{name}`: the transport's `cwd` is left out, since a program is started there in no folder it is told"
        )
    );
}

#[test]
fn a_secret_argument_is_a_reference_claude_expands() {
    let entry = entry_of(&McpMother::secret_argument(&random::kebab()));
    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    assert_eq!(
        members[2],
        (
            String::from("args"),
            Value::array([
                "-y",
                "@example/server",
                "--workspace",
                "${EXAMPLE_WORKSPACE}"
            ])
        )
    );
}

#[test]
fn an_address_is_its_type_url_and_headers_a_secret_header_inside_a_longer_value() {
    let entry = entry_of(&McpMother::http(&random::kebab()));

    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"headers":{"Authorization":"Bearer ${EXAMPLE_TOKEN}"},"#,
            r#""type":"http","url":"https://${EXAMPLE_TENANT}.example.com/mcp"}"#
        )
    );
    assert_eq!(entry.warnings, []);
}

#[test]
fn server_sent_events_are_written_with_a_warning_that_they_are_deprecated() {
    let name = McpMother::name();
    let entry = entry_of(&McpMother::sse(&name));

    assert_eq!(
        entry.canonical(),
        r#"{"headers":{"X-Api-Key":"${EXAMPLE_KEY}"},"type":"sse","url":"https://example.com/sse"}"#
    );
    assert_eq!(codes(&entry.warnings), ["claude_mcp_sse_deprecated"]);
    assert_eq!(
        entry.warnings[0].to_string(),
        format!(
            "`{name}` is reached over sse, which Claude Code calls deprecated: reach it over streamable HTTP where the server offers that"
        )
    );
}

#[test]
fn every_key_is_written_though_it_holds_nothing() {
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

    assert_eq!(
        entry_of(&bare).canonical(),
        r#"{"args":[],"command":"npx","env":{},"type":"stdio"}"#
    );
}

#[test]
fn a_brace_naming_no_input_is_written_as_it_is_and_a_reference_never_read_again() {
    let a = InputName::from("a");
    let b = InputName::from("B");
    let (to_b, to_c) = (VariableName::from("B"), VariableName::from("C"));
    let read = [(&a, &to_b), (&b, &to_c)];

    for (text, written) in [
        ("{a}", "${B}"),
        ("{a}{B}", "${B}${C}"),
        ("--x={a}-{B}.", "--x=${B}-${C}."),
        (r#"{"debug":true}"#, r#"{"debug":true}"#),
        ("{unknown} {a}", "{unknown} ${B}"),
        ("{{a}}", "{${B}}"),
        ("{a", "{a"),
        ("}{", "}{"),
        ("$HOME/{a}", "$HOME/${B}"),
    ] {
        assert_eq!(referring(text, &read), written, "{text}");
    }
}

#[test]
fn extras_for_claude_are_passed_on_beside_the_entry_a_date_as_its_text() {
    let text = McpMother::with_claude_extras(
        &random::kebab(),
        "timeout = 30000\nratio = 0.5\nflag = true\nsince = 2026-11-05\n\
         nested = { list = [1, \"two\"] }\n",
    );
    let entry = entry_of(&text);

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
            (String::from("timeout"), Value::Integer(30000)),
        ]
    );
}

#[test]
fn an_extra_holding_a_key_the_entry_writes_is_refused() {
    let name = McpMother::name();
    for key in ["type", "command", "args", "env"] {
        let text = McpMother::with_claude_extras(&name, &format!("{key} = \"other\"\n"));
        let error = entry(&parsed(&text)).unwrap_err();
        assert_eq!(error.code(), "claude_mcp_extra_repeated", "{key}");
        assert_eq!(
            error.to_string(),
            format!(
                "`{name}` cannot be given to Claude Code: its extras for Claude give `{key}`, which the entry writes from its transport"
            )
        );
    }
}

#[test]
fn an_input_read_from_no_variable_is_refused() {
    let name = McpMother::name();
    let error = entry(&parsed(&McpMother::unread_input(&name))).unwrap_err();

    assert_eq!(error.code(), "claude_mcp_input_unread");
    assert_eq!(
        error.to_string(),
        format!(
            "`{name}` cannot be given to Claude Code: input `workspace` is read from no environment variable, and its entry could be given it only as its value"
        )
    );
}

#[test]
fn an_address_read_from_a_variable_claude_empties_is_refused_by_its_shape() {
    let name = McpMother::name();
    let (_, variable) = random::pick(&McpMother::blanked_variables());
    let error = entry(&parsed(&McpMother::header_read_from(&name, &variable))).unwrap_err();

    assert_eq!(error.code(), "variable_blanked");
    assert!(
        matches!(&error, ClaudeMcpError::Inexpressible { shape: Inexpressible::VariableBlanked { variable: blanked, .. }, .. } if *blanked == variable.as_str()),
        "{error}"
    );
    assert!(
        error.to_string().starts_with(&format!(
            "`{name}` cannot be given to Claude Code: input `token`"
        )),
        "{error}"
    );
}

#[test]
fn a_definition_validate_calls_invalid_is_refused_without_its_token() {
    let name = McpMother::name();
    for place in ["args", "env", "headers", "extras"] {
        let (prefix, token) = McpMother::token();
        let (text, _) = McpMother::with_literal(&name, place, &token);
        let error = entry(&parsed(&text)).unwrap_err();

        assert_eq!(error.code(), "claude_mcp_unfit", "{place}");
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
    // A name no agent keys a server by is the definition's to fix too.
    let (_, unkeyed) = random::pick(&McpMother::unkeyed());
    let text = McpMother::create(&unkeyed, &SkillMother::description(), "http");
    assert_eq!(
        entry(&parsed(&text)).unwrap_err().code(),
        "claude_mcp_unfit"
    );
}

#[test]
fn a_document_holds_only_a_server_when_nothing_else_is_written_beside_it() {
    let server = || Value::object([("type", Value::from("stdio"))]);
    let servers = |members: Vec<(&str, Value)>| Value::object([(SERVERS, Value::object(members))]);

    assert!(holds_only(&Value::object::<&str>([]), "a"));
    assert!(holds_only(&servers(vec![]), "a"));
    assert!(holds_only(&servers(vec![("a", server())]), "a"));

    assert!(!holds_only(&servers(vec![("b", server())]), "a"));
    assert!(!holds_only(
        &servers(vec![("a", server()), ("b", server())]),
        "a"
    ));
    assert!(!holds_only(
        &Value::object([
            (SERVERS, Value::object::<&str>([])),
            ("other", Value::Bool(true))
        ]),
        "a"
    ));
    assert!(!holds_only(&Value::object([(SERVERS, Value::Null)]), "a"));
    assert!(!holds_only(&Value::Array(Vec::new()), "a"));
}
