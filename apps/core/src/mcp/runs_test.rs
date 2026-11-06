use super::*;

fn program(command: &str, args: &[&str]) -> Runs {
    Runs::Program {
        command: command.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

#[test]
fn a_program_is_read_off_its_command_and_its_arguments() {
    let entry = Value::object([
        ("type", Value::from("stdio")),
        ("command", Value::from("npx")),
        ("args", Value::array(["-y", "${EXAMPLE_WORKSPACE}"])),
        ("env", Value::object([("KEY", Value::from("${KEY}"))])),
    ]);

    assert_eq!(
        Runs::of(&entry),
        Some(program("npx", &["-y", "${EXAMPLE_WORKSPACE}"]))
    );
}

#[test]
fn a_program_with_no_arguments_written_is_read_as_one_with_none() {
    let entry = Value::object([("command", Value::from("server"))]);

    assert_eq!(Runs::of(&entry), Some(program("server", &[])));
}

#[test]
fn a_program_written_as_one_line_is_read_off_its_first_word_and_the_rest() {
    let entry = Value::object([
        ("type", Value::from("local")),
        (
            "command",
            Value::array(["npx", "-y", "{env:EXAMPLE_WORKSPACE}"]),
        ),
    ]);

    assert_eq!(
        Runs::of_line(&entry),
        Some(program("npx", &["-y", "{env:EXAMPLE_WORKSPACE}"]))
    );
    assert_eq!(
        Runs::of_line(&Value::object([(
            "command",
            Value::array(Vec::<&str>::new())
        )])),
        None
    );
}

#[test]
fn an_address_is_read_off_its_url_in_either_shape() {
    let entry = Value::object([
        ("type", Value::from("http")),
        ("url", Value::from("https://${TENANT}.example.com/mcp")),
        ("headers", Value::object(Vec::<(&str, Value)>::new())),
    ]);
    let address = Runs::Address {
        url: String::from("https://${TENANT}.example.com/mcp"),
    };

    assert_eq!(Runs::of(&entry), Some(address.clone()));
    assert_eq!(Runs::of_line(&entry), Some(address));
}

#[test]
fn an_entry_holding_neither_is_read_as_nothing() {
    let entry = Value::object([("type", Value::from("stdio"))]);
    let numbered = Value::object([
        ("command", Value::from("npx")),
        ("args", Value::array([Value::Integer(1)])),
    ]);

    assert_eq!(Runs::of(&entry), None);
    assert_eq!(Runs::of(&numbered), None);
    assert_eq!(Runs::of_line(&Value::from("npx")), None);
}

#[test]
fn a_program_is_said_as_the_shell_line_that_runs_it() {
    let said = program(
        "npx",
        &[
            "-y",
            "@example/server",
            "--workspace",
            "${EXAMPLE_WORKSPACE}",
            "{env:EXAMPLE_KEY}",
        ],
    )
    .to_string();

    assert_eq!(
        said,
        "npx -y @example/server --workspace ${EXAMPLE_WORKSPACE} {env:EXAMPLE_KEY}"
    );
}

#[test]
fn a_word_a_shell_would_split_or_read_otherwise_is_quoted() {
    let said = program(
        "/opt/my server/run",
        &[
            "two words",
            "",
            "it's",
            "{\"debug\":true}",
            "a;b",
            "$(x)",
            "$HOME",
        ],
    )
    .to_string();

    assert_eq!(
        said,
        r#"'/opt/my server/run' 'two words' '' 'it'\''s' '{"debug":true}' 'a;b' '$(x)' '$HOME'"#
    );
}

#[test]
fn an_address_is_said_as_it_is() {
    let said = Runs::Address {
        url: String::from("https://example.com/mcp?a=1&b=2"),
    }
    .to_string();

    assert_eq!(said, "https://example.com/mcp?a=1&b=2");
}
