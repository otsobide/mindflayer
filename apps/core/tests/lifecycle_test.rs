//! Adding to what a project holds, and finding, renaming and removing what it
//! already does, and what each of those does to the ledger's records.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::install::{self, InstallError, Standing};
use mindflayer_core::ledger::{Ledger, SourceKind};
use mindflayer_core::lifecycle::{self, Record, Target};
use mindflayer_core::AgentHomes;
use mindflayer_core::{
    create, Agent, Catalog, Directories, FlayerWorkspace, Kind, LifecycleError, MindProject,
    Reference, Share,
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

fn find(project: &MindProject, typed: &str) -> Result<Target, LifecycleError> {
    lifecycle::find(project, &Reference::parse(typed))
}

// ---------------------------------------------------------------------------
// Finding
// ---------------------------------------------------------------------------

#[test]
fn an_artifact_is_found_by_what_it_declares() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let target = find(&project, "deploy").unwrap();

    assert_eq!(target.kind, Kind::Skill);
    assert_eq!(target.name, "deploy");
    assert!(target.artifact.is_some());
    assert_eq!(target.file(), dir.path().join("skills/deploy/SKILL.md"));
}

#[test]
fn a_skill_that_does_not_read_is_still_found_by_its_folder() {
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/broken/SKILL.md",
        "no front matter at all\n",
    );

    let target = find(&project, "broken").unwrap();

    // Left out of every listing, but it is the one that most needs opening.
    assert!(target.artifact.is_none());
    assert_eq!(target.file(), dir.path().join("skills/broken/SKILL.md"));
}

#[test]
fn a_folder_the_naming_rules_refuse_is_still_found_to_be_fixed() {
    // `a--b` was a name before the rules took doubled hyphens away, and a
    // folder of that name has to stay reachable: by its folder when it does
    // not read, and when what it declares is something else.
    let (dir, project) = project();
    write(dir.path(), "skills/a--b/SKILL.md", "no front matter\n");
    write(
        dir.path(),
        "skills/c--d/SKILL.md",
        "---\nname: cd\ndescription: Declared otherwise\n---\n",
    );

    let broken = find(&project, "a--b").unwrap();
    lifecycle::remove(None, &project, &broken).unwrap();
    assert!(!dir.path().join("skills/a--b").exists());

    let mismatched = find(&project, "c--d").unwrap();
    let renamed = lifecycle::rename(None, &project, &mismatched, "c-d").unwrap();
    assert_eq!(renamed.target.artifact.unwrap().validate(), vec![]);
    assert!(!dir.path().join("skills/c--d").exists());

    // A way out of the project is still not a place.
    assert!(matches!(
        find(&project, "../outside"),
        Err(LifecycleError::NotFound { .. })
    ));
}

#[test]
fn one_name_for_two_kinds_has_to_be_qualified() {
    let (_dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    create(&project, Kind::Rule, "deploy", "How we ship").unwrap();

    let error = find(&project, "deploy").unwrap_err();

    assert!(matches!(error, LifecycleError::Ambiguous { .. }), "{error}");
    assert!(error.to_string().contains("skill:deploy"), "{error}");
    assert!(error.to_string().contains("rule:deploy"), "{error}");
    assert_eq!(find(&project, "rule:deploy").unwrap().kind, Kind::Rule);
}

#[test]
fn a_name_that_climbs_out_is_never_turned_into_a_place() {
    let outer = TempDir::new().unwrap();
    let root = outer.path().join("project");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    write(
        outer.path(),
        "outside/SKILL.md",
        "---\nname: outside\ndescription: x\n---\n",
    );

    let error = find(&project, "../outside").unwrap_err();

    assert!(matches!(error, LifecycleError::NotFound { .. }), "{error}");
}

/// A project in a temporary folder, marked by its settings, naming every
/// agent and authoring skills in `skills`.
fn marked(skills: &str) -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let directories = Directories::default().with(Kind::Skill, skills);
    let (project, _) =
        MindProject::init_settings(dir.path(), &Agent::ALL, Share::Shared, &directories).unwrap();
    (dir, project)
}

const REVIEW: &str = "---\nname: review\ndescription: Reads the diff\n---\n";

#[test]
fn a_skill_only_in_an_agents_folder_is_listed_but_never_changed_from_here() {
    let (dir, project) = marked("skills");
    write(dir.path(), ".claude/skills/review/SKILL.md", REVIEW);

    let error = find(&project, "review").unwrap_err();

    assert!(
        matches!(error, LifecycleError::NotAuthored { .. }),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "`review` is at {}, in a folder this project's agents read and it does not author \
             in, so it is not changed from here",
            dir.path().join(".claude/skills/review").display()
        )
    );
    // Which a listing shows all the same.
    let listed = Catalog::discover(std::slice::from_ref(&project));
    assert_eq!(listed.artifacts()[0].name(), "review");
    assert!(matches!(
        find(&project, "absent"),
        Err(LifecycleError::NotFound { .. })
    ));
}

