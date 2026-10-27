//! Unit tests for `rule.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn reads_front_matter_and_hands_back_the_body_after_it() {
    let source = "---\ndescription: How the API is written\npaths:\n  - \"src/api/**/*.ts\"\n  - lib/**\n---\n\n# API rules\n";

    let (manifest, body) = RuleManifest::read(source);

    assert_eq!(
        manifest,
        RuleManifest {
            paths: Lenient::Stated(vec!["src/api/**/*.ts".into(), "lib/**".into()]),
            description: Lenient::Stated("How the API is written".into()),
            unparsed: None,
        }
    );
    assert_eq!(body, "\n# API rules\n");
}

#[test]
fn a_rule_without_front_matter_is_all_body() {
    let source = "# Never force-push\n\nUse --force-with-lease.\n";

    assert_eq!(
        RuleManifest::read(source),
        (RuleManifest::default(), source)
    );
}

#[test]
fn a_horizontal_rule_followed_by_prose_stays_content() {
    // Never closed, so a horizontal rule and nothing else.
    let open = "---\n\nNever force-push.\n";
    // Closed, but what it fences is prose, not a mapping.
    let fenced = "---\nNever force-push a shared branch.\n---\n\nUse the lease.\n";
    // A list is no more a mapping than a sentence is.
    let listed = "---\n- one\n- two\n---\nBody\n";
    // A comment to YAML, and a heading to markdown, which is what it is here.
    let headed = "---\n# Never force-push\n---\nUse the lease.\n";

    for source in [open, fenced, listed, headed] {
        assert_eq!(
            RuleManifest::read(source),
            (RuleManifest::default(), source),
            "{source:?}"
        );
    }
}

#[test]
fn a_sentence_shaped_like_a_key_is_read_as_front_matter() {
    // YAML draws the line, not the author: `Note: ...` is a mapping with one
    // key nothing reads, so the sentence leaves the body, as any key does.
    let source = "---\nNote: never force-push to main.\n---\n\n# Git\n";

    assert_eq!(
        RuleManifest::read(source),
        (RuleManifest::default(), "\n# Git\n")
    );
}

#[test]
fn prose_yaml_cannot_parse_stays_content_and_says_why() {
    // To YAML these open an alias, a reserved character and a flow sequence
    // with text after it, so they read as front matter written wrong.
    for source in [
        "---\n**Never** force-push.\n---\nBody\n",
        "---\n`make test` before you push.\n---\nBody\n",
        "---\n[Docs](https://example.com) say so.\n---\nBody\n",
    ] {
        let (manifest, body) = RuleManifest::read(source);

        assert_eq!(body, source);
        assert!(manifest.unparsed.is_some(), "{source:?}");
    }
}

#[test]
fn unparseable_yaml_stays_content_and_says_why() {
    // A glob that opens with `*` is a YAML alias unless it is quoted, which is
    // the easiest way to write front matter that does not parse.
    let source = "---\npaths:\n  - **/*.ts\n---\n\n# TypeScript\n";

    let (manifest, body) = RuleManifest::read(source);

    assert_eq!(body, source);
    assert_eq!(manifest.paths, Lenient::Unstated);
    assert!(manifest.unparsed.is_some(), "{manifest:?}");
}

#[test]
fn a_key_written_twice_is_not_read_as_either() {
    // Which one an agent reads is up to its YAML parser, and some refuse the
    // file, so neither is a safe guess.
    let source = "---\npaths: a/**\npaths: b/**\n---\nBody\n";

    let (manifest, body) = RuleManifest::read(source);

    assert_eq!(body, source);
    assert!(manifest.unparsed.is_some(), "{manifest:?}");
}

#[test]
fn two_horizontal_rules_with_nothing_between_them_stay_content() {
    // Not a mapping, so not front matter: the fences stay in the body, and
    // the rule is listed by its first one, as it was before rules were read.
    for source in ["---\n---\nBody\n", "---\n  \n\n---\nBody\n"] {
        assert_eq!(
            RuleManifest::read(source),
            (RuleManifest::default(), source),
            "{source:?}"
        );
    }
}

#[test]
fn one_pattern_is_a_list_of_one_and_never_split_on_its_commas() {
    let (manifest, _) = RuleManifest::read("---\npaths: \"src/**/*.{ts,tsx}\"\n---\nBody\n");

    assert_eq!(
        manifest.paths.stated(),
        Some(&vec![String::from("src/**/*.{ts,tsx}")])
    );
}

#[test]
fn a_key_in_another_shape_is_ignored_and_the_rest_still_read() {
    let (manifest, body) =
        RuleManifest::read("---\npaths:\n  - src/**\n  - 42\ndescription: [a, b]\n---\nBody\n");

    assert_eq!(manifest.paths, Lenient::Ignored);
    assert_eq!(manifest.description, Lenient::Ignored);
    assert_eq!(manifest.unparsed, None);
    // Still front matter: what is ignored is a key, not the block.
    assert_eq!(body, "Body\n");

    let (manifest, _) = RuleManifest::read("---\npaths: {src: yes}\ndescription: 7\n---\n");
    assert_eq!(manifest.paths, Lenient::Ignored);
    assert_eq!(manifest.description, Lenient::Ignored);
}

#[test]
fn keys_it_does_not_read_pass_through() {
    // Cursor's keys, an agent's own, and a key that is not even a string.
    let source = "---\nglobs: \"*.ts\"\nalwaysApply: false\n1: one\n---\nBody\n";

    assert_eq!(
        RuleManifest::read(source),
        (RuleManifest::default(), "Body\n")
    );
}

#[test]
fn a_key_with_nothing_in_it_is_unstated() {
    for source in [
        "---\npaths:\ndescription:\n---\nBody\n",
        "---\npaths: []\ndescription: \"\"\n---\nBody\n",
        "---\npaths: \"\"\ndescription: \"  \"\n---\nBody\n",
        "---\npaths: [\"\", \" \"]\n---\nBody\n",
    ] {
        let (manifest, body) = RuleManifest::read(source);

        assert_eq!(manifest, RuleManifest::default(), "{source:?}");
        // Still front matter, declaring nothing.
        assert_eq!(body, "Body\n", "{source:?}");
    }
}

#[test]
fn a_blank_pattern_is_left_out_of_the_rest() {
    let (manifest, _) = RuleManifest::read("---\npaths: [\"\", src/**]\n---\nBody\n");

    assert_eq!(manifest.paths.stated(), Some(&vec![String::from("src/**")]));
}

#[test]
fn front_matter_is_read_through_a_byte_order_mark_and_crlf() {
    let source = "\u{feff}---\r\npaths: src/**\r\n---\r\nBody\r\n";

    let (manifest, body) = RuleManifest::read(source);

    assert_eq!(manifest.paths.stated(), Some(&vec![String::from("src/**")]));
    assert_eq!(body, "Body\r\n");
}

#[test]
fn a_rule_is_scoped_only_by_paths_written_as_they_are_read() {
    let scoped = |source: &str| RuleManifest::read(source).0.is_scoped();

    assert!(scoped("---\npaths:\n  - \"src/**/*.rs\"\n---\nBody\n"));
    assert!(scoped("---\npaths: src/**/*.rs\n---\nBody\n"));
    // Nothing stated, or stated in a shape nothing reads: about every file.
    assert!(!scoped("Body\n"));
    assert!(!scoped("---\ndescription: Only this\n---\nBody\n"));
    assert!(!scoped("---\npaths:\n---\nBody\n"));
    assert!(!scoped("---\npaths: 7\n---\nBody\n"));
    assert!(!scoped("---\npaths:\n  - 7\n---\nBody\n"));
}
