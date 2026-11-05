use super::*;
use crate::mcp::Inexpressible;
use crate::mother::{random, McpMother, SkillMother};

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// The entry Codex is given for `text`'s definition.
fn entry_of(text: &str) -> Entry {
    entry(&parsed(text)).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// The keys of `entry`, in the order written.
fn keys_of(entry: &Entry) -> Vec<&str> {
    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    members.iter().map(|(key, _)| key.as_str()).collect()
}

#[test]
fn a_program_is_its_command_args_env_and_folder_its_secret_passed_on_under_its_own_name() {
    let name = McpMother::name();
    let entry = entry_of(&McpMother::program_for_codex(&name));

    assert_eq!(entry.name, name.as_str());
    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"args":["-y","@example/server","--workspace","."],"command":"npx","cwd":".","#,
            r#""env":{"LOG_LEVEL":"info"},"env_vars":["EXAMPLE_API_KEY"],"#,
            r#""startup_timeout_sec":20}"#
        )
    );
    // Written in the order a person reads it, Codex's extras after.
    assert_eq!(
        keys_of(&entry),
        [
            "command",
            "args",
            "env",
            "env_vars",
            "cwd",
            "startup_timeout_sec"
        ]
    );
    assert_eq!(entry.warnings, []);
}

#[test]
fn an_address_is_its_url_and_headers_a_bearer_token_read_from_its_variable() {
    let entry = entry_of(&McpMother::address_for_codex(&random::kebab()));

    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"bearer_token_env_var":"EXAMPLE_TOKEN","http_headers":{"X-Client":"mindweaver"},"#,
            r#""url":"https://example.com/mcp"}"#
        )
    );
    assert_eq!(
        keys_of(&entry),
        ["url", "http_headers", "bearer_token_env_var"]
    );
    assert_eq!(entry.warnings, []);
}

#[test]
fn a_secret_header_that_is_its_whole_value_is_read_from_its_variable() {
    let entry = entry_of(&McpMother::whole_header_for_codex(&random::kebab()));

    assert_eq!(
        entry.canonical(),
        concat!(
            r#"{"env_http_headers":{"X-Api-Key":"EXAMPLE_TOKEN"},"#,
            r#""http_headers":{"X-Client":"mindweaver"},"url":"https://example.com/mcp"}"#
        )
    );
    assert_eq!(keys_of(&entry), ["url", "http_headers", "env_http_headers"]);
}

#[test]
fn a_key_that_would_hold_nothing_is_not_written() {
    let bare = McpMother::program_for_codex(&random::kebab())
        .replacen(
            "args = [\"-y\", \"@example/server\", \"--workspace\", \".\"]\n",
            "",
            1,
        )
        .replacen("cwd = \".\"\n", "", 1)
        .replacen(", LOG_LEVEL = \"info\"", "", 1)
        .replacen("startup_timeout_sec = 20\n", "", 1);

    assert_eq!(
        entry_of(&bare).canonical(),
        r#"{"command":"npx","env_vars":["EXAMPLE_API_KEY"]}"#
    );
}

#[test]
fn text_is_written_as_it_is_since_codex_reads_no_reference_in_it() {
    let text = McpMother::program_for_codex(&random::kebab())
        .replacen(
            "\".\"]",
            "\".\", \"--home=${HOME}/x\", \"{\\\"debug\\\":true}\"]",
            1,
        )
        .replacen("LOG_LEVEL = \"info\"", "LOG_LEVEL = \"${LEVEL:-info}\"", 1);
    let entry = entry_of(&text);

    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    assert_eq!(
        members[1].1,
        Value::array([
            "-y",
            "@example/server",
            "--workspace",
            ".",
            "--home=${HOME}/x",
            r#"{"debug":true}"#
        ])
    );
    assert_eq!(
        members[2].1,
        Value::object([("LOG_LEVEL", Value::from("${LEVEL:-info}"))])
    );
}

