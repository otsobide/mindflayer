//! Unit tests for `reading.rs`: what `show` and `validate` say of each
//! artifact, every name pinned, since each is a contract once printed.

use std::fs;
use std::io;
use std::path::Path;

use tempfile::TempDir;

use super::*;

/// The skill `name` written into a folder of skills under `root`, holding
/// `text` as its `SKILL.md`, and read.
fn skill(root: &Path, name: &str, text: &str) -> Artifact {
    let directory = root.join("skills").join(name);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("SKILL.md"), text).unwrap();
    Artifact::skill(directory, root).unwrap()
}

/// The rule `name` written as a file of rules under `root`, holding `text`,
/// and read.
fn rule(root: &Path, name: &str, text: &str) -> Artifact {
    let file = root.join("rules").join(format!("{name}.md"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, text).unwrap();
    Artifact::rule(file, name.to_owned(), root).unwrap()
}

/// `value` as JSON, every spelling of `root` in it as `<root>`: as text, so
/// the order its keys are written in is pinned with them.
fn placed(value: &impl Serialize, root: &Path) -> String {
    let text = serde_json::to_string(value).unwrap();
    let escaped = serde_json::to_string(&root.to_string_lossy()).unwrap();
    let text = text.replace(escaped.trim_matches('"'), "<root>");
    if cfg!(windows) {
        text.replace("\\\\", "/")
    } else {
        text
    }
}

/// [`placed`], read back to look into.
fn value(value: &impl Serialize, root: &Path) -> serde_json::Value {
    serde_json::from_str(&placed(value, root)).unwrap()
}

#[test]
fn a_skill_shown_says_what_it_declares_and_its_body_in_order() {
    let dir = TempDir::new().unwrap();
    let deploy = skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nallowed-tools: Bash, Read\nlicense: MIT\n\
         compatibility: Requires git\nmetadata:\n  version: \"1.0\"\n  owner:\n    team: ops\n  \
         author: acme\n---\n\nRun the pipeline.\n",
    );

    let shown = Shown::of(&deploy).unwrap();

    // Every key, in the order it is written: metadata as written too, the
    // entry that is no string left to `validate`.
    assert_eq!(
        placed(&shown, dir.path()),
        concat!(
            r#"{"kind":"skill","name":"deploy","path":"<root>/skills/deploy","#,
            r#""project":"<root>","file":"<root>/skills/deploy/SKILL.md","#,
            r#""summary":"Ship it","description":"Ship it","#,
            r#""allowed_tools":["Bash","Read"],"license":"MIT","#,
            r#""compatibility":"Requires git","#,
            r#""metadata":{"version":"1.0","author":"acme"},"paths":null,"#,
            r#""body":"\nRun the pipeline.\n"}"#,
        )
    );
}

#[test]
fn a_rule_shown_says_only_what_a_rule_can_declare() {
    let dir = TempDir::new().unwrap();
    let scoped = rule(
        dir.path(),
        "scoped",
        "---\ndescription: Keep it typed\npaths:\n  - \"*.ts\"\n---\n\n# Types\n",
    );
    let bare = rule(dir.path(), "bare", "# Commits\n\nSay why.\n");

    let scoped = Shown::of(&scoped).unwrap();
    let bare = Shown::of(&bare).unwrap();

    assert_eq!(scoped.description.as_deref(), Some("Keep it typed"));
    assert_eq!(scoped.paths, Some(vec![String::from("*.ts")]));
    assert_eq!(scoped.body, "\n# Types\n");
    for shown in [&scoped, &bare] {
        assert_eq!(shown.allowed_tools, None);
        assert_eq!(shown.license, None);
        assert_eq!(shown.compatibility, None);
        assert_eq!(shown.metadata, None);
    }
    assert_eq!(bare.description, None);
    assert_eq!(bare.paths, None);
    assert_eq!(bare.summary.as_deref(), Some("Commits"));
    assert_eq!(bare.body, "# Commits\n\nSay why.\n");
    let bare = value(&bare, dir.path());
    assert_eq!(bare["metadata"], serde_json::Value::Null);
    assert_eq!(bare["file"], "<root>/rules/bare.md");
}

#[test]
fn a_file_gone_since_it_was_read_cannot_be_shown() {
    let dir = TempDir::new().unwrap();
    let deploy = skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\n---\n",
    );
    fs::remove_dir_all(dir.path().join("skills")).unwrap();

    assert!(Shown::of(&deploy).is_err());
}

/// What does not pass is counted apart from what would not read and from
/// copies that differ, and a warning alone fails nothing.
#[test]
fn a_validation_counts_what_does_not_pass_and_what_would_not_read() {
    let dir = TempDir::new().unwrap();
    let mismatched = skill(
        dir.path(),
        "deploy",
        "---\nname: Deployment\ndescription: Ship it\n---\n",
    );
    let warned = skill(
        dir.path(),
        "nested",
        "---\nname: nested\ndescription: Nested\nmetadata:\n  owner:\n    team: ops\n---\n",
    );
    let fine = rule(dir.path(), "commits", "# Commits\n");
    let failures = [
        DiscoveryFailure::Directory {
            path: dir.path().join("elsewhere"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        },
        DiscoveryFailure::Diverged {
            kind: Kind::Skill,
            name: String::from("nested"),
            paths: vec![dir.path().join("a"), dir.path().join("b")],
        },
    ];

    let validation = Validation::of(&[&mismatched, &warned, &fine], &failures);

    assert_eq!(validation.invalid, 1);
    assert_eq!(validation.unreadable, 1);
    assert_eq!(validation.differing, 0);
    assert!(!validation.passes());
    let flags: Vec<(&str, bool)> = validation
        .artifacts
        .iter()
        .map(|checked| (checked.name.as_str(), checked.invalid))
        .collect();
    assert_eq!(
        flags,
        [("Deployment", true), ("nested", false), ("commits", false)]
    );
    let text = placed(&validation, dir.path());
    assert!(
        text.ends_with(concat!(
            r#"{"kind":"rule","name":"commits","path":"<root>/rules/commits.md","#,
            r#""project":"<root>","issues":[],"invalid":false,"copies_differ":false}],"#,
            r#""invalid":1,"unreadable":1,"differing":0}"#,
        )),
        "{text}"
    );
    assert!(text.starts_with(r#"{"artifacts":[{"kind":"skill","name":"Deployment","#));
    assert_eq!(
        value(&validation, dir.path())["artifacts"][1]["issues"][0]["warning"],
        true
    );
}

#[test]
fn a_validation_of_nothing_wrong_passes() {
    let dir = TempDir::new().unwrap();
    let fine = rule(dir.path(), "commits", "# Commits\n");

    assert!(Validation::of(&[&fine], &[]).passes());
    assert!(Validation::of(&[], &[]).passes());
}
