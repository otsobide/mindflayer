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
        placement: Placement::authored(),
        others: Vec::new(),
        differ: false,
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
        placement: Placement::authored(),
        others: Vec::new(),
        differ: false,
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

#[test]
fn a_rules_text_is_refused_for_saying_nothing_and_for_what_its_front_matter_leaves_unread() {
    assert_eq!(rule_text_issues("# Never force-push\n"), vec![]);
    assert_eq!(
        rule_text_issues("---\npaths: src/**\n---\nOnly in src.\n"),
        vec![]
    );
    // A heading of hashes alone says nothing, as reading it finds.
    assert_eq!(
        rule_text_issues("---\npaths: src/**\n---\n#\n"),
        vec![ValidationIssue::Empty]
    );
    assert_eq!(
        rule_text_issues("---\npaths: [1, 2]\ndescription: [x]\n---\nBody.\n"),
        vec![
            ValidationIssue::PathsNotStrings,
            ValidationIssue::DescriptionNotAString
        ]
    );
    let unparsed = rule_text_issues("---\n*nope\n---\nBody.\n");
    assert!(
        matches!(
            unparsed.as_slice(),
            [ValidationIssue::FrontMatterUnparsed { .. }]
        ),
        "{unparsed:?}"
    );
}

/// A copy at `path` that `agents` read, with `issues` there.
fn copy_at(path: &str, agents: &[Agent], authored: bool, installed: bool) -> ArtifactCopy {
    ArtifactCopy {
        path: PathBuf::from(path),
        placement: Placement {
            agents: agents.to_vec(),
            authored,
            installed,
        },
        issues: Vec::new(),
    }
}

/// `skill` at `path`, placed as `placement` says, with `others` beside it.
fn copied(path: &str, placement: Placement, others: Vec<ArtifactCopy>) -> Artifact {
    let mut artifact = skill("deploy", "Ship it", path);
    artifact.placement = placement;
    artifact.others = others;
    artifact
}

#[test]
fn an_artifact_loaded_on_its_own_is_one_authored_copy_no_agent_reads() {
    let artifact = skill("deploy", "", "/s/deploy");

    let copies = artifact.copies();

    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].path(), Path::new("/s/deploy"));
    assert_eq!(copies[0].agents(), []);
    assert!(copies[0].is_authored() && !copies[0].is_installed());
    assert_eq!(copies[0].issues(), artifact.validate());
    assert_eq!(artifact.provenance(), Provenance::Authored);
    assert_eq!(artifact.agents(), []);
}

#[test]
fn every_copy_is_judged_and_what_another_has_is_added_after_the_firsts() {
    let mismatch = ValidationIssue::NameDirectoryMismatch {
        name: "deploy".into(),
        directory: "Deploy".into(),
    };
    let mut emptied = copy_at("/p/.claude/skills/Deploy", &[Agent::Claude], false, false);
    emptied.issues = vec![mismatch.clone(), ValidationIssue::DescriptionEmpty];
    let mut first = skill("deploy", "Ship it", "/p/.agents/skills/Deploy");
    first.others = vec![emptied];

    // What both copies have is said once, the first's first; what only the
    // other has is added.
    assert_eq!(first.copies()[0].issues(), std::slice::from_ref(&mismatch));
    assert_eq!(
        first.copies()[1].issues(),
        [mismatch.clone(), ValidationIssue::DescriptionEmpty]
    );
    assert_eq!(
        first.validate(),
        [mismatch, ValidationIssue::DescriptionEmpty]
    );
}

#[test]
fn an_artifacts_agents_are_every_copys_once_in_report_order() {
    let artifact = copied(
        "/p/.claude/skills/deploy",
        Placement {
            agents: vec![Agent::Claude, Agent::OpenCode],
            authored: false,
            installed: false,
        },
        vec![copy_at(
            "/p/.agents/skills/deploy",
            &[Agent::Codex, Agent::OpenCode],
            false,
            false,
        )],
    );

    assert_eq!(
        artifact.agents(),
        [Agent::Claude, Agent::Codex, Agent::OpenCode]
    );
}

#[test]
fn an_artifact_is_installed_only_when_every_copy_is_recorded() {
    let placed = |authored, installed| Placement {
        agents: vec![Agent::Claude],
        authored,
        installed,
    };
    let other = |authored, installed| {
        vec![copy_at(
            "/p/.agents/skills/deploy",
            &[Agent::Codex],
            authored,
            installed,
        )]
    };

    let both = copied("/p/a", placed(false, true), other(false, true));
    assert_eq!(both.provenance(), Provenance::Installed);
    // A copy nothing records is somebody's, so the whole is not Mindflayer's.
    let one = copied("/p/a", placed(false, true), other(false, false));
    assert_eq!(one.provenance(), Provenance::Foreign);
    assert_eq!(one.copies()[0].provenance(), Provenance::Installed);
    assert_eq!(one.copies()[1].provenance(), Provenance::Foreign);
    // And the project's own when that copy sits where it authors.
    let authored = copied("/p/a", placed(false, true), other(true, false));
    assert_eq!(authored.provenance(), Provenance::Authored);
    let neither = copied("/p/a", placed(false, false), other(false, false));
    assert_eq!(neither.provenance(), Provenance::Foreign);
}

