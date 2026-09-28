//! Discovery of every kind, against real mind projects on disk.

use std::fs;

use mindflayer_core::{Catalog, DiscoveryFailure, Kind, MindProject, Reference, ValidationIssue};
use tempfile::TempDir;

/// One mind project per name, all under a single temporary parent so their
/// paths sort the way their names do.
fn projects(names: &[&str]) -> (TempDir, Vec<MindProject>) {
    let parent = TempDir::new().expect("create a temporary directory");
    let projects = names
        .iter()
        .map(|name| {
            let root = parent.path().join(name);
            fs::create_dir_all(&root).expect("create the project root");
            MindProject::init(&root)
                .expect("initialize the mind project")
                .0
        })
        .collect();
    (parent, projects)
}

fn project(name: &str) -> (TempDir, MindProject) {
    let (parent, mut projects) = projects(&[name]);
    (parent, projects.remove(0))
}

/// Write a skill: a directory holding SKILL.md.
fn write_skill(project: &MindProject, directory: &str, contents: &str) {
    let dir = project.directory_for(Kind::Skill).join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

/// Write a rule: one markdown file, at whatever depth `route` names.
fn write_rule(project: &MindProject, route: &str, contents: &str) {
    let path = project.directory_for(Kind::Rule).join(route);
    fs::create_dir_all(path.parent().unwrap()).expect("create the rule folder");
    fs::write(path, contents).expect("write the rule");
}

fn skill_file(name: &str) -> String {
    format!("---\nname: {name}\ndescription: Does {name} things\n---\n\n# {name}\n\nSteps.\n")
}

// ---------------------------------------------------------------------------
// Both kinds
// ---------------------------------------------------------------------------

#[test]
fn finds_every_kind_in_a_project() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "commit-style", &skill_file("commit-style"));
    write_rule(
        &one,
        "no-force-push.md",
        "# No force push\n\nUse --force-with-lease.\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.failures().is_empty());
    let found: Vec<(Kind, &str)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.kind(), artifact.name()))
        .collect();
    assert_eq!(
        found,
        vec![(Kind::Skill, "commit-style"), (Kind::Rule, "no-force-push")]
    );
    assert_eq!(catalog.kinds(), vec![Kind::Skill, Kind::Rule]);
}

#[test]
fn discovery_can_be_narrowed_to_one_kind() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "commit-style", &skill_file("commit-style"));
    write_rule(&one, "no-force-push.md", "Context.\n");

    let rules = Catalog::discover_kinds(std::slice::from_ref(&one), &[Kind::Rule]);

    assert_eq!(rules.kinds(), vec![Kind::Rule]);
    assert_eq!(rules.artifacts().len(), 1);
}

#[test]
fn artifacts_are_grouped_by_kind_then_named_in_order() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "zebra", &skill_file("zebra"));
    write_skill(&one, "alpaca", &skill_file("alpaca"));
    write_rule(&one, "yak.md", "Context.\n");
    write_rule(&one, "ant.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog.artifacts().iter().map(|a| a.name()).collect();
    // Blocks of like things, alphabetical within each, rather than an
    // alphabetical shuffle of two different sorts of thing.
    assert_eq!(found, vec!["alpaca", "zebra", "ant", "yak"]);
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

#[test]
fn a_nested_rule_is_named_by_its_route() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "git/no-force-push.md", "Never force-push.\n");
    write_rule(&one, "ci/no-force-push.md", "Nor in CI.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog.artifacts().iter().map(|a| a.name()).collect();
    // The stem alone would make these one name for two files; the route keeps
    // folders as grouping and nothing else.
    assert_eq!(found, vec!["ci/no-force-push", "git/no-force-push"]);
    assert!(catalog.failures().is_empty());
}

#[test]
fn a_rules_summary_is_its_opening_line() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "heading.md", "# Never force-push\n\nBody.\n");
    write_rule(&one, "prose.md", "\n\nJust prose, no heading.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let summaries: Vec<Option<&str>> = catalog.artifacts().iter().map(|a| a.summary()).collect();

    // The hashes are stripped, because a row of them summarises nothing.
    assert_eq!(
        summaries,
        vec![Some("Never force-push"), Some("Just prose, no heading.")]
    );
}

#[test]
fn a_rule_declares_nothing() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "context.md", "---\nname: not-front-matter\n---\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let rule = &catalog.artifacts()[0];

    assert_eq!(rule.description(), None);
    assert_eq!(rule.manifest(), None);
    // A rule has no front matter, so a leading fence is content like any
    // other line, not a manifest to be parsed.
    assert!(rule.contents().unwrap().starts_with("---"));
}

#[test]
fn an_empty_rule_is_the_one_thing_that_can_be_wrong_with_it() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "empty.md", "\n   \n\n");
    write_rule(&one, "fine.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let empty = &catalog.artifacts()[0];
    assert_eq!(empty.name(), "empty");
    assert_eq!(empty.validate(), vec![ValidationIssue::Empty]);
    assert!(catalog.artifacts()[1].validate().is_empty());
}

