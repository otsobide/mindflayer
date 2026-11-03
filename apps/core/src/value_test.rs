use super::*;

#[test]
fn a_hash_parsed_in_capitals_is_the_one_the_recipe_writes() {
    let written = crate::hash::bytes(b"deploy");
    let given = ContentHash::parse(&written.to_ascii_uppercase()).unwrap();

    assert_eq!(given, written);
    assert_eq!(given.as_str(), written.as_str());
}

#[test]
fn a_text_that_is_no_hash_is_not_parsed_as_one() {
    let digest = crate::hash::bytes(b"deploy");

    assert_eq!(ContentHash::parse(""), None);
    assert_eq!(ContentHash::parse(&digest[1..]), None);
    assert_eq!(ContentHash::parse(&format!("{digest}  SKILL.md")), None);
    assert_eq!(ContentHash::parse(&"g".repeat(ContentHash::LEN)), None);
}

#[test]
fn a_redacted_text_holds_no_credential_whatever_it_was_made_from() {
    let url = Redacted::new("https://user:token@example.com/skills.git");

    assert_eq!(url, "https://example.com/skills.git");
    assert!(!crate::url::carries_credentials(&url));
}

#[test]
fn an_origin_is_written_without_the_credential_it_was_built_with() {
    let origin = Origin::from("https://user:token@example.com/skills.git#main");

    assert_eq!(origin, "https://user:token@example.com/skills.git#main");
    assert_eq!(
        serde_json::to_value(&origin).unwrap(),
        serde_json::json!("https://example.com/skills.git#main")
    );
}

#[test]
fn a_value_reads_as_the_text_it_holds() {
    let name = ArtifactName::from("deploy");

    assert_eq!(format!("{name}"), "deploy");
    assert_eq!(format!("{name:?}"), format!("{:?}", "deploy"));
    assert_eq!(
        serde_json::to_value(&name).unwrap(),
        serde_json::json!("deploy")
    );
    assert!(name.starts_with("dep"));
    assert_eq!(Path::new("skills").join(&name), Path::new("skills/deploy"));
}

#[test]
fn values_sort_as_the_text_they_hold() {
    let mut names: Vec<ArtifactName> = ["b", "a", "B"].map(ArtifactName::from).into();
    names.sort();

    assert_eq!(names, ["B", "a", "b"]);
}