#[test]
fn a_project_authoring_in_an_agents_folder_changes_only_its_own_copy() {
    let (dir, project) = marked(".claude/skills");
    write(dir.path(), ".claude/skills/review/SKILL.md", REVIEW);
    write(dir.path(), ".agents/skills/review/SKILL.md", REVIEW);

    let target = find(&project, "review").unwrap();

    assert_eq!(target.path, dir.path().join(".claude/skills/review"));
    lifecycle::remove(None, &project, &target).unwrap();
    assert!(!dir.path().join(".claude/skills/review").exists());
    assert!(dir.path().join(".agents/skills/review/SKILL.md").is_file());
}

#[test]
fn a_file_that_cannot_be_read_is_identified_by_where_it_sits() {
    let (dir, project) = project();
    let root = dir.path();

    assert_eq!(
        lifecycle::identify(&project, &root.join("skills/broken/SKILL.md")),
        Some((Kind::Skill, String::from("broken")))
    );
    assert_eq!(
        lifecycle::identify(&project, &root.join("rules/git/no-force-push.md")),
        Some((Kind::Rule, String::from("git/no-force-push")))
    );
    // A skill's own files are the skill's, not artifacts of their own.
    assert_eq!(
        lifecycle::identify(&project, &root.join("skills/deploy/scripts/run.sh")),
        None
    );
    assert_eq!(lifecycle::identify(&project, &root.join("README.md")), None);
}

// ---------------------------------------------------------------------------
// Removing
// ---------------------------------------------------------------------------

#[test]
fn removing_a_skill_takes_its_whole_folder() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    write(dir.path(), "skills/deploy/scripts/run.sh", "echo ship\n");
    let target = find(&project, "deploy").unwrap();
    assert_eq!(lifecycle::files(&target), 2);

    lifecycle::remove(None, &project, &target).unwrap();

    assert!(!dir.path().join("skills/deploy").exists());
    assert!(dir.path().join("skills").is_dir());
}

#[test]
fn removing_a_rule_takes_the_folders_it_leaves_empty_and_no_more() {
    let (dir, project) = project();
    create(&project, Kind::Rule, "git/no-force-push", "Never").unwrap();
    create(&project, Kind::Rule, "ci/green", "Keep it green").unwrap();
    create(&project, Kind::Rule, "ci/fast", "Keep it fast").unwrap();

    lifecycle::remove(
        None,
        &project,
        &find(&project, "git/no-force-push").unwrap(),
    )
    .unwrap();
    lifecycle::remove(None, &project, &find(&project, "ci/green").unwrap()).unwrap();

    assert!(
        !dir.path().join("rules/git").exists(),
        "left empty, so gone"
    );
    assert!(
        dir.path().join("rules/ci/fast.md").is_file(),
        "still grouping one"
    );
    assert!(
        dir.path().join("rules").is_dir(),
        "the kind's own folder stays"
    );
}

// ---------------------------------------------------------------------------
// Renaming
// ---------------------------------------------------------------------------

const WRITTEN_BY_HAND: &str = "---\n# who owns this: platform\nname: deploy\ndescription: >\n  Ship the service\n  to staging.\nallowed-tools:\n  - Bash\n  - Read\n---\n\n# Deploying\n\nRun the pipeline.\n";

#[test]
fn renaming_a_skill_moves_its_folder_and_rewrites_nothing_but_its_name() {
    let (dir, project) = project();
    write(dir.path(), "skills/deploy/SKILL.md", WRITTEN_BY_HAND);
    write(dir.path(), "skills/deploy/scripts/run.sh", "echo ship\n");

    let renamed = lifecycle::rename(None, &project, &find(&project, "deploy").unwrap(), "ship")
        .unwrap()
        .target;

    assert_eq!(renamed.name, "ship");
    assert!(!dir.path().join("skills/deploy").exists());
    assert!(dir.path().join("skills/ship/scripts/run.sh").is_file());
    let manifest = fs::read_to_string(dir.path().join("skills/ship/SKILL.md")).unwrap();
    assert_eq!(
        manifest,
        WRITTEN_BY_HAND.replace("name: deploy\n", "name: ship\n")
    );
    assert_eq!(renamed.artifact.unwrap().validate(), vec![]);
}

#[test]
fn renaming_keeps_every_key_it_does_not_model_byte_for_byte() {
    // Claude Code's own keys, and metadata no agent should have written
    // nested, all pass through: they are somebody's, not Mindflayer's.
    const AGENT_KEYS: &str = "---\nname: deploy\ndescription: Ship it\ndisable-model-invocation: true\npaths:\n  - 'src/**'\nallowed-tools: Bash(git add *) Read\ncompatibility: Requires git\nmetadata:\n  author: acme\n  owner:\n    team: ops\n---\n\nRun the pipeline.\n";
    let (dir, project) = project();
    write(dir.path(), "skills/deploy/SKILL.md", AGENT_KEYS);

    lifecycle::rename(None, &project, &find(&project, "deploy").unwrap(), "ship").unwrap();

    let manifest = fs::read_to_string(dir.path().join("skills/ship/SKILL.md")).unwrap();
    assert_eq!(
        manifest,
        AGENT_KEYS.replace("name: deploy\n", "name: ship\n")
    );
}

