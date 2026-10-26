//! Unit tests for `artifact.rs`, kept beside it rather than inside it.

use super::*;
use crate::rule::{Lenient, RuleManifest};
use crate::skill::{Compatibility, Metadata};

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
            compatibility: Compatibility::Unstated,
            metadata: Metadata::default(),
        }),
        summary: first_line(description),
    }
}

/// `skill`, declaring a `compatibility` too.
fn compatible(compatibility: &str) -> Artifact {
    let mut artifact = skill("a", "d", "/s/a");
    if let Declared::Skill(manifest) = &mut artifact.declared {
        manifest.compatibility = Compatibility::Stated(compatibility.to_owned());
    }
    artifact
}

fn rule(name: &str, summary: Option<&str>) -> Artifact {
    declaring(name, summary, RuleManifest::default())
}

/// `rule`, with front matter that says `manifest`.
fn declaring(name: &str, summary: Option<&str>, manifest: RuleManifest) -> Artifact {
    Artifact {
        name: name.to_owned(),
        path: PathBuf::from("/work/repo/.mind/rules").join(format!("{name}.md")),
        project: PathBuf::from("/work/repo"),
        declared: Declared::Rule(manifest),
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
fn flags_a_doubled_hyphen_and_says_what_a_name_has_to_be() {
    let issues = skill("a--b", "Fill forms", "/s/a--b").validate();
    assert_eq!(
        issues,
        vec![ValidationIssue::NameNotKebabCase {
            segment: "a--b".into()
        }]
    );
    assert!(
        issues[0].to_string().contains("joined by single hyphens"),
        "{}",
        issues[0]
    );
    // A rule's route is held to it segment by segment, the same rule.
    assert_eq!(
        rule("git/no--force", Some("Never")).validate(),
        vec![ValidationIssue::NameNotKebabCase {
            segment: "no--force".into()
        }]
    );
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
fn a_compatibility_once_written_has_to_say_something_and_fit() {
    assert_eq!(compatible("Requires git").validate(), vec![]);
    assert_eq!(
        compatible("  ").validate(),
        vec![ValidationIssue::CompatibilityEmpty]
    );
    let long = "\u{00df}".repeat(MAX_COMPATIBILITY_LEN + 1);
    assert_eq!(
        compatible(&long).validate(),
        vec![ValidationIssue::CompatibilityTooLong {
            length: MAX_COMPATIBILITY_LEN + 1
        }]
    );

    let mut artifact = skill("a", "d", "/s/a");
    if let Declared::Skill(manifest) = &mut artifact.declared {
        manifest.compatibility = Compatibility::NotAString;
    }
    assert_eq!(
        artifact.validate(),
        vec![ValidationIssue::CompatibilityNotAString]
    );
}

#[test]
fn metadata_that_is_not_strings_is_reported_entry_by_entry() {
    let mut artifact = skill("a", "d", "/s/a");
    if let Declared::Skill(manifest) = &mut artifact.declared {
        manifest.metadata = Metadata {
            entries: vec![("author".into(), "acme".into())],
            ignored: vec!["internal".into(), "pinned".into()],
            not_a_map: false,
            repeated: vec!["author".into()],
        };
    }
    assert_eq!(
        artifact.validate(),
        vec![
            ValidationIssue::MetadataNotAString {
                key: "internal".into()
            },
            ValidationIssue::MetadataNotAString {
                key: "pinned".into()
            },
            ValidationIssue::MetadataRepeated {
                key: "author".into()
            },
        ]
    );

    if let Declared::Skill(manifest) = &mut artifact.declared {
        manifest.metadata = Metadata {
            not_a_map: true,
            ..Metadata::default()
        };
    }
    assert_eq!(artifact.validate(), vec![ValidationIssue::MetadataNotAMap]);
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
fn front_matter_that_does_not_parse_is_a_warning_and_not_a_problem() {
    // Read as content, so the rule still says everything it was written to.
    let manifest = RuleManifest {
        unparsed: Some("found unknown anchor".into()),
        ..RuleManifest::default()
    };
    let issues = declaring("typescript", Some("---"), manifest).validate();

    assert_eq!(
        issues,
        vec![ValidationIssue::FrontMatterUnparsed {
            reason: "found unknown anchor".into()
        }]
    );
    assert!(issues.iter().all(ValidationIssue::is_warning));
    assert!(
        issues[0].to_string().contains("read as content"),
        "{}",
        issues[0]
    );
}

#[test]
fn a_rules_keys_in_another_shape_are_warnings() {
    let manifest = RuleManifest {
        paths: Lenient::Ignored,
        description: Lenient::Ignored,
        unparsed: None,
    };
    assert_eq!(
        declaring("api", Some("API"), manifest).validate(),
        vec![
            ValidationIssue::PathsNotStrings,
            ValidationIssue::DescriptionNotAString
        ]
    );

    let manifest = RuleManifest {
        paths: Lenient::Stated(vec!["src/**".into()]),
        description: Lenient::Stated("The API".into()),
        unparsed: None,
    };
    assert_eq!(declaring("api", Some("API"), manifest).validate(), vec![]);
}

#[test]
fn a_rule_with_front_matter_and_no_body_has_nothing_to_say() {
    let manifest = RuleManifest {
        paths: Lenient::Stated(vec!["src/**".into()]),
        ..RuleManifest::default()
    };
    assert_eq!(
        declaring("api", None, manifest).validate(),
        vec![ValidationIssue::Empty]
    );
}

#[test]
fn a_rule_describes_itself_only_when_its_front_matter_says() {
    assert_eq!(rule("api", Some("API")).description(), None);
    let manifest = RuleManifest {
        description: Lenient::Stated("The API".into()),
        ..RuleManifest::default()
    };
    assert_eq!(
        declaring("api", Some("API"), manifest).description(),
        Some("The API")
    );
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

#[test]
fn only_the_name_its_folder_and_the_description_are_strict() {
    let strict = [
        ValidationIssue::NameEmpty,
        ValidationIssue::NameNotKebabCase {
            segment: "a--b".into(),
        },
        ValidationIssue::NameSegmentTooLong {
            segment: "a".into(),
            length: 65,
        },
        ValidationIssue::NameDirectoryMismatch {
            name: "a".into(),
            directory: "b".into(),
        },
        ValidationIssue::DescriptionEmpty,
        ValidationIssue::DescriptionTooLong { length: 1025 },
    ];
    let lenient = [
        ValidationIssue::CompatibilityEmpty,
        ValidationIssue::CompatibilityTooLong { length: 501 },
        ValidationIssue::CompatibilityNotAString,
        ValidationIssue::MetadataNotAMap,
        ValidationIssue::MetadataNotAString { key: "k".into() },
        ValidationIssue::MetadataRepeated { key: "k".into() },
        ValidationIssue::Empty,
        ValidationIssue::FrontMatterUnparsed { reason: "r".into() },
        ValidationIssue::PathsNotStrings,
        ValidationIssue::DescriptionNotAString,
    ];
    assert!(strict.iter().all(ValidationIssue::is_strict));
    assert!(!lenient.iter().any(ValidationIssue::is_strict));
}

#[test]
fn only_what_an_agent_reads_past_is_a_warning() {
    let warnings = [
        ValidationIssue::CompatibilityEmpty,
        ValidationIssue::CompatibilityTooLong { length: 501 },
        ValidationIssue::CompatibilityNotAString,
        ValidationIssue::MetadataNotAMap,
        ValidationIssue::MetadataNotAString { key: "k".into() },
        ValidationIssue::MetadataRepeated { key: "k".into() },
        ValidationIssue::FrontMatterUnparsed { reason: "r".into() },
        ValidationIssue::PathsNotStrings,
        ValidationIssue::DescriptionNotAString,
    ];
    let problems = [
        ValidationIssue::NameEmpty,
        ValidationIssue::NameNotKebabCase {
            segment: "a--b".into(),
        },
        ValidationIssue::NameSegmentTooLong {
            segment: "a".into(),
            length: 65,
        },
        ValidationIssue::NameDirectoryMismatch {
            name: "a".into(),
            directory: "b".into(),
        },
        ValidationIssue::DescriptionEmpty,
        ValidationIssue::DescriptionTooLong { length: 1025 },
        ValidationIssue::Empty,
    ];
    assert!(warnings.iter().all(ValidationIssue::is_warning));
    assert!(!problems.iter().any(ValidationIssue::is_warning));
    // What install refuses is never only a warning.
    assert!(!warnings.iter().any(ValidationIssue::is_strict));
}

#[test]
fn the_strict_rules_are_asked_of_the_folder_a_skill_is_about_to_be_filed_in() {
    let artifact = skill("ship", "Ship it", "/shelf/source/deploy");
    let manifest = artifact.manifest().unwrap();
    // Wherever it was read from, filed under its own name it is fine.
    assert_eq!(strict_issues(manifest, "ship"), vec![]);
    assert_eq!(
        strict_issues(manifest, "deploy"),
        vec![ValidationIssue::NameDirectoryMismatch {
            name: "ship".into(),
            directory: "deploy".into(),
        }]
    );
}
