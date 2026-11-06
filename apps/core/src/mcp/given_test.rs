//! Unit tests for `given.rs`: the definition `add` writes from what it is
//! told, read back as every definition is.

use super::*;
use crate::mcp::{InputKind, McpDefinition, Transport};
use crate::mother::{random, McpMother, SkillMother, VariableNameMother};
use crate::value::{InputName, VariableName};

/// `given`'s text for a server drawn afresh, read as a definition.
fn read(given: &Given) -> McpDefinition {
    let text = given.text(&McpMother::name(), &SkillMother::description());
    McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}:\n{text}"))
}

/// What `given` reads each of its inputs from, if it is a program.
fn program(given: &Given) -> &Program {
    match given {
        Given::Program(program) => program,
        Given::Address(_) => panic!("{given:?} is no program"),
    }
}

/// What `given` reads each of its inputs from, if it is an address.
fn address(given: &Given) -> &Address {
    match given {
        Given::Address(address) => address,
        Given::Program(_) => panic!("{given:?} is no address"),
    }
}

#[test]
fn a_program_is_written_as_a_definition_that_reads_back_as_it_was_given() {
    let given = McpMother::given_program();
    let told = program(&given);
    let secret = &told.secret_env[0];
    let argued = &told.arg_env[0];

    let definition = read(&given);

    let Transport::Stdio(stdio) = &definition.transport else {
        panic!("{:?}", definition.transport);
    };
    assert_eq!(stdio.command, "npx");
    assert_eq!(
        stdio.args,
        [
            "-y",
            "@example/server",
            "--workspace",
            &format!("{{{argued}}}")
        ]
    );
    assert_eq!(stdio.env.get("LOG_LEVEL").map(String::as_str), Some("info"));
    assert_eq!(
        stdio.env.get(secret.as_str()),
        Some(&format!("{{{secret}}}"))
    );
    assert_eq!(stdio.cwd, None);
    let inputs: Vec<(&str, InputKind, bool, Option<&str>)> = definition
        .inputs
        .iter()
        .map(|input| {
            (
                input.name.as_str(),
                input.kind,
                input.secret,
                input.env_var.as_ref().map(|variable| variable.as_str()),
            )
        })
        .collect();
    assert_eq!(
        inputs,
        [
            (secret.as_str(), InputKind::Env, true, Some(secret.as_str())),
            (
                argued.as_str(),
                InputKind::Arg,
                false,
                Some(argued.as_str())
            ),
        ]
    );
    assert_eq!(definition.issues(), []);
}

#[test]
fn an_address_is_written_as_a_definition_that_reads_back_as_it_was_given() {
    for (events, word) in [(false, "http"), (true, "sse")] {
        let given = McpMother::given_address(events);
        let told = address(&given);
        let (header, keyed) = &told.secret_headers[0];
        let bearer = told.bearer.as_ref().unwrap();
        let tenant = &told.url_vars[0].1;

        let definition = read(&given);

        assert_eq!(definition.transport.slug(), word);
        let (Transport::Http(endpoint) | Transport::Sse(endpoint)) = &definition.transport else {
            panic!("{:?}", definition.transport);
        };
        assert_eq!(endpoint.url, "https://{tenant}.example.com/mcp");
        assert_eq!(
            endpoint.headers.get("X-Client").map(String::as_str),
            Some("mindweaver")
        );
        assert_eq!(endpoint.headers.get(header), Some(&format!("{{{keyed}}}")));
        assert_eq!(
            endpoint.headers.get("Authorization"),
            Some(&format!("Bearer {{{bearer}}}"))
        );
        let inputs: Vec<(&str, InputKind, bool, Option<&str>)> = definition
            .inputs
            .iter()
            .map(|input| {
                (
                    input.name.as_str(),
                    input.kind,
                    input.secret,
                    input.env_var.as_ref().map(|variable| variable.as_str()),
                )
            })
            .collect();
        assert_eq!(
            inputs,
            [
                (
                    keyed.as_str(),
                    InputKind::Header,
                    true,
                    Some(keyed.as_str())
                ),
                (
                    bearer.as_str(),
                    InputKind::Header,
                    true,
                    Some(bearer.as_str())
                ),
                ("tenant", InputKind::UrlVar, false, Some(tenant.as_str())),
            ]
        );
        assert_eq!(definition.issues(), [], "{word}");
    }
}

