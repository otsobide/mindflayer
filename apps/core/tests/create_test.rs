//! Creating a skill or a rule, from nothing or from a template.

use std::fs;
use std::path::Path;

use mindflayer_core::template::{self, Origin};
use mindflayer_core::{
    create, create_from, Catalog, CreateError, Directories, FlayerWorkspace, Kind, MindProject,
    TemplateError,
};
use tempfile::TempDir;

fn project() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    (dir, project)
}

fn write(root: &Path, route: &str, contents: &str) {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn a_created_skill_is_one_validate_accepts() {
    let (dir, project) = project();

    let skill = create(
        &project,
        Kind::Skill,
        "deploy",
        "Ship the service to staging",
    )
    .unwrap();

    assert_eq!(skill.name(), "deploy");
    assert_eq!(skill.description(), Some("Ship the service to staging"));
    assert_eq!(skill.validate(), vec![]);
    assert_eq!(
        skill.file(),
        dir.path().join("skills").join("deploy").join("SKILL.md")
    );
    // And a listing finds it, which is the point of creating it.
    let catalog = Catalog::discover(&[project]);
    assert_eq!(catalog.artifacts().len(), 1);
    assert!(catalog.failures().is_empty());
}

#[test]
fn a_description_that_looks_like_yaml_survives_as_typed() {
    let (_dir, project) = project();
    // A colon-space, a leading quote, a hash: each one breaks front matter
    // written by string formatting.
    let typed = "'Quoted' start: deploy # not a comment";

    let skill = create(&project, Kind::Skill, "deploy", typed).unwrap();

    assert_eq!(skill.description(), Some(typed));
}

#[test]
fn a_created_rule_is_filed_by_its_route() {
    let (dir, project) = project();

    let rule = create(
        &project,
        Kind::Rule,
        "git/no-force-push",
        "Never force-push a shared branch",
    )
    .unwrap();

    let file = dir
        .path()
        .join("rules")
        .join("git")
        .join("no-force-push.md");
    assert_eq!(rule.file(), file);
    assert_eq!(rule.name(), "git/no-force-push");
    assert_eq!(rule.summary(), Some("Never force-push a shared branch"));
    assert_eq!(rule.validate(), vec![]);
    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "# Never force-push a shared branch\n"
    );
}

#[test]
fn it_goes_where_the_project_keeps_that_kind() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init_with(
        dir.path(),
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    )
    .unwrap();

    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    assert!(dir.path().join(".claude/skills/deploy/SKILL.md").is_file());
}

#[test]
fn nothing_already_there_is_written_over() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "The first one").unwrap();
    create(&project, Kind::Rule, "style", "The first rule").unwrap();

    let skill = create(&project, Kind::Skill, "deploy", "A second one").unwrap_err();
    let rule = create(&project, Kind::Rule, "style", "A second rule").unwrap_err();

    assert!(matches!(skill, CreateError::Exists { .. }), "{skill}");
    assert!(matches!(rule, CreateError::Exists { .. }), "{rule}");
    let kept = fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap();
    assert!(kept.contains("The first one"));
}

#[test]
fn a_folder_that_is_not_a_skill_is_still_somebodys() {
    let (dir, project) = project();
    let folder = dir.path().join("skills").join("deploy");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("notes.txt"), "mine").unwrap();

    let error = create(&project, Kind::Skill, "deploy", "Ship it").unwrap_err();

    assert!(matches!(error, CreateError::Exists { .. }), "{error}");
    assert!(!folder.join("SKILL.md").exists());
    assert!(folder.join("notes.txt").is_file());
}

#[test]
fn a_name_validate_would_reject_is_refused_before_anything_is_written() {
    let (dir, project) = project();

    let error = create(&project, Kind::Skill, "Deploy Now", "Ship it").unwrap_err();

    assert!(matches!(error, CreateError::Name { .. }), "{error}");
    assert!(error.to_string().contains("Deploy Now"), "{error}");
    assert_eq!(fs::read_dir(dir.path().join("skills")).unwrap().count(), 0);
}

#[test]
fn a_rule_cannot_climb_out_of_its_folder() {
    let (dir, project) = project();

    for name in ["../escape", "git/../../escape", "/etc/escape"] {
        let error = create(&project, Kind::Rule, name, "Out").unwrap_err();
        assert!(matches!(error, CreateError::Name { .. }), "{name}: {error}");
    }
    assert!(!dir.path().join("escape.md").exists());
}

#[test]
fn a_skill_cannot_be_filed_in_a_folder() {
    let (_dir, project) = project();

    let error = create(&project, Kind::Skill, "tools/deploy", "Ship it").unwrap_err();

    assert!(matches!(error, CreateError::Nested { .. }), "{error}");
}

