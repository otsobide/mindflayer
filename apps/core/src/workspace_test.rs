//! Unit tests for `workspace.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn templates_round_trip_through_the_parser() {
    let mind: MindConfig =
        toml::from_str(&mind_template("collapse", &Directories::default())).unwrap();
    assert_eq!(mind.version, FORMAT_VERSION);
    assert_eq!(mind.name, "collapse");
    // Spelled out rather than left to the default, so the file answers
    // where a project's skills go without anyone reading the source.
    assert_eq!(mind.directories.get(Kind::Skill), Some(Path::new("skills")));
    assert_eq!(mind.directories.get(Kind::Rule), Some(Path::new("rules")));

    let flayer: FlayerConfig =
        toml::from_str(&flayer_template("projects", &Directories::default())).unwrap();
    assert_eq!(flayer.version, FORMAT_VERSION);
    assert_eq!(flayer.name, "projects");
    assert!(flayer.projects.is_empty());
    assert_eq!(
        flayer.directories.get(Kind::Skill),
        Some(Path::new("skills"))
    );
}

#[test]
fn a_name_needing_quotes_still_round_trips() {
    let awkward = "quote\" and \\ backslash";
    let mind: MindConfig =
        toml::from_str(&mind_template(awkward, &Directories::default())).unwrap();
    assert_eq!(mind.name, awkward);
}
