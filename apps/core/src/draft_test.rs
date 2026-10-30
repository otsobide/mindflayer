//! Unit tests for `draft.rs`: what core says of an artifact before a
//! command writes it, held to what the command and `validate` then say.

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::template::TEMPLATES_DIR;
use crate::MIND_DIR;

/// A project holding the skill `deploy` and the rule `git/commits`, both as
/// `add` writes them.
fn fixture() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path().join("api")).unwrap();
    lifecycle::add(None, &project, Kind::Skill, "deploy", "Ship it", None).unwrap();
    lifecycle::add(None, &project, Kind::Rule, "git/commits", "Say why", None).unwrap();
    (dir, project)
}

/// Every file under `dir` with what it holds, to tell that nothing changed.
fn files(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                folders.push(entry.path());
            } else {
                found.push((entry.path(), fs::read(entry.path()).unwrap()));
            }
        }
    }
    found.sort();
    found
}

fn codes(issues: &[ValidationIssue]) -> Vec<&'static str> {
    issues.iter().map(ValidationIssue::code).collect()
}

fn refusal(judged: &Judged) -> Option<&'static str> {
    judged.refused.as_ref().map(|said| said.code)
}

/// What `validate` says of the artifact `reference` names in `project`, as
/// it reads it from the disk.
fn validated(project: &MindProject, reference: &str) -> Vec<ValidationIssue> {
    lifecycle::find(project, &Reference::parse(reference))
        .unwrap()
        .artifact
        .expect("it reads")
        .validate()
}

#[test]
fn a_skill_add_would_write_is_judged_fit_and_nothing_is_written() {
    let (dir, project) = fixture();
    let before = files(dir.path());

    let judged = added(None, &project, Kind::Skill, "review", "Read it", None);

    assert_eq!(
        judged,
        Judged {
            issues: Vec::new(),
            refused: None,
        }
    );
    assert_eq!(files(dir.path()), before);
}

/// Each refused by the code `add` refuses it with, and in its words.
#[test]
fn what_add_would_refuse_is_said_by_its_code_and_in_its_words() {
    let (_dir, project) = fixture();

    for (kind, name, description, template, code) in [
        (Kind::Skill, "Review", "Read it", None, "add_name_unfit"),
        (Kind::Skill, "", "Read it", None, "add_name_unfit"),
        (Kind::Skill, "review", "  ", None, "add_description_empty"),
        (
            Kind::Skill,
            "review",
            "one\ntwo",
            None,
            "add_description_not_one_line",
        ),
        (Kind::Skill, "a/b", "Read it", None, "add_skill_nested"),
        (Kind::Skill, "deploy", "Ship it", None, "add_exists"),
        (Kind::Rule, "git/commits", "Say why", None, "add_exists"),
        (
            Kind::Skill,
            "review",
            "Read it",
            Some("nowhere"),
            "unknown_template",
        ),
        (
            Kind::Skill,
            "review",
            "Read it",
            Some("../up"),
            "template_name_invalid",
        ),
    ] {
        let judged = added(None, &project, kind, name, description, template);
        let written = match template {
            None => lifecycle::add(None, &project, kind, name, description, None)
                .map(|_| ())
                .expect_err("add wrote it"),
            Some(template) => template::find(None, &project, kind, template)
                .map(|_| ())
                .map_err(crate::CreateError::from)
                .expect_err("the template was found"),
        };
        assert_eq!(refusal(&judged), Some(code), "{name:?}");
        assert_eq!(written.code(), code, "{name:?}");
        assert_eq!(
            judged.refused.as_ref().unwrap().message,
            written.to_string(),
            "{name:?}"
        );
    }
}

/// A name refused for each rule it breaks says each in `validate`'s words.
#[test]
fn a_name_add_refuses_is_said_in_the_words_validate_says_it_in() {
    let (_dir, project) = fixture();

    let judged = added(None, &project, Kind::Skill, "Bad--Name", "Read it", None);

    let issue = ValidationIssue::NameNotKebabCase {
        segment: String::from("Bad--Name"),
    };
    assert_eq!(judged.refused.unwrap().message, issue.to_string());
}

/// A template's front matter can carry what `validate` warns of and `add`
/// writes all the same: said, in the words `validate` says it in once the
/// skill is written.
#[test]
fn what_validate_would_warn_of_in_what_add_writes_is_said_as_it_then_says_it() {
    let (_dir, project) = fixture();
    let template = project.mind_dir().join(TEMPLATES_DIR).join("loose");
    fs::create_dir_all(&template).unwrap();
    fs::write(
        template.join("SKILL.md"),
        "---\ncompatibility: \"\"\nmetadata:\n  owner: [a, b]\n---\n\n# {{name}}\n",
    )
    .unwrap();

    let judged = added(
        None,
        &project,
        Kind::Skill,
        "review",
        "Read it",
        Some("loose"),
    );

    assert_eq!(judged.refused, None);
    assert_eq!(
        codes(&judged.issues),
        ["compatibility_empty", "metadata_not_a_string"]
    );
    let template = template::find(None, &project, Kind::Skill, "loose").unwrap();
    lifecycle::add(
        None,
        &project,
        Kind::Skill,
        "review",
        "Read it",
        Some(&template),
    )
    .unwrap();
    assert_eq!(judged.issues, validated(&project, "skill:review"));
}