#[test]
fn renaming_refuses_a_name_that_breaks_the_rules() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let error =
        lifecycle::rename(None, &project, &find(&project, "deploy").unwrap(), "a--b").unwrap_err();

    assert!(matches!(error, LifecycleError::Name { .. }), "{error}");
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
    assert!(!dir.path().join("skills/a--b").exists());
}

#[test]
fn renaming_refuses_a_skill_whose_description_breaks_the_rules() {
    // A rename writes only the name, but what it leaves behind is a skill
    // Mindflayer wrote, and an agent skips one with an empty description.
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/quiet/SKILL.md",
        "---\nname: quiet\ndescription: ''\n---\n",
    );

    let error =
        lifecycle::rename(None, &project, &find(&project, "quiet").unwrap(), "hush").unwrap_err();

    assert_eq!(
        error.to_string(),
        "`quiet` cannot be renamed until it is fixed: `description` is empty"
    );
    assert!(dir.path().join("skills/quiet/SKILL.md").is_file());
    assert!(!dir.path().join("skills/hush").exists());
}

#[test]
fn renaming_fixes_a_name_and_folder_that_broke_the_rules() {
    // What is wrong with the old name is what a rename is for, so it is the
    // skill as renamed that is judged.
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/a--b/SKILL.md",
        "---\nname: a--b\ndescription: Doubled\n---\n",
    );

    let renamed =
        lifecycle::rename(None, &project, &find(&project, "a--b").unwrap(), "a-b").unwrap();

    assert_eq!(renamed.target.artifact.unwrap().validate(), vec![]);
    assert!(!dir.path().join("skills/a--b").exists());
}

#[test]
fn a_name_yaml_would_read_as_something_else_is_quoted() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();

    let renamed = lifecycle::rename(None, &project, &find(&project, "deploy").unwrap(), "true")
        .unwrap()
        .target;

    assert_eq!(renamed.name, "true");
    let manifest = fs::read_to_string(dir.path().join("skills/true/SKILL.md")).unwrap();
    assert!(manifest.contains("name: 'true'"), "{manifest}");
}

#[test]
fn renaming_refuses_whatever_would_break_or_clobber_something() {
    let (dir, project) = project();
    create(&project, Kind::Skill, "deploy", "Ship it").unwrap();
    create(&project, Kind::Skill, "ship", "Already here").unwrap();
    let deploy = find(&project, "deploy").unwrap();

    let refusals = [
        lifecycle::rename(None, &project, &deploy, "Bad Name").unwrap_err(),
        lifecycle::rename(None, &project, &deploy, "tools/deploy").unwrap_err(),
        lifecycle::rename(None, &project, &deploy, "ship").unwrap_err(),
        lifecycle::rename(None, &project, &deploy, "deploy").unwrap_err(),
    ];

    assert!(matches!(refusals[0], LifecycleError::Name { .. }));
    assert!(matches!(refusals[1], LifecycleError::Nested { .. }));
    assert!(matches!(refusals[2], LifecycleError::Exists { .. }));
    assert!(matches!(refusals[3], LifecycleError::Unchanged { .. }));
    // And every one of them left both skills where they were.
    assert!(dir.path().join("skills/deploy/SKILL.md").is_file());
    let kept = fs::read_to_string(dir.path().join("skills/ship/SKILL.md")).unwrap();
    assert!(kept.contains("Already here"));
}

#[test]
fn a_skill_whose_name_is_not_one_line_is_left_for_a_person() {
    let (dir, project) = project();
    let written = "---\nname: >-\n  deploy\ndescription: Ship it\n---\n";
    write(dir.path(), "skills/deploy/SKILL.md", written);

    let error =
        lifecycle::rename(None, &project, &find(&project, "deploy").unwrap(), "ship").unwrap_err();

    assert!(matches!(error, LifecycleError::Rewrite { .. }), "{error}");
    assert!(!dir.path().join("skills/ship").exists(), "nothing moved");
    assert_eq!(
        fs::read_to_string(dir.path().join("skills/deploy/SKILL.md")).unwrap(),
        written
    );
}

