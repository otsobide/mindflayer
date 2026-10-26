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
    assert_eq!(
        manifest.compatibility.as_deref(),
        Some("Requires git and jq")
    );
    let manifest = SkillManifest::parse("name: a\ndescription: d\n").unwrap();
    assert_eq!(manifest.compatibility, None);
}

#[test]
fn rejects_front_matter_missing_a_required_key() {
    assert!(SkillManifest::parse("name: a\n").is_err());
    assert!(SkillManifest::parse("description: d\n").is_err());
}