/// What an issue serializes to is a contract: its code, what it is about by
/// name, and whether it is only worth a warning. The message is the one
/// part worded rather than named, and is the issue's text exactly.
#[test]
fn an_issue_serializes_its_code_what_it_is_about_and_whether_it_warns() {
    let segment = || String::from("Deploy");
    let key = || String::from("team");
    let issues = [
        (
            ValidationIssue::NameEmpty,
            serde_json::json!({"code": "name_empty", "warning": false}),
        ),
        (
            ValidationIssue::NameNotKebabCase { segment: segment() },
            serde_json::json!({
                "code": "name_not_kebab_case", "segment": "Deploy", "warning": false,
            }),
        ),
        (
            ValidationIssue::NameSegmentTooLong {
                segment: segment(),
                length: 70,
            },
            serde_json::json!({
                "code": "name_segment_too_long", "segment": "Deploy", "length": 70,
                "warning": false,
            }),
        ),
        (
            ValidationIssue::NameDirectoryMismatch {
                name: String::from("deploy"),
                directory: String::from("ship"),
            },
            serde_json::json!({
                "code": "name_directory_mismatch", "name": "deploy", "directory": "ship",
                "warning": false,
            }),
        ),
        (
            ValidationIssue::DescriptionEmpty,
            serde_json::json!({"code": "description_empty", "warning": false}),
        ),
        (
            ValidationIssue::DescriptionTooLong { length: 1100 },
            serde_json::json!({"code": "description_too_long", "length": 1100, "warning": false}),
        ),
        (
            ValidationIssue::CompatibilityEmpty,
            serde_json::json!({"code": "compatibility_empty", "warning": true}),
        ),
        (
            ValidationIssue::CompatibilityTooLong { length: 600 },
            serde_json::json!({"code": "compatibility_too_long", "length": 600, "warning": true}),
        ),
        (
            ValidationIssue::CompatibilityNotAString,
            serde_json::json!({"code": "compatibility_not_a_string", "warning": true}),
        ),
        (
            ValidationIssue::MetadataNotAMap,
            serde_json::json!({"code": "metadata_not_a_map", "warning": true}),
        ),
        (
            ValidationIssue::MetadataNotAString { key: key() },
            serde_json::json!({"code": "metadata_not_a_string", "key": "team", "warning": true}),
        ),
        (
            ValidationIssue::MetadataRepeated { key: key() },
            serde_json::json!({"code": "metadata_repeated", "key": "team", "warning": true}),
        ),
        (
            ValidationIssue::Empty,
            serde_json::json!({"code": "content_empty", "warning": false}),
        ),
        (
            ValidationIssue::FrontMatterUnparsed {
                reason: String::from("a tab"),
            },
            serde_json::json!({
                "code": "front_matter_unparsed", "reason": "a tab", "warning": true,
            }),
        ),
        (
            ValidationIssue::PathsNotStrings,
            serde_json::json!({"code": "paths_not_strings", "warning": true}),
        ),
        (
            ValidationIssue::DescriptionNotAString,
            serde_json::json!({"code": "description_not_a_string", "warning": true}),
        ),
    ];
    assert_eq!(
        issues.len(),
        ValidationIssue::CODES.len(),
        "one of each issue"
    );

    for (issue, expected) in issues {
        let mut serialized = serde_json::to_value(&issue).unwrap();
        let message = serialized
            .as_object_mut()
            .unwrap()
            .remove("message")
            .unwrap();
        assert_eq!(message, issue.to_string());
        assert_eq!(serialized, expected, "{issue:?}");
    }
}

#[test]
fn an_issue_says_its_code_first_and_its_message_last() {
    let serialized =
        serde_json::to_string(&ValidationIssue::DescriptionTooLong { length: 1100 }).unwrap();

    assert_eq!(
        serialized,
        format!(
            "{{\"code\":\"description_too_long\",\"length\":1100,\"warning\":false,\
             \"message\":\"`description` is 1100 characters, over the {MAX_DESCRIPTION_LEN} \
             character limit\"}}"
        )
    );
}

#[test]
fn provenance_is_serialized_as_its_slug() {
    let serialized = serde_json::to_string(&[
        Provenance::Installed,
        Provenance::Authored,
        Provenance::Foreign,
    ])
    .unwrap();

    assert_eq!(serialized, r#"["installed","authored","foreign"]"#);
}

/// What an artifact serializes to is a contract, as a status report's is:
/// each field by its name, in the order a reader meets them, every copy
/// with what is wrong with it there.
#[test]
fn an_artifact_serializes_as_a_listing_shows_it() {
    let mut first = copied(
        "/work/repo/.claude/skills/deploy",
        Placement {
            agents: vec![Agent::Claude, Agent::OpenCode],
            authored: false,
            installed: true,
        },
        vec![copy_at(
            "/work/repo/.agents/skills/deploy",
            &[Agent::Codex, Agent::OpenCode],
            false,
            false,
        )],
    );
    first.others[0].issues = vec![ValidationIssue::DescriptionEmpty];
    first.differ = true;

    let serialized = serde_json::to_string(&first).unwrap();

    let empty = concat!(
        r#"{"code":"description_empty","warning":false,"#,
        r#""message":"`description` is empty"}"#
    );
    assert_eq!(
        serialized,
        [
            r#"{"kind":"skill","name":"deploy","summary":"Ship it","#,
            r#""path":"/work/repo/.claude/skills/deploy","project":"/work/repo","#,
            r#""provenance":"foreign","agents":["claude","codex","opencode"],"#,
            &format!(r#""issues":[{empty}],"copies_differ":true,"copies":["#),
            r#"{"path":"/work/repo/.claude/skills/deploy","agents":["claude","opencode"],"#,
            r#""provenance":"installed","issues":[]},"#,
            r#"{"path":"/work/repo/.agents/skills/deploy","agents":["codex","opencode"],"#,
            &format!(r#""provenance":"foreign","issues":[{empty}]}}]}}"#),
        ]
        .concat()
    );
}