#[test]
fn a_skill_that_does_not_read_is_edited_before_it_is_renamed() {
    let (dir, project) = project();
    write(dir.path(), "skills/broken/SKILL.md", "no front matter\n");

    let error =
        lifecycle::rename(None, &project, &find(&project, "broken").unwrap(), "fixed").unwrap_err();

    assert!(
        matches!(error, LifecycleError::Unreadable { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("mind edit"), "{error}");
}

#[test]
fn renaming_puts_a_folder_back_in_line_with_what_it_declares() {
    let (dir, project) = project();
    write(
        dir.path(),
        "skills/deploy/SKILL.md",
        "---\nname: deployment\ndescription: Ship it\n---\n",
    );
    let target = find(&project, "deploy").unwrap();
    assert_eq!(
        target.name, "deployment",
        "found by folder, named by itself"
    );

    let renamed = lifecycle::rename(None, &project, &target, "deployment")
        .unwrap()
        .target;

    assert!(dir.path().join("skills/deployment/SKILL.md").is_file());
    assert_eq!(renamed.artifact.unwrap().validate(), vec![]);
}

#[test]
fn renaming_a_rule_refiles_it_by_its_new_route() {
    let (dir, project) = project();
    create(
        &project,
        Kind::Rule,
        "git/no-force-push",
        "Never force-push",
    )
    .unwrap();

    let renamed = lifecycle::rename(
        None,
        &project,
        &find(&project, "git/no-force-push").unwrap(),
        "vcs/no-force-push",
    )
    .unwrap()
    .target;

    assert_eq!(renamed.name, "vcs/no-force-push");
    assert!(
        !dir.path().join("rules/git").exists(),
        "left empty, so gone"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("rules/vcs/no-force-push.md")).unwrap(),
        "# Never force-push\n"
    );
}

// ---------------------------------------------------------------------------
// The ledger's records
// ---------------------------------------------------------------------------

/// A workspace managing one project, `collapse`, from above it, with each of
/// `names` among the workspace's own skills and installed into the project.
fn installed(names: &[&str]) -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    workspace.link(&project).unwrap();
    for name in names {
        create(&workspace.own(), Kind::Skill, name, "From the workspace").unwrap();
    }
    let ledger = workspace.ledger().unwrap();
    for candidate in install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap() {
        install::install(
            &workspace,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    (dir, workspace, project, ledger)
}

/// The skills `ledger` says Mindflayer installed into `collapse`.
fn records(ledger: &Ledger) -> Vec<String> {
    ledger.installations("collapse", Kind::Skill).unwrap()
}

/// How every skill installed into the project stands now, by name.
fn standings(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
) -> Vec<(String, Standing)> {
    install::standings(workspace, ledger, project, Kind::Skill).unwrap()
}

/// How the workspace's own `name` stands with the project now.
fn standing(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
) -> Standing {
    install::survey(workspace, ledger, project, Kind::Skill)
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.name() == name)
        .unwrap_or_else(|| panic!("the workspace offers no {name}"))
        .standing
}

/// What somebody's editor does: add a line to the file.
fn type_into(file: &Path) {
    let mut text = fs::read_to_string(file).unwrap();
    text.push_str("Mine.\n");
    fs::write(file, text).unwrap();
}

/// A workspace with its own `names`, and a project marked by its settings,
/// naming every agent and authoring skills in `skills`, linked to it, with
/// each of the workspace's installed for its agents.
fn installed_for_agents(names: &[&str]) -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    installed_authoring_in("skills", Share::Shared, names)
}

/// [`installed_for_agents`], for a project at the root of a git repository
/// of its own that authors skills in `folder` and shares as `share` says.
fn installed_authoring_in(
    folder: &str,
    share: Share,
    names: &[&str],
) -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let directories = Directories::default().with(Kind::Skill, folder);
    let (project, _) = MindProject::init_settings(&root, &Agent::ALL, share, &directories).unwrap();
    workspace.link(&project).unwrap();
    for name in names {
        create(&workspace.own(), Kind::Skill, name, "From the workspace").unwrap();
    }
    let ledger = workspace.ledger().unwrap();
    for candidate in install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap() {
        install::install(
            &workspace,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    (dir, workspace, project, ledger)
}

#[test]
fn what_a_project_authors_beside_its_agents_copies_leaves_their_record_alone() {
    let (dir, workspace, project, ledger) = installed_for_agents(&["deploy"]);
    let installed = dir.path().join("collapse/.claude/skills/deploy");
    assert!(installed.is_dir());

    // Its own of the same name, in the folder it authors in.
    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "deploy",
        "Our own",
        None,
    )
    .unwrap();
    assert_eq!(added.stale.unwrap(), Record::Untouched);
    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());
    type_into(edit.file());
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);
    assert_eq!(edited.copy.unwrap(), Record::Untouched);
    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
        "ship",
    )
    .unwrap();
    assert_eq!(renamed.copy.unwrap(), Record::Untouched);
    let removed =
        lifecycle::remove(Some(&workspace), &project, &find(&project, "ship").unwrap()).unwrap();
    assert_eq!(removed.copy.unwrap(), Record::Untouched);

    // The copies installed for its agents are still Mindflayer's to manage.
    assert_eq!(records(&ledger), ["deploy"]);
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Current
    );
    assert!(installed.join("SKILL.md").is_file());
}