#[test]
fn a_rule_add_would_write_is_judged_fit() {
    let (_dir, project) = fixture();

    let judged = added(None, &project, Kind::Rule, "style/comments", "Why", None);

    assert_eq!(judged.refused, None);
    assert_eq!(judged.issues, Vec::new());
}

#[test]
fn contents_an_edit_would_write_are_judged_fit_and_nothing_is_written() {
    let (dir, project) = fixture();
    let before = files(dir.path());

    let judged = edited(
        None,
        &project,
        &Reference::parse("skill:deploy"),
        "---\nname: deploy\ndescription: Ship it today\n---\n\n# deploy\n",
    );

    assert_eq!(
        judged,
        Judged {
            issues: Vec::new(),
            refused: None,
        }
    );
    assert_eq!(files(dir.path()), before);
}

/// What `validate` warns of is no reason for the edit to refuse, and is said
/// as `validate` says it once the contents are written.
#[test]
fn what_validate_would_warn_of_in_contents_is_said_as_it_then_says_it() {
    let (_dir, project) = fixture();
    let contents =
        "---\nname: deploy\ndescription: Ship it\nmetadata:\n  owner: [a]\n  owner: b\n---\n\n# deploy\n";
    let reference = Reference::parse("skill:deploy");

    let judged = edited(None, &project, &reference, contents);

    assert_eq!(judged.refused, None);
    assert_eq!(
        codes(&judged.issues),
        ["metadata_not_a_string", "metadata_repeated"]
    );
    let target = lifecycle::find(&project, &reference).unwrap();
    lifecycle::replace_contents(None, &project, &target, contents.as_bytes(), None).unwrap();
    assert_eq!(judged.issues, validated(&project, "skill:deploy"));
}

/// Each refused by the code `edit --contents` refuses it with, and in its
/// words, beside whatever `validate` would say of the artifact holding them.
#[test]
fn what_an_edit_would_refuse_is_said_by_its_code_and_in_its_words() {
    let (_dir, project) = fixture();

    for (reference, contents, code, issues) in [
        (
            "skill:deploy",
            "---\nname: deploy\ndescription: \"\"\n---\n",
            "edit_contents_unfit",
            vec!["description_empty"],
        ),
        (
            "skill:deploy",
            "---\nname: ship\ndescription: Ship it\n---\n",
            "edit_name_changed",
            vec!["name_directory_mismatch"],
        ),
        (
            "skill:deploy",
            "no front matter at all\n",
            "edit_front_matter_unreadable",
            vec![],
        ),
        (
            "rule:git/commits",
            "---\npaths: src/**\n---\n\n",
            "edit_contents_unfit",
            vec!["content_empty"],
        ),
        ("skill:nowhere", "anything", "not_found", vec![]),
    ] {
        let reference = Reference::parse(reference);
        let judged = edited(None, &project, &reference, contents);

        let written = lifecycle::find_to_change(None, &project, &reference).and_then(|target| {
            lifecycle::replace_contents(None, &project, &target, contents.as_bytes(), None)
        });
        let written = written.map(|_| ()).expect_err("the edit wrote it");
        assert_eq!(refusal(&judged), Some(code), "{contents:?}");
        assert_eq!(written.code(), code, "{contents:?}");
        assert_eq!(
            judged.refused.as_ref().unwrap().message,
            written.to_string(),
            "{contents:?}"
        );
        assert_eq!(codes(&judged.issues), issues, "{contents:?}");
    }
}

/// The workspace's own are judged where `flayer edit` finds them: in its
/// library, held to the folder it keeps them in.
#[test]
fn the_workspaces_own_are_judged_in_its_library() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let own = workspace.own();
    lifecycle::add(
        Some(&workspace),
        &own,
        Kind::Skill,
        "deploy",
        "Ship it",
        None,
    )
    .unwrap();

    let fit = edited(
        Some(&workspace),
        &own,
        &Reference::parse("deploy"),
        "---\nname: deploy\ndescription: Ship it now\n---\n",
    );
    let taken = added(Some(&workspace), &own, Kind::Skill, "deploy", "Again", None);

    assert_eq!(fit.refused, None);
    assert_eq!(refusal(&taken), Some("add_exists"));
    assert!(!dir.path().join(MIND_DIR).exists());
}

#[test]
fn a_judgement_is_its_issues_and_why_it_is_refused_by_name() {
    let (_dir, project) = fixture();

    let judged = edited(
        None,
        &project,
        &Reference::parse("skill:deploy"),
        "---\nname: deploy\ndescription: \"\"\n---\n",
    );

    let json = serde_json::to_value(&judged).unwrap();
    assert_eq!(json["issues"][0]["code"], "description_empty");
    assert_eq!(json["issues"][0]["warning"], false);
    assert_eq!(json["refused"]["code"], "edit_contents_unfit");
    let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    keys.sort_unstable();
    assert_eq!(keys, ["issues", "refused"]);
    let mut keys: Vec<&String> = json["refused"].as_object().unwrap().keys().collect();
    keys.sort_unstable();
    assert_eq!(keys, ["code", "message"]);
}
