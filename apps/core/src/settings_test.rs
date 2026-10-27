//! Unit tests for `settings.rs`, kept beside it rather than inside it.

use super::*;

use crate::kind::Kind;
use crate::workspace::{MIND_FORMAT_VERSION, SETTINGS_TOML_VERSION};

const ID_TEXT: &str = "0123456789abcdef0123456789abcdef";

/// Where a `settings.json` is, whose name says it is JSON.
fn path() -> &'static Path {
    Path::new(".mind/settings.json")
}

/// Where a `settings.toml` is, whose name says it is TOML.
fn toml_path() -> &'static Path {
    Path::new(".mind/settings.toml")
}

/// What a new project made with every default would hold, with an id fixed
/// so the text can be compared byte for byte: version 5, a `settings.toml`.
fn standard_in_toml() -> Settings {
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

fn standard_toml() -> String {
    standard_in_toml().text().unwrap()
}

/// The same, as a version 3 `settings.json` holds it.
fn standard() -> Settings {
    Settings {
        version: SETTINGS_VERSION,
        ..standard_in_toml()
    }
}

/// A `settings.json` holding what [`standard`] does, with `member` in place
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

/// [`with_member`] without the member whose key is `key`.
fn without_member(key: &str) -> String {
    let text = with_member(r#""version": 3"#);
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("\"{key}\"")))
        .unwrap();
    text.replace(&format!("{line}\n"), "")
        .replace(",\n}", "\n}")
}

/// The detail a refusal of `text`, read as a `settings.json`, gives.
fn refusal(text: &str) -> String {
    match parse(path(), text).unwrap_err() {
        WorkspaceError::Malformed { detail, .. } => detail,
        other => panic!("not refused as malformed: {other}"),
    }
}

/// The detail a refusal of `text`, read as a `settings.toml`, gives.
fn toml_refusal(text: &str) -> String {
    match parse(toml_path(), text).unwrap_err() {
        WorkspaceError::Malformed { detail, .. } => detail,
        other => panic!("not refused as malformed: {other}"),
    }
}

/// [`standard_in_toml`], installing skills in `.skills` alone.
fn in_skills() -> Settings {
    Settings {
        targets: Targets::default().with(Kind::Skill, vec![PathBuf::from(".skills")]),
        ..standard_in_toml()
    }
}

// ---------------------------------------------------------------------------
// settings.toml
// ---------------------------------------------------------------------------

#[test]
fn a_settings_toml_is_written_from_a_template_whose_comments_say_what_each_key_does() {
    assert_eq!(
        standard_toml(),
        r#"# Mindflayer mind project: what Mindflayer needs to know about the project
# this .mind is in, and nothing else. No agent reads it.
#
# `name` is what the project is shown as, and `agents` the agents that work
# in it (claude, codex, opencode): what is installed into the project is
# copied where each of them reads. `mind agents` changes them.
#
# `share` is "local" to keep .mind, and what is installed, out of git
# for this checkout alone, or "shared" to leave both for the team to
# commit. `edit_tracked` lets a file git tracks, such as AGENTS.md, be
# edited to reach an agent in a local project. `mind share` changes both.
#
# `directories` says where the project keeps the skills and rules it writes
# itself, relative to the directory holding .mind, to commit with its
# code; a kind left out has no folder. A `[targets]` table, when there,
# names the folders a kind is installed in instead of where the agents
# read: `mind targets` writes it.
#
# `id` names the project for good, wherever it moves, and `version` is how
# this file is laid out: change neither.
version = 5
id = "0123456789abcdef0123456789abcdef"
name = "collapse"
agents = ["claude", "codex", "opencode"]
share = "local"
edit_tracked = false

[directories]
skills = "skills"
rules = "rules"
"#
    );
}

#[test]
fn new_settings_round_trip_through_their_toml() {
    let settings = Settings::new(
        "say \"hi\" \\ and\u{1b}",
        vec![Agent::Codex, Agent::Claude],
        Share::Shared,
        Directories::default().with(Kind::Skill, ".claude/skills"),
    );

    let read = parse(toml_path(), &settings.text().unwrap()).unwrap();

    assert_eq!(read, settings);
}

#[test]
fn a_settings_toml_is_version_5_whatever_it_sets() {
    let in_toml = in_skills();

    let text = in_toml.text().unwrap();

    assert!(text.contains("\nversion = 5\n"), "{text}");
    assert!(
        text.ends_with("\n[targets]\nskills = [\".skills\"]\n"),
        "{text}"
    );
    assert_eq!(parse(toml_path(), &text).unwrap(), in_toml);
    assert!(!standard_toml().contains("\n[targets]\n"));
}

#[test]
fn a_settings_toml_claiming_another_files_version_is_refused() {
    for (version, file) in [(2, "mind.toml"), (3, "settings.json"), (4, "settings.json")] {
        let text = standard_toml().replace("version = 5", &format!("version = {version}"));

        assert_eq!(
            toml_refusal(&text),
            format!("version {version} is a {file}'s, and a settings.toml is version 5"),
        );
    }
}