/// The routes the exclude block keeps out of git for the project.
fn excluded(project: &MindProject) -> Vec<String> {
    fs::read_to_string(project.root().join(".git/info/exclude"))
        .unwrap()
        .lines()
        .filter(|line| line.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn removing_the_copy_in_a_folder_the_project_authors_in_leaves_the_others_installed() {
    // Claude's folder is the one it authors in, so an install's copy there
    // is one a change from the project reaches.
    let (dir, workspace, project, ledger) =
        installed_authoring_in(".claude/skills", Share::Local, &["deploy"]);
    let reached = dir.path().join("collapse/.claude/skills/deploy");
    let other = dir.path().join("collapse/.agents/skills/deploy");
    assert!(reached.is_dir() && other.is_dir());

    let removed = lifecycle::remove(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
    )
    .unwrap();

    assert_eq!(removed.copy.unwrap(), Record::Dropped);
    assert!(!reached.exists());
    // The copy nobody touched is still Mindflayer's, and so still out of git;
    // the place the change emptied is not.
    let record = ledger
        .installation("collapse", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    let routes: Vec<&str> = record.targets.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(routes, [".agents/skills/deploy"]);
    assert_eq!(excluded(&project), ["/.agents/skills/deploy/"]);
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Current
    );

    // What the project writes there next is its own, in git's sight, and
    // refuses an install rather than being written over.
    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "deploy",
        "Our own",
        None,
    )
    .unwrap();
    assert_eq!(added.stale.unwrap(), Record::Untouched);
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Foreign
    );

    // And the copy left can still be taken out, without the project's own.
    let uninstalled = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();
    assert!(
        matches!(uninstalled, install::Removed::Removed { .. }),
        "{uninstalled:?}"
    );
    assert!(!other.exists());
    assert!(reached.join("SKILL.md").is_file());
    assert!(excluded(&project).is_empty());
}

#[test]
fn an_exclude_block_that_cannot_be_edited_is_reported_and_the_copys_record_still_goes() {
    let (_dir, workspace, project, ledger) =
        installed_authoring_in(".claude/skills", Share::Local, &["deploy"]);
    let file = project.root().join(".git/info/exclude");
    let mut text = fs::read_to_string(&file).unwrap();
    text.push_str("# mindflayer:begin\n");
    fs::write(&file, &text).unwrap();

    let removed = lifecycle::remove(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
    )
    .unwrap();

    let Err(error) = removed.copy else {
        panic!("{:?}", removed.copy);
    };
    assert!(
        matches!(error, InstallError::StillExcluded { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("was dropped"), "{error}");
    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        [".agents/skills/deploy"]
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn editing_or_renaming_the_copy_in_a_folder_the_project_authors_in_keeps_every_record() {
    let (dir, workspace, project, ledger) =
        installed_authoring_in(".claude/skills", Share::Shared, &["deploy", "audit"]);

    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());
    type_into(edit.file());
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);
    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "audit").unwrap(),
        "review",
    )
    .unwrap();

    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        [
            ".agents/skills/audit",
            ".agents/skills/deploy",
            ".claude/skills/deploy",
            ".claude/skills/review"
        ]
    );
    // Nothing a shared project installs is kept out of git.
    assert!(excluded(&project).is_empty());
    // Each change reads as an edit of what was installed; the copy nobody
    // touched is still installed as it was.
    assert_eq!(
        standings(&workspace, &ledger, &project),
        [
            (String::from("audit"), Standing::Current),
            (String::from("deploy"), Standing::Edited),
            (String::from("review"), Standing::Edited),
        ]
    );
    assert!(dir.path().join("collapse/.agents/skills/audit").is_dir());
    let uninstalled = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "audit",
        &AgentHomes::default(),
    )
    .unwrap();
    assert!(
        matches!(uninstalled, install::Removed::Removed { .. }),
        "{uninstalled:?}"
    );
}

#[test]
fn a_renamed_copy_in_a_local_project_is_kept_out_of_git_where_it_went() {
    let (_dir, workspace, project, _ledger) =
        installed_authoring_in(".claude/skills", Share::Local, &["audit"]);
    assert_eq!(
        excluded(&project),
        ["/.agents/skills/audit/", "/.claude/skills/audit/"]
    );

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "audit").unwrap(),
        "review",
    )
    .unwrap();

    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    assert_eq!(
        excluded(&project),
        ["/.agents/skills/audit/", "/.claude/skills/review/"]
    );
}

#[test]
fn a_copy_renamed_onto_a_name_installed_from_elsewhere_is_its_authors() {
    let (_dir, workspace, project, ledger) =
        installed_authoring_in(".claude/skills", Share::Local, &["deploy", "ship"]);
    // The project's own copy of `ship` gone by hand, the one for the other
    // agents still installed.
    fs::remove_dir_all(project.root().join(".claude/skills/ship")).unwrap();

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
        "ship",
    )
    .unwrap();

    // The record of the copy gone is dropped first, and `ship` is still
    // installed in `.agents/skills`: one name, one installation, so the
    // renamed copy cannot join it.
    assert_eq!(renamed.stale.unwrap(), Record::Dropped);
    assert_eq!(renamed.copy.unwrap(), Record::Dropped);
    assert_eq!(
        ledger.copies("collapse", Kind::Skill).unwrap(),
        [".agents/skills/deploy", ".agents/skills/ship"]
    );
    assert_eq!(
        excluded(&project),
        ["/.agents/skills/deploy/", "/.agents/skills/ship/"]
    );
    assert_eq!(
        standing(&workspace, &ledger, &project, "ship"),
        Standing::Foreign
    );
}

