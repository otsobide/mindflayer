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
fn reads_allowed_tools_written_either_way() {
    let inline =
        SkillManifest::parse("name: a\ndescription: d\nallowed-tools: Read, Write , Bash\n")
            .unwrap();
    let list = SkillManifest::parse(
        "name: a\ndescription: d\nallowed-tools:\n  - Read\n  - Write\n  - Bash\n",
    )
    .unwrap();
    let expected = Some(vec!["Read".into(), "Write".into(), "Bash".into()]);
    assert_eq!(inline.allowed_tools, expected);
    assert_eq!(list.allowed_tools, expected);
}

#[test]
fn rejects_front_matter_missing_a_required_key() {
    assert!(SkillManifest::parse("name: a\n").is_err());
    assert!(SkillManifest::parse("description: d\n").is_err());
}
