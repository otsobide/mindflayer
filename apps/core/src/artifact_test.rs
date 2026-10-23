//! Unit tests for `artifact.rs`, kept beside it rather than inside it.

use super::*;

/// A skill built without touching the disk, to check the rules rather
/// than the loading.
fn skill(name: &str, description: &str, directory: &str) -> Artifact {
    Artifact {
        name: name.to_owned(),
        path: PathBuf::from(directory),
        project: PathBuf::from("/work/repo"),
        declared: Declared::Skill(SkillManifest {
            name: name.to_owned(),
            description: description.to_owned(),
            allowed_tools: None,
            license: None,
        }),
        summary: first_line(description),
    }
}

fn rule(name: &str, summary: Option<&str>) -> Artifact {
    Artifact {
        name: name.to_owned(),
        path: PathBuf::from("/work/repo/.mind/rules").join(format!("{name}.md")),
        project: PathBuf::from("/work/repo"),
        declared: Declared::Rule,
        summary: summary.map(str::to_owned),
    }
}

#[test]
fn a_well_formed_skill_has_no_issues() {
    assert_eq!(
        skill("pdf-forms", "Fill forms", "/s/pdf-forms").validate(),
        vec![]
    );
}

#[test]
fn flags_a_name_the_directory_does_not_match() {
    assert_eq!(
        skill("pdf-forms", "Fill forms", "/s/pdf").validate(),
        vec![ValidationIssue::NameDirectoryMismatch {
            name: "pdf-forms".into(),
            directory: "pdf".into(),
        }]
    );
}

#[test]
fn flags_a_name_that_is_not_kebab_case() {
    let issues = skill("PDF Forms", "Fill forms", "/s/PDF Forms").validate();
    assert!(issues.contains(&ValidationIssue::NameNotKebabCase {
        segment: "PDF Forms".into()
    }));
}

#[test]
fn flags_names_hyphenated_at_the_edges() {
    for name in ["-lead", "trail-"] {
        let issues = skill(name, "Fill forms", &format!("/s/{name}")).validate();
        assert!(
            issues.contains(&ValidationIssue::NameNotKebabCase {
                segment: name.into()
            }),
            "expected `{name}` to be rejected"
        );
    }
}

#[test]
fn flags_an_empty_or_whitespace_description() {
    assert_eq!(
        skill("a", "   ", "/s/a").validate(),
        vec![ValidationIssue::DescriptionEmpty]
    );
}

#[test]
fn flags_limits_by_characters_not_bytes() {
    // A character that is two bytes and exactly one char, with no
    // decomposed spelling to muddy what is being counted.
    let name = "\u{00df}".repeat(MAX_NAME_SEGMENT_LEN + 1);
    let issues = skill(&name, "d", &format!("/s/{name}")).validate();
    assert!(issues.contains(&ValidationIssue::NameSegmentTooLong {
        segment: name.clone(),
        length: MAX_NAME_SEGMENT_LEN + 1
    }));

    let long = "\u{00df}".repeat(MAX_DESCRIPTION_LEN + 1);
    let issues = skill("a", &long, "/s/a").validate();
    assert!(issues.contains(&ValidationIssue::DescriptionTooLong {
        length: MAX_DESCRIPTION_LEN + 1
    }));
}

#[test]
fn the_length_limit_is_per_segment_so_nesting_does_not_spend_it() {
    // Five segments of sixty characters is a deep route, not a long name.
    let segment = "a".repeat(60);
    let name = vec![segment; 5].join("/");
    assert_eq!(rule(&name, Some("Context")).validate(), vec![]);
}

#[test]
fn a_rule_is_judged_only_on_what_it_has() {
    // No description to be empty, no directory to disagree with.
    assert_eq!(rule("git/no-force-push", Some("Never")).validate(), vec![]);
    assert_eq!(rule("empty", None).validate(), vec![ValidationIssue::Empty]);
}

#[test]
fn a_description_opening_with_a_blank_line_still_summarises() {
    // A YAML block scalar routinely opens with one, and listing that skill
    // with no summary would be a formatting accident showing through.
    let skill = skill("a", "\n\nWhat it does.\n", "/s/a");
    assert_eq!(skill.summary(), Some("What it does."));
}

#[test]
fn an_opening_line_is_the_first_that_carries_text() {
    assert_eq!(opening_line("# Title\n\nBody\n").as_deref(), Some("Title"));
    assert_eq!(opening_line("\n\n  prose\n").as_deref(), Some("prose"));
    assert_eq!(opening_line("###\n\n# Real\n").as_deref(), Some("Real"));
    assert_eq!(opening_line("\n   \n"), None);
}