#[test]
fn what_is_written_says_what_each_part_is_for() {
    let secret = VariableNameMother::random();
    let given = Given::Program(Program {
        command: String::from("npx"),
        args: vec![String::from("-y"), String::from("@acme/server")],
        env: Vec::new(),
        secret_env: vec![secret.clone()],
        arg_env: Vec::new(),
    });

    let text = given.text("acme", "Acme's tickets");

    assert_eq!(
        text,
        format!(
            "# An MCP server, as Mindweaver keeps it: each agent's own entry for it\n\
             # is written from this file, and `show` says how each agent is given it.\n\
             format = 1\n\
             name = \"acme\"\n\
             description = \"Acme's tickets\"\n\
             \n\
             # How an agent starts it: the program, what it is started with, and what\n\
             # its environment is given beside the agent's own.\n\
             [transport]\n\
             type = \"stdio\"\n\
             command = \"npx\"\n\
             args = [\"-y\", \"@acme/server\"]\n\
             env = {{ {secret} = \"{{{secret}}}\" }}\n\
             \n\
             # What is supplied when an agent starts it, each written between braces\n\
             # where it goes and read from the environment variable it names: no value\n\
             # is written here, and a secret never.\n\
             [[inputs]]\n\
             name = \"{secret}\"\n\
             kind = \"env\"\n\
             secret = true\n\
             env_var = \"{secret}\"\n"
        )
    );
}

#[test]
fn a_transport_given_nothing_beside_what_it_needs_writes_no_table_and_no_input() {
    let program = Given::Program(Program {
        command: String::from("acme-mcp"),
        ..Program::default()
    });
    let address = Given::Address(Address {
        url: String::from("https://example.com/mcp"),
        ..Address::default()
    });

    for given in [program, address] {
        let text = given.text("acme", "Acme's tickets");

        for key in ["\nargs = ", "\nenv = ", "\nheaders = ", "[[inputs]]"] {
            assert!(!text.contains(key), "{key}: {text}");
        }
        assert_eq!(read(&given).issues(), []);
    }
}

#[test]
fn every_value_reads_back_as_it_was_typed_whatever_it_holds() {
    let awkward = [
        String::from("a \"quoted\" word"),
        String::from("a back\\slash"),
        String::from("two\nlines"),
        String::from("{\"debug\":true}"),
        random::text(40),
    ];
    for value in awkward {
        let given = Given::Program(Program {
            command: value.clone(),
            args: vec![value.clone()],
            env: vec![(String::from("a key.with dots"), value.clone())],
            ..Program::default()
        });
        let description = format!("For {value}");

        let text = given.text("acme", &description);

        let definition = McpDefinition::parse(&text).unwrap_or_else(|e| panic!("{e}:\n{text}"));
        assert_eq!(definition.description, description);
        let Transport::Stdio(stdio) = &definition.transport else {
            panic!("{:?}", definition.transport);
        };
        assert_eq!(stdio.command, value);
        assert_eq!(stdio.args, std::slice::from_ref(&value));
        assert_eq!(stdio.env.get("a key.with dots"), Some(&value));
    }
}

#[test]
fn a_key_given_twice_is_found_by_its_table_and_one_given_once_is_not() {
    for (why, given, repeated) in McpMother::given_repeating() {
        assert_eq!(given.repeated(), Some(repeated), "{why}");
    }
    for given in [
        McpMother::given_program(),
        McpMother::given_address(false),
        McpMother::given_address(true),
    ] {
        assert_eq!(given.repeated(), None, "{given:?}");
    }
}

#[test]
fn a_header_given_in_another_case_is_another_key_as_the_file_holds_it() {
    let given = Given::Address(Address {
        url: String::from("https://example.com/mcp"),
        headers: vec![(String::from("authorization"), String::from("Basic x"))],
        bearer: Some(VariableNameMother::random()),
        ..Address::default()
    });

    assert_eq!(given.repeated(), None);
    assert!(McpDefinition::parse(&given.text("acme", "Acme")).is_ok());
}

#[test]
fn what_reaches_a_server_unseen_is_a_secret_and_what_is_shown_is_not() {
    for given in [McpMother::given_program(), McpMother::given_address(false)] {
        let definition = read(&given);

        for input in &definition.inputs {
            let secret = matches!(input.kind, InputKind::Env | InputKind::Header);
            assert_eq!(input.secret, secret, "{input:?}");
            assert!(input.env_var.is_some(), "{input:?}");
        }
    }
}

/// `given` as a definition reads it back: each table's keys in the order
/// a file keeps them, which a TOML table sorts.
fn as_read(given: &Given) -> Given {
    let mut given = given.clone();
    match &mut given {
        Given::Program(program) => program.env.sort(),
        Given::Address(address) => address.headers.sort(),
    }
    given
}

