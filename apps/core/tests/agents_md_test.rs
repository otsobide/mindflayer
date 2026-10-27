//! Rendering a project's rules into its `AGENTS.md`: the file made, changed
//! and taken away, every byte outside the block kept, and what is refused.

mod common;

use std::fs;
use std::path::PathBuf;

use mindflayer_core::agents_md::{self, AgentsMdError, Malformed, Rendered, Written, BEGIN, END};
use mindflayer_core::{Artifact, MindProject};
use tempfile::TempDir;

/// What a team's `AGENTS.md` holds before Mindflayer writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A project, and a library of rules beside it to render from.
struct Fixture {
    _dir: TempDir,
    project: MindProject,
    library: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    Fixture {
        library: dir.path().join("library"),
        _dir: dir,
        project,
    }
}

impl Fixture {
    /// The rule filed at `route` in the library, holding `source`, loaded.
    fn rule(&self, route: &str, source: &str) -> Artifact {
        let file = self.library.join(format!("{route}.md"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        Artifact::rule(&file, route.to_owned(), &self.library).unwrap()
    }

    fn agents_md(&self) -> PathBuf {
        self.project.root().join(agents_md::FILE)
    }

    fn render(&self, rules: &[&Artifact], created: bool) -> Result<Rendered, AgentsMdError> {
        agents_md::render(&self.project, rules.iter().copied(), created)
    }

    fn text(&self) -> String {
        fs::read_to_string(self.agents_md()).unwrap()
    }
}

#[test]
fn a_render_matches_the_golden_block() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let tone = fixture.rule(
        "review/tone",
        "---\ndescription: How a review reads\n---\n\n# Tone\n\nSay what a change does before what is wrong with it.\nAsk, rather than tell, when unsure.\n",
    );
    let force = fixture.rule(
        "git/no-force-push",
        "Never force-push a branch somebody else may have pulled.\n\nRewrite only what is yours alone.\n",
    );
    // About some files only, so Claude's alone (#43), and not in the block.
    let errors = fixture.rule(
        "rust/errors",
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n",
    );
    // A `paths` in a shape nothing reads says nothing, so this one is about
    // every file.
    let commits = fixture.rule(
        "commit-style",
        "---\npaths: 7\n---\nWrite commit messages as `<area>: <imperative summary>`.\n",
    );

    let rendered = fixture
        .render(&[&tone, &force, &errors, &commits], false)
        .unwrap();
    assert_eq!(rendered.written, Written::Changed);
    assert_eq!(
        rendered.routes,
        ["commit-style", "git/no-force-push", "review/tone"]
    );
    assert_eq!(fixture.text(), include_str!("fixtures/agents-md.md"));
}

#[test]
fn created_is_said_only_when_the_file_was_not_there() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");

    let first = fixture.render(&[&rule], false).unwrap();
    assert_eq!(first.file, fixture.agents_md());
    assert_eq!(first.written, Written::Created);
    assert!(first.created());
    assert_eq!(
        fixture.text(),
        format!("{BEGIN}\n<!-- mindflayer:rule a -->\nRule a.\n{END}\n")
    );

    let again = fixture.render(&[&rule], true).unwrap();
    assert_eq!(again.written, Written::Unchanged);
    assert!(!again.created());

    let other = fixture.rule("b", "Rule b.\n");
    let changed = fixture.render(&[&rule, &other], true).unwrap();
    assert_eq!(changed.written, Written::Changed);
    assert!(!changed.created());
}

#[test]
fn a_file_somebody_wrote_is_changed_and_never_said_to_be_created() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    let rendered = fixture.render(&[&rule], false).unwrap();
    assert_eq!(rendered.written, Written::Changed);
    assert!(!rendered.created());
}

#[test]
fn no_rules_and_no_file_writes_nothing() {
    let fixture = fixture();
    let rendered = fixture.render(&[], true).unwrap();
    assert_eq!(rendered.written, Written::Unchanged);
    assert!(rendered.routes.is_empty());
    assert!(!fixture.agents_md().exists());
}

#[test]
fn text_around_the_block_is_the_same_through_a_render_a_second_one_and_a_removal() {
    let fixture = fixture();
    let before =
        format!("{TEAM}{BEGIN}\n<!-- mindflayer:rule old -->\nOld.\n{END}\n\n## Later\n\nMore.\n");
    fs::write(fixture.agents_md(), &before).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    fixture.render(&[&rule], false).unwrap();
    let rendered = fixture.text();
    assert!(rendered.starts_with(&format!("{TEAM}{BEGIN}\n")));
    assert!(rendered.ends_with(&format!("{END}\n\n## Later\n\nMore.\n")));

    let again = fixture.render(&[&rule], false).unwrap();
    assert_eq!(again.written, Written::Unchanged);
    assert_eq!(fixture.text(), rendered);

    let removed = fixture.render(&[], false).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), format!("{TEAM}\n## Later\n\nMore.\n"));
}