/// A workspace managing `collapse`, a project a `mind.toml` marks, with the
/// workspace's own rule `git/no-force-push` copied into it.
fn installed_rule() -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let (dir, workspace, project, ledger) = installed(&[]);
    let own = workspace.own().directory_for(Kind::Rule).unwrap();
    write(&own, "git/no-force-push.md", "Never force-push.\n");
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "git/no-force-push",
        None,
    )
    .unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    (dir, workspace, project, ledger)
}

fn rule_standings(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
) -> Vec<(String, Standing)> {
    install::standings(workspace, ledger, project, Kind::Rule).unwrap()
}

#[test]
fn an_installed_rules_copy_edited_or_renamed_keeps_its_record() {
    let (dir, workspace, project, ledger) = installed_rule();

    let edit = lifecycle::start_edit(&find(&project, "git/no-force-push").unwrap());
    type_into(edit.file());
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(
        rule_standings(&workspace, &ledger, &project),
        [(String::from("git/no-force-push"), Standing::Edited)]
    );

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "git/no-force-push").unwrap(),
        "git/never-force-push",
    )
    .unwrap();

    // A rule's name is its route, never in its file: renamed, it holds what
    // it held, the edit and all, and nothing offers it by its new route.
    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    let record = ledger
        .installation("collapse", Kind::Rule, "git/never-force-push")
        .unwrap()
        .unwrap();
    assert_eq!(record.targets[0].path, "rules/git/never-force-push.md");
    assert_eq!(
        rule_standings(&workspace, &ledger, &project),
        [(String::from("git/never-force-push"), Standing::Edited)]
    );
    assert!(dir
        .path()
        .join("collapse/rules/git/never-force-push.md")
        .is_file());
}

#[test]
fn a_renamed_rules_copy_nobody_edited_is_orphaned_and_taken_out_unforced() {
    let (_dir, workspace, project, ledger) = installed_rule();

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "git/no-force-push").unwrap(),
        "git/never-force-push",
    )
    .unwrap();

    // Moved as it was, so it holds what was installed, and says so.
    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: false });
    assert_eq!(
        rule_standings(&workspace, &ledger, &project),
        [(String::from("git/never-force-push"), Standing::Orphaned)]
    );
    let removed = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "git/never-force-push",
        &AgentHomes::default(),
    )
    .unwrap();
    assert!(
        matches!(removed, install::Removed::Removed { .. }),
        "{removed:?}"
    );
}

#[test]
fn adding_where_an_installed_copy_was_deleted_by_hand_drops_its_record() {
    let (dir, workspace, project, ledger) = installed(&["deploy"]);
    fs::remove_dir_all(dir.path().join("collapse/skills/deploy")).unwrap();

    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "deploy",
        "Mine now",
        None,
    )
    .unwrap();

    // Left in place, the record would make what was just written look like
    // the workspace's to overwrite.
    assert_eq!(added.stale.unwrap(), Record::Dropped);
    assert_eq!(records(&ledger), Vec::<String>::new());
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Foreign
    );
}

#[test]
fn adding_a_rule_beside_an_installed_one_that_reaches_no_agent_keeps_its_record() {
    // A project naming only Codex, given a rule about some files, which only
    // Claude reads: installed with no target at all.
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let directories = Directories::default().with(Kind::Rule, "rules");
    let (project, _) =
        MindProject::init_settings(&root, &[Agent::Codex], Share::Shared, &directories).unwrap();
    workspace.link(&project).unwrap();
    let own = workspace.own().directory_for(Kind::Rule).unwrap();
    write(
        &own,
        "rust/errors.md",
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors.\n",
    );
    let ledger = workspace.ledger().unwrap();
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "rust/errors",
        None,
    )
    .unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    let recorded = ledger
        .installation("collapse", Kind::Rule, "rust/errors")
        .unwrap()
        .unwrap();
    assert_eq!(recorded.targets, []);

    // Its own of the same route, in the folder it authors in.
    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Rule,
        "rust/errors",
        "Our own",
        None,
    )
    .unwrap();

    assert_eq!(added.stale.unwrap(), Record::Untouched);
    assert_eq!(
        ledger
            .installation("collapse", Kind::Rule, "rust/errors")
            .unwrap(),
        Some(recorded)
    );
}