#[test]
fn a_definition_add_writes_reads_back_as_what_it_was_told() {
    for _ in 0..32 {
        let given = McpMother::given();

        let definition = read(&given);

        assert_eq!(Given::of(&definition), Some(as_read(&given)), "{given:?}");
    }
}

#[test]
fn a_definition_holding_what_no_option_says_reads_back_as_nothing() {
    // A secret under a name of its own, a folder to start in and an input
    // described and required: each is written away by the options.
    for text in [
        McpMother::stdio(&McpMother::name()),
        McpMother::http(&McpMother::name()),
    ] {
        let definition = McpDefinition::parse(&text).unwrap();

        assert_eq!(Given::of(&definition), None, "{text}");
    }
    // An argument read from a variable before one written as it is, which
    // `add` writes after every one of them.
    let text = format!(
        "format = 1\nname = \"{}\"\ndescription = \"A server\"\n\n\
         [transport]\ntype = \"stdio\"\ncommand = \"npx\"\nargs = [\"{{TOKEN}}\", \"-y\"]\n\n\
         [[inputs]]\nname = \"TOKEN\"\nkind = \"arg\"\nenv_var = \"TOKEN\"\n",
        McpMother::name()
    );
    assert_eq!(Given::of(&McpDefinition::parse(&text).unwrap()), None);
}

#[test]
fn a_given_goes_out_as_a_front_end_sends_it_and_reads_back_whole() {
    let given = Given::Address(Address {
        url: String::from("https://{tenant}.example.com/mcp"),
        events: false,
        headers: vec![(String::from("X-Client"), String::from("me"))],
        secret_headers: vec![(String::from("X-Api-Key"), VariableName::from("API_KEY"))],
        bearer: Some(VariableName::from("TOKEN")),
        url_vars: vec![(InputName::from("tenant"), VariableName::from("TENANT"))],
    });

    let json = serde_json::to_value(&given).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "type": "address",
            "url": "https://{tenant}.example.com/mcp",
            "events": false,
            "headers": [["X-Client", "me"]],
            "secret_headers": [["X-Api-Key", "API_KEY"]],
            "bearer": "TOKEN",
            "url_vars": [["tenant", "TENANT"]]
        })
    );
    assert_eq!(serde_json::from_value::<Given>(json).unwrap(), given);
    // A field it does not have is refused rather than dropped, a value
    // for a secret above all, which no field holds.
    let sent = serde_json::json!({
        "type": "program", "command": "npx", "args": [], "env": [],
        "secret_env": ["TOKEN"], "arg_env": [], "secret": "a value"
    });
    assert!(serde_json::from_value::<Given>(sent).is_err());
}

/// A definition somebody wrote, with a comment, a provenance and extras
/// beside its transport.
const KEPT: &str = "\
# Ours, kept as written.
format = 1
name = \"internal\"
description = \"Our own server\" # what it is for

[transport]
type = \"stdio\"
command = \"old\"

[provenance]
upstream_name = \"io.example/internal\"

[extras.claude]
timeout = 30000
";

#[test]
fn a_transport_written_again_keeps_every_other_key_and_its_comments() {
    let given = McpMother::given_address(false);

    let text = retransported(KEPT, &given).unwrap();

    let before = McpDefinition::parse(KEPT).unwrap();
    let after = McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}:\n{text}"));
    assert_eq!(after.name, before.name);
    assert_eq!(after.description, before.description);
    assert_eq!(after.provenance, before.provenance);
    assert_eq!(after.extras, before.extras);
    assert_eq!(Given::of(&after), Some(as_read(&given)), "{text}");
    for kept in [
        "# Ours, kept as written.\n",
        "description = \"Our own server\" # what it is for\n",
    ] {
        assert!(text.contains(kept), "{text}");
    }
    // Where the transport was, its inputs after it, and the extras after
    // both, as a file is read top to bottom.
    let at = |key: &str| text.find(key).unwrap_or_else(|| panic!("{key}:\n{text}"));
    assert!(at("[transport]") < at("[[inputs]]"), "{text}");
    assert!(at("[[inputs]]") < at("[provenance]"), "{text}");
    assert!(!text.contains("command = \"old\""), "{text}");
}

#[test]
fn a_transport_is_not_written_into_what_reads_as_no_definition() {
    assert!(retransported("format = 1\nname = ", &McpMother::given()).is_err());
}