#[test]
fn a_crlf_file_keeps_crlf_through_a_render_a_second_one_and_a_removal() {
    let fixture = fixture();
    let before = "# Working here\r\n\r\nRun `make test`.\r\n";
    fs::write(fixture.agents_md(), before).unwrap();
    let rule = fixture.rule("a", "One.\nTwo.\n");

    fixture.render(&[&rule], false).unwrap();
    let rendered = fs::read(fixture.agents_md()).unwrap();
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("{before}{BEGIN}\r\n<!-- mindflayer:rule a -->\r\nOne.\r\nTwo.\r\n{END}\r\n")
    );

    assert_eq!(
        fixture.render(&[&rule], false).unwrap().written,
        Written::Unchanged
    );
    assert_eq!(fs::read(fixture.agents_md()).unwrap(), rendered);

    fixture.render(&[], false).unwrap();
    assert_eq!(fs::read(fixture.agents_md()).unwrap(), before.as_bytes());
}

#[test]
fn removing_the_last_rule_takes_away_a_file_mindflayer_made() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    assert!(fixture.render(&[&rule], false).unwrap().created());

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Removed);
    assert!(removed.routes.is_empty());
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_file_mindflayer_made_stays_once_somebody_wrote_in_it() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    let mut text = fixture.text();
    text.push_str("\nA line of somebody's.\n");
    fs::write(fixture.agents_md(), &text).unwrap();

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), "\nA line of somebody's.\n");
}

#[test]
fn a_file_mindflayer_did_not_make_stays_when_the_block_was_all_it_held() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();

    let removed = fixture.render(&[], false).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), "");
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_a_file_mindflayer_made_is_emptied_not_taken_away() {
    let fixture = fixture();
    let target = fixture.project.root().join("NOTES.md");
    fs::write(&target, "").unwrap();
    std::os::unix::fs::symlink("NOTES.md", fixture.agents_md()).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    assert!(fs::read_to_string(&target).unwrap().contains("Rule a."));

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert!(fs::symlink_metadata(fixture.agents_md())
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "");
}

#[cfg(unix)]
#[test]
fn a_link_to_nothing_in_place_of_the_file_is_never_written_through() {
    let fixture = fixture();
    std::os::unix::fs::symlink("NOTES.md", fixture.agents_md()).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Write { path, .. }) if *path == fixture.agents_md()),
        "{refused:?}"
    );
    assert!(!fixture.project.root().join("NOTES.md").exists());
}

#[test]
fn a_malformed_block_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let before = format!("{TEAM}{BEGIN}\n<!-- mindflayer:rule a -->\nRule a.\n");
    fs::write(fixture.agents_md(), &before).unwrap();
    let rule = fixture.rule("b", "Rule b.\n");

    match fixture.render(&[&rule], true) {
        Err(AgentsMdError::Malformed { file, problem }) => {
            assert_eq!(file, fixture.agents_md());
            assert_eq!(problem, Malformed::Unclosed { line: 4 });
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fixture.text(), before);
}

#[test]
fn a_duplicated_marker_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let before = format!(
        "{BEGIN}\n<!-- mindflayer:rule a -->\nOne.\n<!-- mindflayer:rule a -->\nTwo.\n{END}\n"
    );
    fs::write(fixture.agents_md(), &before).unwrap();

    let refused = fixture.render(&[], true);
    assert!(
        matches!(
            refused,
            Err(AgentsMdError::Malformed {
                problem: Malformed::Repeated { line: 4, .. },
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), before);
}

#[test]
fn two_rules_of_one_name_are_refused_and_nothing_is_written() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let elsewhere = TempDir::new().unwrap();
    let file = elsewhere.path().join("a.md");
    fs::write(&file, "Another rule a.\n").unwrap();
    let other = Artifact::rule(&file, "a".to_owned(), elsewhere.path()).unwrap();

    let refused = fixture.render(&[&rule, &other], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Twice { route }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_rule_that_cannot_be_read_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fs::remove_file(rule.path()).unwrap();

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(refused, Err(AgentsMdError::Rule(_))),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), TEAM);
}

#[test]
fn a_rule_holding_a_marker_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let rule = fixture.rule("a", &format!("Before.\n{END}\nAfter.\n"));
    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Marked { route, .. }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_skill_has_no_section() {
    let fixture = fixture();
    let folder = fixture.library.join("commit-style");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        common::skill("commit-style", "Commits"),
    )
    .unwrap();
    let skill = Artifact::skill(&folder, &fixture.library).unwrap();

    let refused = fixture.render(&[&skill], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::NotARule { name }) if name == "skill:commit-style"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}