#[test]
fn each_shape_codex_cannot_be_given_refuses_the_entry_by_its_code_and_reason() {
    let name = McpMother::name();
    for (text, code, reason) in [
        (
            McpMother::stdio(&name),
            "secret_env_renamed",
            "secret input `api_key` is written as environment variable `API_KEY`, and a program \
             is given a variable there only whole, under its own name, `EXAMPLE_API_KEY`",
        ),
        (
            McpMother::composite_secret_env(&name),
            "secret_env_composite",
            "secret input `api_key` is written in environment variable `EXAMPLE_API_KEY` beside \
             other text, and a program is given a variable there only whole, under its own name",
        ),
        (
            McpMother::secret_argument(&name),
            "secret_in_argument",
            "secret input `workspace` is written in the transport's `args`, which hold no \
             reference to an environment variable there",
        ),
        (
            McpMother::secret_in_url(&name),
            "secret_in_url",
            "secret input `tenant` is written in the transport's `url`, which holds no reference \
             to an environment variable there",
        ),
        (
            McpMother::composite_secret_header(&name),
            "secret_header_composite",
            "secret input `token` is written in header `Authorization` beside other text, and a \
             header is read from an environment variable there only as its whole value, or as \
             `Authorization: Bearer` and the token",
        ),
        (
            McpMother::sse(&name),
            "sse_unsupported",
            "the sse transport, server-sent events, is not one taken there",
        ),
        (
            McpMother::with_codex_extras(&name, "env_vars = [\"OTHER\"]\n"),
            "extra_repeated",
            "its extras for the agent give `env_vars`, a key the entry there is written with \
             from the transport, so one of the two would be lost",
        ),
        (
            McpMother::with_codex_extras(&name, "experimental_environment = \"remote\"\n"),
            "extra_withheld",
            "its extras for the agent give `experimental_environment`, a key for the environment \
             it runs a server in, which is never written there",
        ),
        (
            McpMother::with_codex_extras(&name, "environment_id = \"cloud\"\n"),
            "extra_withheld",
            "its extras for the agent give `environment_id`, a key for the environment it runs \
             a server in, which is never written there",
        ),
    ] {
        let error = entry(&parsed(&text)).unwrap_err();
        assert_eq!(error.code(), code, "{error}");
        assert!(
            matches!(error, CodexMcpError::Inexpressible { .. }),
            "{error}"
        );
        assert_eq!(
            error.to_string(),
            format!("`{name}` cannot be given to Codex: {reason}"),
            "{code}"
        );
    }
}

#[test]
fn a_second_authorization_header_is_no_bearer_token_codex_reads() {
    let name = McpMother::name();
    let twice = McpMother::address_for_codex(&name)
        .replacen(
            "X-Client = \"mindweaver\"",
            "authorization = \"Bearer {other}\"",
            1,
        )
        .replacen(
            "[provenance]",
            "[[inputs]]\nname = \"other\"\nkind = \"header\"\nsecret = true\n\
             env_var = \"EXAMPLE_OTHER\"\n\n[provenance]",
            1,
        );
    let definition = parsed(&twice);

    assert_eq!(definition.issues(), []);
    assert_eq!(
        definition.inexpressible(Agent::Codex),
        [Inexpressible::SecretHeaderComposite {
            header: String::from("authorization"),
            input: InputName::from("other"),
        }]
    );
    assert_eq!(
        entry(&definition).unwrap_err().code(),
        "secret_header_composite"
    );
}

#[test]
fn bearer_is_the_first_authorization_header_holding_the_scheme_and_the_input_alone() {
    let headers = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    };

    for (header, value) in [
        ("Authorization", "Bearer {token}"),
        ("authorization", "bearer {token}"),
        ("AUTHORIZATION", "BEARER {token}"),
    ] {
        assert!(
            bearer(&headers(&[(header, value)]), header, "{token}"),
            "{header}: {value}"
        );
    }
    for (header, value) in [
        ("Authorization", "Token {token}"),
        ("Authorization", "Bearer  {token}"),
        ("Authorization", "Bearer {token}x"),
        ("Authorization", "{token}"),
        ("X-Authorization", "Bearer {token}"),
    ] {
        assert!(
            !bearer(&headers(&[(header, value)]), header, "{token}"),
            "{header}: {value}"
        );
    }
    let two = headers(&[
        ("Authorization", "Bearer {token}"),
        ("authorization", "Bearer {token}"),
    ]);
    assert!(bearer(&two, "Authorization", "{token}"));
    assert!(!bearer(&two, "authorization", "{token}"));
    assert!(!bearer(&two, "X-Other", "{token}"));
}

#[test]
fn an_input_written_where_codex_reads_no_variable_is_refused_though_no_agent_s_report_says_it() {
    let name = McpMother::name();
    let open_env = McpMother::program_for_codex(&name)
        .replacen("secret = true\n", "", 1)
        .replacen(
            "EXAMPLE_API_KEY = \"{api_key}\"",
            "API_KEY = \"{api_key}\"",
            1,
        );
    let open_header = McpMother::address_for_codex(&name)
        .replacen("secret = true\n", "", 1)
        .replacen("\"Bearer {token}\"", "\"Token {token}\"", 1);
    for (text, input, at) in [
        (
            McpMother::whole_secret_env(&name),
            "workspace",
            "transport.args[3]",
        ),
        (McpMother::http(&name), "tenant", "transport.url"),
        (open_env, "api_key", "transport.env.API_KEY"),
        (open_header, "token", "transport.headers.Authorization"),
    ] {
        let definition = parsed(&text);
        let error = entry(&definition).unwrap_err();

        // Codex could be given the value, once an install hands it one.
        assert_eq!(definition.inexpressible(Agent::Codex), [], "{text}");
        assert_eq!(error.code(), "codex_mcp_input_in_text", "{error}");
        assert_eq!(
            error.to_string(),
            format!(
                "`{name}` cannot be given to Codex: input `{input}` is written in `{at}`, where \
                 Codex reads no environment variable into it, so its entry could be given it \
                 there only as its value"
            )
        );
    }
}

