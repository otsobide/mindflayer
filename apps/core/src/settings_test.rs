//! Unit tests for `settings.rs`, kept beside it rather than inside it.

use super::*;

use crate::kind::Kind;
use crate::workspace::MIND_FORMAT_VERSION;

const ID_TEXT: &str = "0123456789abcdef0123456789abcdef";

fn path() -> &'static Path {
    Path::new(".mind/settings.json")
}

/// What a new project made with every default would hold, with an id fixed
/// so the text can be compared byte for byte.
fn standard() -> Settings {
    Settings {
        id: ProjectId::parse(ID_TEXT).unwrap(),
        ..Settings::new(
            "collapse",
            Agent::ALL.to_vec(),
            Share::Local,
            Directories::default()
                .with(Kind::Skill, "skills")
                .with(Kind::Rule, "rules"),
        )
    }
}

/// A settings text holding what [`standard`] does, with `member` in place
/// of the one with its key, or after them when none has it.
fn with_member(member: &str) -> String {
    let key = member.split(':').next().unwrap();
    let mut members = vec![
        r#""version": 3"#,
        r#""id": "0123456789abcdef0123456789abcdef""#,
        r#""name": "collapse""#,
        r#""agents": ["claude", "codex", "opencode"]"#,
        r#""share": "local""#,
        r#""edit_tracked": false"#,
        r#""directories": {"skills": "skills", "rules": "rules"}"#,
    ];
    match members.iter().position(|written| written.starts_with(key)) {
        Some(at) => members[at] = member,
        None => members.push(member),
    }
    format!("{{\n  {}\n}}\n", members.join(",\n  "))
}

/// The detail a refusal of `text` gives.
fn refusal(text: &str) -> String {
    match parse(path(), text).unwrap_err() {
        WorkspaceError::Malformed { detail, .. } => detail,
        other => panic!("not refused as malformed: {other}"),
    }
}

#[test]
fn new_settings_round_trip_through_their_text() {
    let settings = Settings::new(
        "say \"hi\" \\ and\u{1b}",
        vec![Agent::Codex, Agent::Claude],
        Share::Shared,
        Directories::default().with(Kind::Skill, ".claude/skills"),
    );

    let read = parse(path(), &settings.text().unwrap()).unwrap();

    assert_eq!(read, settings);
}

#[test]
fn the_text_is_pretty_printed_and_ends_in_a_newline() {
    assert_eq!(
        standard().text().unwrap(),
        r#"{
  "version": 3,
  "id": "0123456789abcdef0123456789abcdef",
  "name": "collapse",
  "agents": ["claude", "codex", "opencode"],
  "share": "local",
  "edit_tracked": false,
  "directories": {
    "skills": "skills",
    "rules": "rules"
  }
}
"#
    );
}

#[test]
fn a_project_authoring_nothing_writes_an_empty_directories() {
    let settings = Settings {
        directories: Directories::default(),
        ..standard()
    };

    let text = settings.text().unwrap();

    assert!(text.contains("  \"directories\": {}\n}\n"), "{text}");
}

#[test]
fn a_version_4_settings_file_is_refused_naming_the_project_marker() {
    let text = with_member(&format!("\"version\": {}", MIND_FORMAT_VERSION + 1));

    let error = parse(path(), &text).unwrap_err();

    assert!(
        matches!(
            error,
            WorkspaceError::Version {
                marker: Marker::Project,
                found: 4,
                ..
            }
        ),
        "{error}"
    );
    assert!(
        error.to_string().contains("project marker version 4"),
        "{error}"
    );
}

#[test]
fn a_newer_version_is_refused_for_being_newer_whatever_else_the_file_holds() {
    let error = parse(path(), "{\"version\": 4, \"agents\": \"everyone\"}").unwrap_err();

    assert!(
        matches!(error, WorkspaceError::Version { found: 4, .. }),
        "{error}"
    );
}

#[test]
fn a_settings_file_claiming_a_toml_version_is_refused() {
    let detail = refusal(&with_member("\"version\": 2"));

    assert!(detail.contains("version 2 is a mind.toml's"), "{detail}");
}

#[test]
fn a_settings_file_without_directories_names_no_folder() {
    let text = standard().text().unwrap();
    let start = text.find(",\n  \"directories\"").unwrap();
    let without = format!("{}\n}}\n", &text[..start]);

    let settings = parse(path(), &without).unwrap();

    assert!(settings.directories.is_empty());
    for kind in Kind::ALL {
        assert_eq!(settings.directories.get(kind), None, "{kind}");
    }
}

#[test]
fn edit_tracked_left_out_is_off() {
    let text = standard().text().unwrap();
    let without = text.replace("  \"edit_tracked\": false,\n", "");

    assert!(!parse(path(), &without).unwrap().edit_tracked);
}

