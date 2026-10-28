//! Unit tests for `template.rs`, kept beside it rather than inside it.

use super::*;

/// What a template serializes to is a contract: its name, its kind as a
/// command line takes it, and where it is, none for one built in.
#[test]
fn a_template_serializes_its_name_its_kind_and_where_it_is() {
    let templates = [
        Template {
            name: String::from("full"),
            kind: Kind::Skill,
            origin: Origin::BuiltIn,
        },
        Template {
            name: String::from("runbook"),
            kind: Kind::Rule,
            origin: Origin::Path(PathBuf::from("/work/api/.mind/templates/runbook.md")),
        },
    ];

    let serialized = serde_json::to_string(&templates).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"[{"name":"full","kind":"skill","origin":null},"#,
            r#"{"name":"runbook","kind":"rule","#,
            r#""origin":"/work/api/.mind/templates/runbook.md"}]"#,
        )
    );
}