#[test]
fn a_rules_name_is_checked_segment_by_segment() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "Git/no-force-push.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let issues = catalog.artifacts()[0].validate();

    // The bad segment is named, not the whole route: `no-force-push` is fine.
    assert_eq!(
        issues,
        vec![ValidationIssue::NameNotKebabCase {
            segment: "Git".into()
        }]
    );
}

#[test]
fn only_markdown_files_are_rules_and_hidden_ones_are_not() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "real.md", "Context.\n");
    write_rule(&one, "notes.txt", "not a rule");
    write_rule(&one, ".hidden.md", "not a rule either");
    write_rule(&one, ".git/config.md", "nor this");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog.artifacts().iter().map(|a| a.name()).collect();
    assert_eq!(found, vec!["real"]);
    assert!(catalog.failures().is_empty());
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn a_skill_keeps_its_declared_name_and_its_directory_is_checked_against_it() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deployment"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let skill = &catalog.artifacts()[0];

    // The name comes from where it is declared. A rule declares nowhere, so
    // its name is its route; a skill declares one, so a mismatch with the
    // directory is a problem rather than a renaming.
    assert_eq!(skill.name(), "deployment");
    assert_eq!(
        skill.validate(),
        vec![ValidationIssue::NameDirectoryMismatch {
            name: "deployment".into(),
            directory: "deploy".into(),
        }]
    );
}

#[test]
fn a_skills_own_directory_is_never_walked_into() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));
    let assets = one.directory_for(Kind::Skill).join("deploy/references");
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("SKILL.md"), skill_file("nested")).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // The directory belongs to the skill, assets and all.
    assert_eq!(catalog.artifacts().len(), 1);
}

#[test]
fn one_broken_artifact_does_not_hide_the_others() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "good", &skill_file("good"));
    write_skill(&one, "no-front-matter", "# Just markdown\n");
    write_skill(&one, "unterminated", "---\nname: unterminated\n");
    write_rule(&one, "fine.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert_eq!(
        catalog.artifacts().len(),
        2,
        "the good ones are still listed"
    );
    assert_eq!(catalog.failures().len(), 2);
    for failure in catalog.failures() {
        assert!(matches!(failure, DiscoveryFailure::Artifact(_)));
    }
}

#[test]
fn a_project_with_no_folder_for_a_kind_is_not_a_failure() {
    let (_dir, one) = project("alpha");
    fs::remove_dir(one.directory_for(Kind::Rule)).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.is_empty());
    assert!(catalog.failures().is_empty());
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

#[test]
fn a_bare_name_finds_it_in_any_kind() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));
    write_rule(&one, "deploy.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let both = catalog.find(&Reference::parse("deploy"));
    assert_eq!(both.len(), 2);
    let just_the_rule = catalog.find(&Reference::parse("rule:deploy"));
    assert_eq!(just_the_rule.len(), 1);
    assert_eq!(just_the_rule[0].kind(), Kind::Rule);
    assert!(catalog.find(&Reference::parse("absent")).is_empty());
}

#[test]
fn a_qualifier_is_separated_by_a_colon_so_a_route_is_never_one() {
    // A rule's name IS a route, so `/` cannot also mean "kind". With `:`,
    // `skills/naming` is a rule filed under `skills/` and nothing else — and
    // a rules folder holding rules about writing skills is not exotic.
    for typed in ["git/no-force-push", "skills/naming", "rule/x"] {
        let bare = Reference::parse(typed);
        assert_eq!(bare.kind(), None, "{typed} was read as qualified");
        assert_eq!(bare.name(), typed);
        assert_eq!(bare.typed(), typed);
    }

    let qualified = Reference::parse("rule:git/no-force-push");
    assert_eq!(qualified.kind(), Some(Kind::Rule));
    assert_eq!(qualified.name(), "git/no-force-push");
    assert_eq!(qualified.typed(), "rule:git/no-force-push");
}

#[test]
fn a_rule_filed_under_a_kind_word_is_reachable_by_the_name_it_is_listed_under() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "skills/naming.md", "How to name a skill.\n");
    write_rule(&one, "naming.md", "Something else entirely.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // The name a listing prints has to be a name the tool accepts back, and
    // has to mean that artifact rather than a different one.
    let nested = catalog.find(&Reference::parse("skills/naming"));
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].summary(), Some("How to name a skill."));
    let other = catalog.find(&Reference::parse("naming"));
    assert_eq!(other[0].summary(), Some("Something else entirely."));
}

#[test]
fn find_spans_the_projects_a_workspace_manages() {
    let (_dir, made) = projects(&["alpha", "beta"]);
    write_rule(&made[0], "shared.md", "One.\n");
    write_rule(&made[1], "shared.md", "The other.\n");

    let catalog = Catalog::discover(&made);

    let matches = catalog.find(&Reference::parse("shared"));
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].project_name(), Some("alpha"));
    assert_eq!(matches[1].project_name(), Some("beta"));
}