#[test]
fn a_settings_toml_is_held_to_the_keys_a_settings_json_is() {
    for (from, to, expected) in [
        (
            r#"agents = ["claude", "codex", "opencode"]"#,
            r#"agents = ["claude", "cursor"]"#,
            "`agents`: unknown agent `cursor`; expected claude, codex or opencode",
        ),
        (
            r#"share = "local""#,
            r#"share = "team""#,
            "`share` is neither \"local\" nor \"shared\"",
        ),
        (
            "edit_tracked = false",
            "edit_tracked = \"no\"",
            "`edit_tracked` is neither true nor false",
        ),
        (
            "[directories]\nskills = \"skills\"",
            "directories = [\"skills\"]\n[nothing]\nskills = \"skills\"",
            "`directories` is not a table",
        ),
        (
            "skills = \"skills\"",
            "skills = 1",
            "`directories.skills` is not a string",
        ),
        (
            "rules = \"rules\"\n",
            "rules = \"rules\"\n\n[targets]\nskills = []\n",
            "`targets.skills` names no folder; leave `skills` out to install them where the \
             project's agents read",
        ),
        (
            "version = 5",
            "version = 5.0",
            "`version` is not a version number",
        ),
    ] {
        let text = standard_toml().replacen(from, to, 1);
        assert_ne!(text, standard_toml(), "{from}");

        assert_eq!(toml_refusal(&text), expected, "{to}");
    }
}

#[test]
fn a_settings_toml_that_is_not_toml_is_refused() {
    let detail = toml_refusal("version = 5\nversion = 5\n");

    assert!(detail.contains("duplicate key"), "{detail}");
}

#[test]
fn a_key_this_build_does_not_know_is_read_past_in_toml_too() {
    let text = standard_toml().replace(
        "edit_tracked = false\n",
        "edit_tracked = false\nservers = { github = [\"npx\"] }\n",
    ) + "\n[mcp]\nanything = true\n";

    assert_eq!(parse(toml_path(), &text).unwrap(), standard_in_toml());
}

#[test]
fn a_directory_or_target_key_this_build_does_not_know_is_carried_in_toml() {
    let text = in_skills()
        .text()
        .unwrap()
        .replace(
            "rules = \"rules\"\n",
            "rules = \"rules\"\nmcp = \"servers\"\n",
        )
        .replace(
            "skills = [\".skills\"]\n",
            "skills = [\".skills\"]\nmcp = [\".mcp\"]\n",
        );

    let read = parse(toml_path(), &text).unwrap();

    assert_eq!(read.text().unwrap(), text);
}

// ---------------------------------------------------------------------------
// Either file, and a settings.json read as it was written
// ---------------------------------------------------------------------------

#[test]
fn a_project_authoring_nothing_writes_an_empty_directories() {
    let settings = Settings {
        directories: Directories::default(),
        ..standard_in_toml()
    };

    let text = settings.text().unwrap();

    assert!(text.ends_with("\n\n[directories]\n"), "{text}");
    assert_eq!(parse(toml_path(), &text).unwrap(), settings);
}

#[test]
fn a_settings_file_newer_than_this_build_is_refused_naming_the_project_marker() {
    let newer = MIND_FORMAT_VERSION + 1;
    for (path, text) in [
        (path(), with_member(&format!("\"version\": {newer}"))),
        (
            toml_path(),
            standard_toml().replace("version = 5", &format!("version = {newer}")),
        ),
    ] {
        let error = parse(path, &text).unwrap_err();

        assert!(
            matches!(
                error,
                WorkspaceError::Version {
                    marker: Marker::Project,
                    found,
                    ..
                } if found == newer
            ),
            "{error}"
        );
        assert!(
            error
                .to_string()
                .contains(&format!("project marker version {newer}")),
            "{error}"
        );
    }
}

#[test]
fn a_newer_version_is_refused_for_being_newer_whatever_else_the_file_holds() {
    let error = parse(path(), "{\"version\": 6, \"agents\": \"everyone\"}").unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Version { found: 6, .. }),
        "{error}"
    );

    let error = parse(toml_path(), "version = 6\nagents = \"everyone\"\n").unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Version { found: 6, .. }),
        "{error}"
    );
}

#[test]
fn a_settings_json_claiming_another_files_version_is_refused() {
    let detail = refusal(&with_member("\"version\": 2"));
    assert_eq!(
        detail,
        "version 2 is a mind.toml's, and a settings.json is version 3 or 4"
    );

    let detail = refusal(&with_member("\"version\": 5"));
    assert_eq!(
        detail,
        "version 5 is a settings.toml's, and a settings.json is version 3 or 4"
    );
}

#[test]
fn a_settings_file_without_directories_names_no_folder() {
    let toml = standard_toml();
    let start = toml.find("\n[directories]").unwrap();
    for (path, text) in [
        (path(), without_member(DIRECTORIES)),
        (toml_path(), toml[..start].to_owned()),
    ] {
        let settings = parse(path, &text).unwrap();

        assert!(settings.directories.is_empty());
        for kind in Kind::ALL {
            assert_eq!(settings.directories.get(kind), None, "{kind}");
        }
    }
}

