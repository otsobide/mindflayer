//! Discovery against real mind projects on disk.

use std::fs;
use std::path::Path;

use mindflayer_core::{Catalog, DiscoveryFailure, MindProject};
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
            let (project, _) = MindProject::init(&root).expect("initialize the mind project");
            project
        })
        .collect();
    (parent, projects)
}

/// A single fresh mind project.
fn project(name: &str) -> (TempDir, MindProject) {
    let (parent, mut projects) = projects(&[name]);
    (parent, projects.remove(0))
}

/// Write a skill directory holding `SKILL.md` with the given contents.
fn write_skill(project: &MindProject, directory: &str, contents: &str) {
    let dir = project.skills_dir().join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

/// A minimal well formed skill.
fn skill_file(name: &str) -> String {
    format!("---\nname: {name}\ndescription: Does {name} things\n---\n\n# {name}\n\nSteps.\n")
}

#[test]
fn finds_the_skills_in_every_project() {
    let (_dir, made) = projects(&["alpha", "beta"]);
    let (one, two) = (made[0].clone(), made[1].clone());
    write_skill(&one, "commit-style", &skill_file("commit-style"));
    write_skill(&two, "deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(&[one.clone(), two.clone()]);

    assert!(catalog.failures().is_empty());
    let found: Vec<(&str, Option<&str>)> = catalog
        .skills()
        .iter()
        .map(|skill| (skill.name(), skill.project_name()))
        .collect();
    assert_eq!(
        found,
        vec![("commit-style", Some("alpha")), ("deploy", Some("beta"))]
    );
    assert_eq!(catalog.in_project(one.root()).count(), 1);
}

#[test]
fn sorts_by_name_and_then_by_project() {
    let (_dir, made) = projects(&["alpha", "beta"]);
    let (one, two) = (made[0].clone(), made[1].clone());
    write_skill(&one, "zebra", &skill_file("zebra"));
    write_skill(&one, "shared", &skill_file("shared"));
    write_skill(&two, "shared", &skill_file("shared"));

    let catalog = Catalog::discover(&[two.clone(), one.clone()]);

    let found: Vec<&str> = catalog.skills().iter().map(|s| s.name()).collect();
    assert_eq!(found, vec!["shared", "shared", "zebra"]);
    // Discovery order does not decide the listing: the project paths do.
    assert!(catalog.skills()[0].project < catalog.skills()[1].project);
}

#[test]
fn a_project_without_a_skills_folder_is_not_a_failure() {
    let (_dir, one) = project("alpha");
    fs::remove_dir(one.skills_dir()).expect("remove the empty skills folder");

    let catalog = Catalog::discover(&[one]);

    assert!(catalog.is_empty());
    assert!(catalog.failures().is_empty());
}

#[test]
fn ignores_directories_and_files_that_are_not_skills() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "real", &skill_file("real"));
    fs::create_dir_all(one.skills_dir().join("assets")).unwrap();
    fs::write(one.skills_dir().join("assets/logo.png"), b"not a skill").unwrap();
    fs::write(one.skills_dir().join("README.md"), "loose file").unwrap();

    let catalog = Catalog::discover(&[one]);

    assert_eq!(catalog.skills().len(), 1);
    assert!(catalog.failures().is_empty());
}

#[test]
fn one_broken_skill_does_not_hide_the_others() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "good", &skill_file("good"));
    write_skill(&one, "no-front-matter", "# Just markdown\n");
    write_skill(&one, "unterminated", "---\nname: unterminated\n");
    write_skill(&one, "no-name", "---\ndescription: nameless\n---\n");

    let catalog = Catalog::discover(&[one]);

    assert_eq!(catalog.skills().len(), 1, "the good skill is still listed");
    assert_eq!(catalog.failures().len(), 3);
    for failure in catalog.failures() {
        assert!(
            matches!(failure, DiscoveryFailure::Skill(_)),
            "expected a per-skill failure, got {failure}"
        );
        assert!(failure.path().ends_with("SKILL.md"));
    }
}

#[test]
fn find_returns_every_project_declaring_the_name() {
    let (_dir, made) = projects(&["alpha", "beta"]);
    let (one, two) = (made[0].clone(), made[1].clone());
    write_skill(&one, "deploy", &skill_file("deploy"));
    write_skill(&two, "deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(&[one, two]);

    let matches = catalog.find("deploy");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].project_name(), Some("alpha"));
    assert_eq!(matches[1].project_name(), Some("beta"));
    assert!(catalog.find("absent").is_empty());
}

#[test]
fn a_skill_knows_its_path_and_its_instructions() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let skill = &catalog.skills()[0];

    assert_eq!(skill.path(), one.skills_dir().join("deploy/SKILL.md"));
    assert_eq!(skill.directory_name(), Some("deploy"));
    assert_eq!(skill.project.as_path(), one.root() as &Path);
    assert_eq!(skill.instructions().unwrap(), "\n# deploy\n\nSteps.\n");
    assert!(skill.validate().is_empty());
}

#[test]
fn a_name_that_does_not_match_its_directory_still_loads_but_does_not_validate() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deployment"));

    let catalog = Catalog::discover(&[one]);

    assert!(catalog.failures().is_empty());
    assert_eq!(catalog.skills()[0].validate().len(), 1);
}