#[test]
fn a_key_this_build_does_not_know_is_read_past() {
    let text = with_member(r#""targets": {"skills": [".skills"]}"#);

    assert_eq!(parse(path(), &text).unwrap(), standard());
}

#[test]
fn a_directory_key_this_build_does_not_know_is_carried() {
    let text = standard().text().unwrap().replace(
        "\"rules\": \"rules\"",
        "\"rules\": \"rules\",\n    \"mcp\": \"mcp\"",
    );

    let settings = parse(path(), &text).unwrap();

    assert_eq!(settings.text().unwrap(), text);
}

#[test]
fn an_unknown_agent_is_refused_with_every_name_there_is() {
    let detail = refusal(&with_member(r#""agents": ["claude", "cursor"]"#));

    assert_eq!(
        detail,
        "`agents`: unknown agent `cursor`; expected claude, codex or opencode"
    );
}

#[test]
fn each_key_the_format_defines_is_held_to_what_it_holds() {
    for (member, expected) in [
        (
            r#""id": "0123456789ABCDEF0123456789ABCDEF""#,
            "`id` is not 32 lowercase hexadecimal digits",
        ),
        (r#""id": 12"#, "`id` is not 32 lowercase hexadecimal digits"),
        (r#""name": ["collapse"]"#, "`name` is not a string"),
        (
            r#""agents": "claude""#,
            "`agents` is not a list of agent names",
        ),
        (r#""agents": [1]"#, "`agents` is not a list of agent names"),
        (
            r#""share": "team""#,
            "`share` is neither \"local\" nor \"shared\"",
        ),
        (
            r#""edit_tracked": "no""#,
            "`edit_tracked` is neither true nor false",
        ),
        (
            r#""directories": ["skills"]"#,
            "`directories` is not an object",
        ),
        (
            r#""directories": {"skills": 1}"#,
            "`directories.skills` is not a string",
        ),
        (
            r#""directories": {"skills": "a", "skills": "b"}"#,
            "`skills` is written more than once, so which one is meant cannot be told",
        ),
        (r#""version": "3""#, "`version` is not a version number"),
        (r#""version": 3.5"#, "`version` is not a version number"),
    ] {
        assert_eq!(refusal(&with_member(member)), expected, "{member}");
    }
}

#[test]
fn a_key_the_format_defines_is_needed_unless_it_says_otherwise() {
    for key in [VERSION, ID, NAME, AGENTS, SHARE] {
        let text = standard().text().unwrap();
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("\"{key}\"")))
            .unwrap();
        let without = text.replace(&format!("{line}\n"), "");

        assert_eq!(refusal(&without), format!("`{key}` is missing"), "{key}");
    }
}

#[test]
fn a_key_written_twice_is_refused() {
    let text = standard().text().unwrap().replace(
        "\"share\": \"local\",",
        "\"share\": \"local\",\n  \"share\": \"shared\",",
    );

    assert_eq!(
        refusal(&text),
        "`share` is written more than once, so which one is meant cannot be told"
    );
}

#[test]
fn what_is_not_strict_json_is_refused() {
    for text in [
        "// a comment\n{\"version\": 3}\n",
        "{\"version\": 3,}\n",
        "[3]\n",
        "",
    ] {
        let error = parse(path(), text).unwrap_err();
        assert!(
            matches!(error, WorkspaceError::Malformed { .. }),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn an_id_is_serialized_as_its_text() {
    let id = ProjectId::parse(ID_TEXT).unwrap();

    assert_eq!(
        serde_json::to_string(&id).unwrap(),
        format!("\"{ID_TEXT}\"")
    );
}

#[test]
fn an_id_is_32_lowercase_hexadecimal_digits_and_new_ones_differ() {
    let one = ProjectId::generate();
    let other = ProjectId::generate();

    assert_ne!(one, other);
    for id in [&one, &other] {
        assert_eq!(ProjectId::parse(id.as_str()).as_ref(), Some(id));
    }
    for wrong in [
        "",
        "0123456789abcdef",
        "0123456789abcdef0123456789abcdef0",
        "0123456789ABCDEF0123456789abcdef",
        "0123456789abcdeg0123456789abcdef",
    ] {
        assert_eq!(ProjectId::parse(wrong), None, "{wrong}");
    }
}

/// Unix only, as Windows makes no path that is not UTF-8 from bytes.
#[cfg(unix)]
#[test]
fn a_directory_that_is_not_utf8_is_refused_rather_than_mangled() {
    use std::os::unix::ffi::OsStrExt as _;

    let directory = std::ffi::OsStr::from_bytes(b"skills\xff");
    let settings = Settings {
        directories: Directories::default().with(Kind::Skill, directory),
        ..standard()
    };

    let error = settings.text().unwrap_err();

    assert!(
        matches!(error, WorkspaceError::NonUtf8Path { .. }),
        "{error}"
    );
}