#[test]
fn edit_tracked_left_out_is_off() {
    let toml = standard_toml().replace("edit_tracked = false\n", "");

    assert!(
        !parse(path(), &without_member(EDIT_TRACKED))
            .unwrap()
            .edit_tracked
    );
    assert!(!parse(toml_path(), &toml).unwrap().edit_tracked);
}

#[test]
fn a_key_this_build_does_not_know_is_read_past() {
    let text = with_member(r#""servers": {"github": ["npx"]}"#);

    assert_eq!(parse(path(), &text).unwrap(), standard());
}

#[test]
fn a_version_4_file_is_read_with_or_without_targets() {
    let without = with_member("\"version\": 4");
    let with = format!(
        "{},\n  \"targets\": {{\"skills\": [\".skills\", \"tools/skills\"]}}\n}}\n",
        without.trim_end().trim_end_matches('}').trim_end()
    );

    assert_eq!(parse(path(), &without).unwrap().version, TARGETS_VERSION);
    assert_eq!(
        parse(path(), &with).unwrap().targets.get(Kind::Skill),
        Some(&[PathBuf::from(".skills"), PathBuf::from("tools/skills")][..])
    );
}

#[test]
fn targets_written_into_a_version_3_file_by_hand_are_read_all_the_same() {
    let text = with_member(r#""targets": {"skills": [".skills"]}"#);

    let settings = parse(path(), &text).unwrap();

    assert_eq!(settings.version, SETTINGS_VERSION);
    assert_eq!(
        settings.targets.get(Kind::Skill),
        Some(&[PathBuf::from(".skills")][..])
    );
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

/// `targets` that do not read as targets: each member, with what a version
/// 4 file holding it is refused for.
const UNREAD_TARGETS: [(&str, &str); 5] = [
    (r#""targets": [".skills"]"#, "`targets` is not an object"),
    (
        r#""targets": {"skills": ".skills"}"#,
        "`targets.skills` is not a list of folders",
    ),
    (
        r#""targets": {"skills": [".skills", 1]}"#,
        "`targets.skills` is not a list of folders",
    ),
    (
        r#""targets": {"skills": []}"#,
        "`targets.skills` names no folder; leave `skills` out to install them where the \
         project's agents read",
    ),
    (
        r#""targets": {"skills": ["a"], "skills": ["b"]}"#,
        "`skills` is written more than once, so which one is meant cannot be told",
    ),
];

#[test]
fn targets_in_a_version_4_file_are_held_to_what_they_hold() {
    for (member, expected) in UNREAD_TARGETS {
        let text = with_member(member).replace("\"version\": 3", "\"version\": 4");

        assert_eq!(refusal(&text), expected, "{member}");
    }
}

#[test]
fn targets_a_version_3_file_holds_that_do_not_read_are_read_past() {
    // Every build before targets opened such a file, reading the key past,
    // so this one opens it too, installing where the agents read.
    for (member, _) in UNREAD_TARGETS {
        assert_eq!(
            parse(path(), &with_member(member)).unwrap(),
            standard(),
            "{member}"
        );
    }
}

#[test]
fn a_key_the_format_defines_is_needed_unless_it_says_otherwise() {
    for key in [VERSION, ID, NAME, AGENTS, SHARE] {
        let toml = standard_toml();
        let line = toml
            .lines()
            .find(|line| line.starts_with(&format!("{key} = ")))
            .unwrap();
        let toml = toml.replace(&format!("{line}\n"), "");

        let missing = format!("`{key}` is missing");
        assert_eq!(refusal(&without_member(key)), missing, "{key}");
        assert_eq!(toml_refusal(&toml), missing, "{key}");
    }
}

#[test]
fn a_key_written_twice_is_refused() {
    let text = with_member(r#""version": 3"#).replace(
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
        ..standard_in_toml()
    };

    let error = settings.text().unwrap_err();

    assert!(
        matches!(error, WorkspaceError::NonUtf8Path { .. }),
        "{error}"
    );
}

#[test]
fn a_directory_or_target_key_a_settings_json_holds_that_this_build_does_not_know_is_carried() {
    let text = with_member(r#""version": 4"#)
        .replace(
            r#""rules": "rules"}"#,
            r#""rules": "rules", "mcp": "servers"}"#,
        )
        .replace(
            "\n}\n",
            ",\n  \"targets\": {\"skills\": [\".skills\"], \"mcp\": [\".mcp\"]}\n}\n",
        );

    let settings = parse(path(), &text).unwrap();
    let toml = Settings {
        version: SETTINGS_TOML_VERSION,
        ..settings
    }
    .text()
    .unwrap();

    assert!(
        toml.ends_with(
            "[directories]\nskills = \"skills\"\nrules = \"rules\"\nmcp = \"servers\"\n\n\
             [targets]\nskills = [\".skills\"]\nmcp = [\".mcp\"]\n"
        ),
        "{toml}"
    );
}