#[test]
fn the_description_is_one_line_that_says_something() {
    let (_dir, project) = project();

    let empty = create(&project, Kind::Rule, "style", "   ").unwrap_err();
    let two_lines = create(&project, Kind::Skill, "deploy", "One\nTwo").unwrap_err();
    let long = create(&project, Kind::Skill, "deploy", &"x".repeat(1025)).unwrap_err();

    assert!(matches!(
        empty,
        CreateError::DescriptionEmpty { kind: Kind::Rule }
    ));
    assert!(empty.to_string().contains("a rule needs a description"));
    assert!(matches!(two_lines, CreateError::DescriptionNotOneLine));
    assert!(matches!(
        long,
        CreateError::DescriptionTooLong { length: 1025 }
    ));
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[test]
fn the_built_in_full_template_brings_scripts_and_references() {
    let (dir, project) = project();
    let full = template::find(&project, Kind::Skill, "full").unwrap();
    assert_eq!(full.origin, Origin::BuiltIn);

    let skill = create_from(&project, Kind::Skill, "deploy", "Ship it", &full).unwrap();

    let folder = dir.path().join("skills/deploy");
    assert!(folder.join("scripts/.gitkeep").is_file());
    assert!(folder.join("references/.gitkeep").is_file());
    let manifest = fs::read_to_string(folder.join("SKILL.md")).unwrap();
    assert!(manifest.contains("# deploy\n\nShip it\n"), "{manifest}");
    assert!(manifest.contains("## Steps"), "{manifest}");
    assert_eq!(skill.validate(), vec![]);
}

#[test]
fn a_template_is_copied_whole_with_only_its_manifest_filled_in() {
    let (dir, project) = project();
    // `name: {{name}}` is not YAML anybody could parse; it is replaced before
    // anything tries, along with a description spread over several lines.
    write(
        dir.path(),
        ".mind/templates/runbook/SKILL.md",
        "---\n# runbooks are owned by ops\nname: {{name}}\ndescription: >\n  replaced\n  entirely\nallowed-tools: Bash, Read\n---\n\n# {{name}}\n\n{{description}}\n",
    );
    write(
        dir.path(),
        ".mind/templates/runbook/scripts/run.sh",
        "echo {{name}}\n",
    );
    let runbook = template::find(&project, Kind::Skill, "runbook").unwrap();

    let skill = create_from(
        &project,
        Kind::Skill,
        "deploy",
        "Ship: the fast way",
        &runbook,
    )
    .unwrap();

    let manifest = fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap();
    assert!(
        manifest.starts_with("---\nname: deploy\ndescription: 'Ship: the fast way'\n# runbooks are owned by ops\nallowed-tools: Bash, Read\n---\n"),
        "{manifest}"
    );
    assert!(
        manifest.ends_with("\n# deploy\n\nShip: the fast way\n"),
        "{manifest}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("skills/deploy/scripts/run.sh")).unwrap(),
        "echo {{name}}\n",
        "a script's placeholders are the script's business"
    );
    let tools = skill.manifest().unwrap().allowed_tools.clone();
    assert_eq!(
        tools,
        Some(vec![String::from("Bash"), String::from("Read")])
    );
}

#[test]
fn a_template_that_does_not_parse_writes_nothing() {
    let (dir, project) = project();
    write(
        dir.path(),
        ".mind/templates/broken/SKILL.md",
        "---\nallowed-tools: [unclosed\n---\n",
    );
    let broken = template::find(&project, Kind::Skill, "broken").unwrap();

    let error = create_from(&project, Kind::Skill, "deploy", "Ship it", &broken).unwrap_err();

    assert!(matches!(error, CreateError::Manifest { .. }), "{error}");
    assert!(!dir.path().join("skills/deploy").exists());
}

#[test]
fn a_rule_template_opens_with_the_description_unless_it_says_where_it_goes() {
    let (dir, project) = project();
    write(
        dir.path(),
        ".mind/templates/policy.md",
        "## Why\n\n## How\n",
    );
    write(
        dir.path(),
        ".mind/templates/placed.md",
        "# {{description}}\n\nFiled as {{name}}.\n",
    );

    let policy = template::find(&project, Kind::Rule, "policy").unwrap();
    let placed = template::find(&project, Kind::Rule, "placed").unwrap();
    create_from(
        &project,
        Kind::Rule,
        "git/no-force-push",
        "Never force-push",
        &policy,
    )
    .unwrap();
    create_from(&project, Kind::Rule, "style", "Write it plainly", &placed).unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("rules/git/no-force-push.md")).unwrap(),
        "# Never force-push\n\n## Why\n\n## How\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("rules/style.md")).unwrap(),
        "# Write it plainly\n\nFiled as style.\n"
    );
}

#[test]
fn the_closest_template_of_a_name_wins() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    write(
        dir.path(),
        ".mindflayer/templates/runbook/SKILL.md",
        "---\n---\nthe workspace's\n",
    );

    // Not managed, so the workspace's templates are not this project's.
    assert!(template::find(&project, Kind::Skill, "runbook").is_err());

    workspace.link(&project).unwrap();
    let from_workspace = template::find(&project, Kind::Skill, "runbook").unwrap();
    assert_eq!(
        from_workspace.origin,
        Origin::Path(dir.path().join(".mindflayer/templates/runbook"))
    );

    write(
        &root,
        ".mind/templates/runbook/SKILL.md",
        "---\n---\nthe project's\n",
    );
    write(&root, ".mind/templates/full/SKILL.md", "---\n---\nmine\n");
    let listed = template::templates(&project).unwrap();
    let origins: Vec<(&str, &Origin)> = listed
        .iter()
        .map(|template| (template.name.as_str(), &template.origin))
        .collect();
    assert_eq!(
        origins,
        vec![
            ("full", &Origin::Path(root.join(".mind/templates/full"))),
            (
                "runbook",
                &Origin::Path(root.join(".mind/templates/runbook"))
            ),
        ],
        "each name once, the closest one, the built-in `full` included"
    );
}

#[test]
fn a_template_is_asked_for_by_its_name_and_its_kind() {
    let (_dir, project) = project();

    let climbing = template::find(&project, Kind::Skill, "../../etc").unwrap_err();
    let wrong_kind = template::find(&project, Kind::Rule, "full").unwrap_err();
    let unknown = template::find(&project, Kind::Skill, "nope").unwrap_err();

    assert!(matches!(climbing, TemplateError::BadName { .. }));
    assert!(matches!(wrong_kind, TemplateError::WrongKind { .. }));
    assert!(wrong_kind.to_string().contains("is a skill template"));
    assert!(matches!(unknown, TemplateError::Unknown { .. }));
    assert!(unknown.to_string().contains("there are full"), "{unknown}");
}
