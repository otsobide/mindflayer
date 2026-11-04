//! Cutting a skill's or a rule's file at its front matter, held as
//! properties over documents nobody wrote by hand.
//!
//! Every artifact is read through this one cut, the panel's file contents
//! and a skill's summary included, and the body handed back is the prompt an
//! agent reads, so a change to where it cuts moves what every agent is told.

use mindweaver_core::frontmatter::{split, FrontMatterError};
use mindweaver_core::mother::random;
use mindweaver_core::mother::SkillMother;
use mindweaver_core::skill::SkillManifest;

const DRAWS: usize = 300;

/// Lines of text that are never a fence: a `---` line is drawn apart, since
/// where one is decides the cut.
fn lines(most: usize) -> String {
    (0..random::between(0, most))
        .map(|_| {
            let line = random::text(30).replace(['\n', '\r'], "");
            let line = if line == "---" {
                String::from("--")
            } else {
                line
            };
            format!("{line}{}", random::pick(&["\n", "\r\n"]))
        })
        .collect()
}

#[test]
fn a_document_is_cut_between_its_fences_and_hands_back_both_halves_whole() {
    for _ in 0..DRAWS {
        let (front_matter, body) = (lines(6), lines(10));
        let (open, close) = (
            random::pick(&["---\n", "---\r\n"]),
            random::pick(&["---\n", "---\r\n"]),
        );
        let source = format!("{open}{front_matter}{close}{body}");

        let document = split(&source).unwrap_or_else(|e| panic!("{source:?}: {e}"));

        assert_eq!(document.front_matter, front_matter, "{source:?}");
        assert_eq!(document.body, body, "{source:?}");
    }
}

#[test]
fn a_byte_order_mark_before_the_fence_is_no_reason_to_miss_it() {
    for _ in 0..DRAWS {
        let (front_matter, body) = (lines(4), lines(4));
        let source = format!("\u{feff}---\n{front_matter}---\n{body}");

        let document = split(&source).unwrap();

        assert_eq!(
            (document.front_matter, document.body),
            (front_matter.as_str(), body.as_str())
        );
    }
}

#[test]
fn the_first_closing_fence_ends_the_front_matter_and_a_later_one_is_body() {
    for _ in 0..DRAWS {
        let (front_matter, middle, rest) = (lines(4), lines(4), lines(4));
        let source = format!("---\n{front_matter}---\n{middle}---\n{rest}");

        let document = split(&source).unwrap();

        assert_eq!(document.front_matter, front_matter);
        assert_eq!(document.body, format!("{middle}---\n{rest}"));
    }
}

#[test]
fn a_document_not_opening_with_a_fence_has_no_front_matter() {
    for _ in 0..DRAWS {
        let first = random::pick(&[" ---\n", "--- \n", "----\n", "# Title\n", "\n---\n"]);
        let source = format!("{first}{}---\n", lines(4));

        assert_eq!(split(&source), Err(FrontMatterError::Missing), "{source:?}");
    }
    assert_eq!(split(""), Err(FrontMatterError::Missing));
}

#[test]
fn a_fence_never_closed_is_refused_rather_than_read_to_the_end() {
    for _ in 0..DRAWS {
        let source = format!("---\n{}", lines(8));

        assert_eq!(
            split(&source),
            Err(FrontMatterError::Unterminated),
            "{source:?}"
        );
    }
}

#[test]
fn a_skill_a_mother_writes_is_read_back_as_the_name_and_description_it_holds() {
    for _ in 0..DRAWS {
        let name = random::kebab();
        let description = SkillMother::description();

        let text = SkillMother::create(&name, &description);
        let manifest = SkillManifest::parse(split(&text).unwrap().front_matter).unwrap();

        assert_eq!(manifest.name, name);
        assert_eq!(manifest.description, description);
    }
}