#[test]
fn adding_touches_no_record_but_one_under_the_name_it_writes() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);

    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "notes",
        "Mine",
        None,
    )
    .unwrap();

    assert_eq!(added.stale.unwrap(), Record::Untouched);
    assert_eq!(records(&ledger), ["deploy"]);
}

#[test]
fn a_change_asks_only_a_workspace_that_manages_the_project_and_creates_no_ledger() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();

    let unmanaged = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "deploy",
        "Ship it",
        None,
    )
    .unwrap();
    workspace.link(&project).unwrap();
    let managed = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Rule,
        "deploy",
        "How we ship",
        None,
    )
    .unwrap();
    let alone = lifecycle::add(None, &project, Kind::Skill, "notes", "Mine", None).unwrap();

    for added in [unmanaged, managed, alone] {
        assert_eq!(added.stale.unwrap(), Record::Untouched);
    }
    // A workspace that never installed anything has no ledger, and looking
    // must not give it one.
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn a_record_that_cannot_be_dropped_is_reported_and_the_change_stands() {
    let (dir, workspace, project, _ledger) = installed(&["deploy"]);
    fs::remove_dir_all(dir.path().join("collapse/skills/deploy")).unwrap();
    // A ledger a newer Mindflayer wrote is refused, so nothing in it can be
    // changed from here.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();

    let added = lifecycle::add(
        Some(&workspace),
        &project,
        Kind::Skill,
        "deploy",
        "Mine now",
        None,
    )
    .unwrap();

    assert!(
        matches!(added.stale, Err(InstallError::Ledger(_))),
        "{:?}",
        added.stale
    );
    assert!(added.artifact.file().is_file());
}

#[test]
fn an_edit_that_changes_an_installed_copy_keeps_its_record_and_reads_as_edited() {
    let (dir, workspace, project, ledger) = installed(&["deploy"]);
    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());
    type_into(edit.file());

    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    assert!(edited.changed);
    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(records(&ledger), ["deploy"]);
    // Still installed, and edited: shown, never overwritten or deleted by
    // unticking it unless forced.
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Edited
    );
    assert_eq!(
        install::uninstall(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            &AgentHomes::default()
        )
        .unwrap(),
        install::Removed::Edited {
            name: String::from("deploy"),
            paths: vec![project.root().join("skills").join("deploy")],
        }
    );
    assert!(dir.path().join("collapse/skills/deploy/SKILL.md").is_file());
}

#[test]
fn an_edit_that_changes_nothing_keeps_the_record() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);
    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());

    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    assert!(!edited.changed);
    assert_eq!(edited.copy.unwrap(), Record::Untouched);
    assert_eq!(records(&ledger), ["deploy"]);
}

#[test]
fn renaming_an_installed_copy_carries_its_record_and_reads_as_edited() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
        "ship",
    )
    .unwrap();

    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    assert_eq!(renamed.stale.unwrap(), Record::Untouched);
    assert_eq!(records(&ledger), ["ship"]);
    let record = ledger
        .installation("collapse", Kind::Skill, "ship")
        .unwrap()
        .unwrap();
    assert_eq!(record.origin.as_deref(), Some("workspace"));
    assert_eq!(record.targets[0].path, "skills/ship");
    // Its name line is not what was installed.
    assert_eq!(
        standings(&workspace, &ledger, &project),
        [(String::from("ship"), Standing::Edited)]
    );
}

#[test]
fn renaming_onto_a_name_an_old_record_claims_drops_that_record_too() {
    let (dir, workspace, project, ledger) = installed(&["deploy", "ship"]);
    fs::remove_dir_all(dir.path().join("collapse/skills/ship")).unwrap();

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
        "ship",
    )
    .unwrap();

    // Left in place, it would make what was just renamed look like the
    // workspace's own `ship` as it was installed, to overwrite.
    assert_eq!(renamed.stale.unwrap(), Record::Dropped);
    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    assert_eq!(
        standing(&workspace, &ledger, &project, "ship"),
        Standing::Edited
    );
}

#[test]
fn removing_an_installed_copy_uninstalls_it() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);

    let removed = lifecycle::remove(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
    )
    .unwrap();

    assert_eq!(removed.copy.unwrap(), Record::Dropped);
    assert_eq!(records(&ledger), Vec::<String>::new());
}

/// What an older build could have left in the same ledger: a source whose URL
/// carries a token. Returns where its clone would be.
fn tainted(workspace: &FlayerWorkspace, ledger: &Ledger) -> PathBuf {
    let source = ledger
        .source_for(
            SourceKind::Git,
            "https://example.com/acme/skills",
            None,
            "skills",
        )
        .unwrap();
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
    workspace.cache_dir().join(source.directory)
}

#[test]
fn a_change_that_drops_a_record_hands_back_what_scrubbing_the_ledger_took_out() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);
    tainted(&workspace, &ledger);

    let removed = lifecycle::remove(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
    )
    .unwrap();

    assert_eq!(removed.copy.unwrap(), Record::Dropped);
    let scrubbed: Vec<&str> = removed
        .scrub
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(scrubbed, ["https://example.com/acme/skills"]);
    assert_eq!(ledger.tainted_sources().unwrap(), []);
}

