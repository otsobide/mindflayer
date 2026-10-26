//! Unit tests for `skill.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn parses_the_required_keys() {
    let manifest = SkillManifest::parse("name: pdf-forms\ndescription: Fill PDF forms\n")
        .expect("valid front matter");
    assert_eq!(manifest.name, "pdf-forms");
    assert_eq!(manifest.description, "Fill PDF forms");
    assert_eq!(manifest.allowed_tools, None);
}

#[test]
fn reads_allowed_tools_written_any_way() {
    let spaced =
        SkillManifest::parse("name: a\ndescription: d\nallowed-tools: Read Write  Bash\n").unwrap();
    let commas =
        SkillManifest::parse("name: a\ndescription: d\nallowed-tools: Read, Write , Bash\n")
            .unwrap();
    let list = SkillManifest::parse(
        "name: a\ndescription: d\nallowed-tools:\n  - Read\n  - Write\n  - Bash\n",
    )
    .unwrap();
    let expected = Some(vec!["Read".into(), "Write".into(), "Bash".into()]);
    assert_eq!(spaced.allowed_tools, expected);
    assert_eq!(commas.allowed_tools, expected);
    assert_eq!(list.allowed_tools, expected);
}

#[test]
fn a_tool_pattern_is_not_split_inside_its_parentheses() {
    assert_eq!(
        split_tools("Bash(git add *) Bash(jq:*), Read"),
        vec!["Bash(git add *)", "Bash(jq:*)", "Read"]
    );
    // An unbalanced pattern keeps the rest of the line rather than losing it.
    assert_eq!(
        split_tools("Bash(git add * Read"),
        vec!["Bash(git add * Read"]
    );
    assert_eq!(split_tools(" , "), Vec::<String>::new());
}

#[test]
fn reads_compatibility_when_it_is_there() {
    let manifest =
        SkillManifest::parse("name: a\ndescription: d\ncompatibility: Requires git and jq\n")
            .unwrap();
    assert_eq!(manifest.compatibility.stated(), Some("Requires git and jq"));
    let manifest = SkillManifest::parse("name: a\ndescription: d\n").unwrap();
    assert_eq!(manifest.compatibility, Compatibility::Unstated);
    let manifest = SkillManifest::parse("name: a\ndescription: d\ncompatibility:\n").unwrap();
    assert_eq!(manifest.compatibility, Compatibility::Unstated);
}

#[test]
fn a_compatibility_that_is_not_a_string_is_noted_rather_than_refused() {
    // A list of agents is the obvious thing to write; the skill still reads.
    for written in [
        "compatibility: [claude-code, codex]\n",
        "compatibility:\n  - claude-code\n  - codex\n",
        "compatibility:\n  os: linux\n",
        "compatibility: 2\n",
        "compatibility: true\n",
        // Skipped, not built: a repeated key in it is not a reason to fail.
        "compatibility:\n  os: linux\n  os: macos\n",
    ] {
        let manifest = SkillManifest::parse(&format!("name: a\ndescription: d\n{written}"))
            .unwrap_or_else(|error| panic!("{written}: {error}"));
        assert_eq!(
            manifest.compatibility,
            Compatibility::NotAString,
            "{written}"
        );
        assert_eq!(manifest.compatibility.stated(), None);
    }
}

#[test]
fn reads_metadata_as_strings_in_the_order_written() {
    let manifest = SkillManifest::parse(
        "name: a\ndescription: d\nmetadata:\n  version: \"1.0\"\n  author: acme\n",
    )
    .unwrap();
    assert_eq!(
        manifest.metadata.entries,
        vec![
            ("version".to_owned(), "1.0".to_owned()),
            ("author".to_owned(), "acme".to_owned()),
        ]
    );
    assert!(manifest.metadata.ignored.is_empty());
    assert!(!manifest.metadata.not_a_map);

    let absent = SkillManifest::parse("name: a\ndescription: d\nmetadata:\n").unwrap();
    assert_eq!(absent.metadata, Metadata::default());
}

#[test]
fn metadata_that_is_not_strings_is_named_rather_than_refused() {
    // Nested maps and booleans are written by agents that ship; the skill
    // still reads, and what is not a string is named for `validate`.
    let manifest = SkillManifest::parse(
        "name: a\ndescription: d\nmetadata:\n  author: acme\n  internal:\n    team: ops\n  pinned: true\n  7: seven\n",
    )
    .unwrap();
    assert_eq!(
        manifest.metadata.entries,
        vec![("author".to_owned(), "acme".to_owned())]
    );
    assert_eq!(manifest.metadata.ignored, vec!["internal", "pinned", "7"]);
    assert!(!manifest.metadata.not_a_map);
}

#[test]
fn metadata_that_is_not_a_map_is_ignored_whole() {
    let manifest = SkillManifest::parse("name: a\ndescription: d\nmetadata: [one, two]\n").unwrap();
    assert!(manifest.metadata.not_a_map);
    assert!(manifest.metadata.entries.is_empty());
}

#[test]
fn rejects_front_matter_missing_a_required_key() {
    assert!(SkillManifest::parse("name: a\n").is_err());
    assert!(SkillManifest::parse("description: d\n").is_err());
}