#[test]
fn an_input_read_from_no_variable_is_refused_as_claude_s_entry_refuses_it() {
    let name = McpMother::name();
    let error = entry(&parsed(&McpMother::unread_input(&name))).unwrap_err();

    assert_eq!(error.code(), "codex_mcp_input_unread");
    assert_eq!(
        error.to_string(),
        format!(
            "`{name}` cannot be given to Codex: input `workspace` is read from no environment \
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
        let error = entry(&parsed(&text)).unwrap_err();

        assert_eq!(error.code(), "codex_mcp_unfit", "{place}");
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
    assert_eq!(entry(&parsed(&text)).unwrap_err().code(), "codex_mcp_unfit");
}

#[test]
fn extras_for_codex_are_passed_on_beside_the_entry_a_date_as_its_text() {
    let text = McpMother::with_codex_extras(
        &random::kebab(),
        "startup_timeout_sec = 20\ntool_timeout_sec = 0.5\nenabled = true\n\
         since = 2026-11-05\nenabled_tools = [\"search\", \"fetch\"]\n",
    );
    let entry = entry_of(&text);

    let Value::Object(members) = &entry.value else {
        panic!("{:?}", entry.value)
    };
    assert_eq!(
        members[5..].to_vec(),
        [
            (String::from("enabled"), Value::Bool(true)),
            (
                String::from("enabled_tools"),
                Value::array(["search", "fetch"])
            ),
            (String::from("since"), Value::from("2026-11-05")),
            (String::from("startup_timeout_sec"), Value::Integer(20)),
            (String::from("tool_timeout_sec"), Value::Float(0.5)),
        ]
    );
}

#[test]
fn an_extra_holding_a_key_the_entry_may_write_is_refused_by_its_shape() {
    let name = McpMother::name();
    let programs = keys(TransportType::Stdio).iter().map(|key| {
        (
            McpMother::with_codex_extras(&name, &format!("{key} = \"other\"\n")),
            *key,
        )
    });
    let addresses = keys(TransportType::Http).iter().map(|key| {
        (
            format!(
                "{}\n[extras.codex]\n{key} = \"other\"\n",
                McpMother::address_for_codex(&name)
            ),
            *key,
        )
    });
    for (text, key) in programs.chain(addresses) {
        let error = entry(&parsed(&text)).unwrap_err();
        assert!(
            matches!(&error, CodexMcpError::Inexpressible { shape: Inexpressible::ExtraRepeated { key: repeated }, .. } if repeated.as_str() == key),
            "{key}: {error}"
        );
        assert_eq!(
            error.to_string(),
            format!(
                "`{name}` cannot be given to Codex: its extras for the agent give `{key}`, a key \
                 the entry there is written with from the transport, so one of the two would be \
                 lost"
            )
        );
    }
    // A key of a program's entry is none of an address's, which takes it
    // beside its own.
    let text = format!(
        "{}\n[extras.codex]\ncommand = \"other\"\n",
        McpMother::address_for_codex(&name)
    );
    assert_eq!(parsed(&text).inexpressible(Agent::Codex), []);
}

#[test]
fn the_keys_an_entry_is_written_with_are_the_ones_its_extras_are_held_to() {
    let name = McpMother::name();
    for (text, kind) in [
        (McpMother::program_for_codex(&name), TransportType::Stdio),
        (McpMother::address_for_codex(&name), TransportType::Http),
        (
            McpMother::whole_header_for_codex(&name),
            TransportType::Http,
        ),
    ] {
        let entry = entry_of(&text);
        let extras = parsed(&text)
            .extras_for(Agent::Codex)
            .map_or(0, |table| table.len());
        let written = keys_of(&entry);
        let transport = &written[..written.len() - extras];
        assert!(
            transport.iter().all(|key| keys(kind).contains(key)),
            "{kind:?}: {written:?}"
        );
        // In the order `keys` gives.
        let order: Vec<usize> = transport
            .iter()
            .map(|key| keys(kind).iter().position(|known| known == key).unwrap())
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    }
}