#[test]
fn an_edit_leaves_the_ledger_unscrubbed() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);
    tainted(&workspace, &ledger);
    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());
    type_into(edit.file());

    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    // The record stays as it was, so there was no reason to write.
    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(ledger.tainted_sources().unwrap().len(), 1);
}

// Unix only: what listing a file as a directory fails with is the platform's
// to say, and only there is it sure not to read as an absence.
#[cfg(unix)]
#[test]
fn a_rename_tells_once_of_a_clone_neither_of_its_scrubs_could_rewrite() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);
    let clone = tainted(&workspace, &ledger);
    // A file where the scrub lists the reflogs: the clone keeps its
    // credential, so its source keeps its URL for every scrub to try again.
    fs::create_dir_all(clone.join(".git")).unwrap();
    fs::write(clone.join(".git").join("logs"), "").unwrap();

    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "deploy").unwrap(),
        "ship",
    )
    .unwrap();

    assert_eq!(renamed.copy.unwrap(), Record::Moved { edited: true });
    // The ledger was opened once for each name, and both scrubs failed on the
    // same clone.
    let failed: Vec<&str> = renamed
        .scrub
        .failures
        .iter()
        .map(|failure| failure.url.as_str())
        .collect();
    assert_eq!(failed, ["https://example.com/acme/skills"]);
    assert!(renamed.scrub.sources.is_empty());
    assert_eq!(ledger.tainted_sources().unwrap().len(), 1);
}

#[test]
fn changing_something_mindflayer_did_not_install_touches_no_record() {
    let (_dir, workspace, project, ledger) = installed(&["deploy"]);
    create(&workspace.own(), Kind::Skill, "notes", "The workspace's").unwrap();
    create(&project, Kind::Skill, "notes", "Mine").unwrap();
    assert_eq!(
        standing(&workspace, &ledger, &project, "notes"),
        Standing::Foreign
    );

    let edit = lifecycle::start_edit(&find(&project, "notes").unwrap());
    type_into(edit.file());
    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);
    let renamed = lifecycle::rename(
        Some(&workspace),
        &project,
        &find(&project, "notes").unwrap(),
        "jots",
    )
    .unwrap();
    let removed =
        lifecycle::remove(Some(&workspace), &project, &find(&project, "jots").unwrap()).unwrap();

    assert!(edited.changed);
    for record in [edited.copy, renamed.copy, renamed.stale, removed.copy] {
        assert_eq!(record.unwrap(), Record::Untouched);
    }
    // Nor the record of what was installed beside it.
    assert_eq!(records(&ledger), ["deploy"]);
}

#[test]
fn a_change_sees_to_the_record_of_the_workspace_it_is_given_wherever_that_sits() {
    // Beside the project rather than above it, the way the one in your home
    // is for a project anywhere else: walking up from the project never
    // reaches it.
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    fs::create_dir(&hub).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(&hub).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    workspace.link(&project).unwrap();
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let ledger = workspace.ledger().unwrap();
    for candidate in install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap() {
        install::install(
            &workspace,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    let edit = lifecycle::start_edit(&find(&project, "deploy").unwrap());
    type_into(edit.file());

    let edited = lifecycle::finish_edit(Some(&workspace), &project, edit);

    assert_eq!(edited.copy.unwrap(), Record::Kept);
    assert_eq!(
        standing(&workspace, &ledger, &project, "deploy"),
        Standing::Edited
    );
}

#[cfg(unix)]
#[test]
fn a_record_is_kept_under_the_workspaces_spelling_of_a_project_whatever_the_symlinks() {
    // A workspace named with `-W` keeps the spelling it was given, and `mind`
    // spells its project the way the operating system does, through no
    // symlink: `/tmp` is one on every Mac. Either way round, the record is
    // the one `flayer -p` reads, under the entry the workspace stores.
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    fs::create_dir_all(real.join("collapse")).unwrap();
    std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(real.join("hub")).unwrap();
    let (project, _) = MindProject::init(real.join("collapse")).unwrap();
    workspace.link(&project).unwrap();
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let through_link = FlayerWorkspace::open(dir.path().join("link/hub")).unwrap();
    let ledger = workspace.ledger().unwrap();
    let recorded = || ledger.installations("../collapse", Kind::Skill).unwrap();

    for candidate in install::survey(&through_link, &ledger, &project, Kind::Skill).unwrap() {
        install::install(
            &through_link,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    assert_eq!(recorded(), ["deploy"]);

    let linked = MindProject::open(dir.path().join("link/collapse")).unwrap();
    let removed =
        lifecycle::remove(Some(&workspace), &linked, &find(&linked, "deploy").unwrap()).unwrap();

    assert_eq!(removed.copy.unwrap(), Record::Dropped);
    assert_eq!(recorded(), Vec::<String>::new());
}
