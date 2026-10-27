//! `mind migrate`: moving a project an older Mindflayer made from its
//! `mind.toml` to settings, and what is installed into it to where its
//! agents read.

mod common;

use common::*;

/// A workspace keeping each of `names` as a skill of its own, and `root`, a
/// repository a version 2 `mind init` made a project (`older_project`),
/// linked as `project`, with each of them installed into it: into its
/// `skills/`, where such a project takes its installs.
fn older_in(dir: &Path, project: &str, names: &[&str]) -> PathBuf {
    let root = dir.join(project);
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    older_project(&root);
    flayer(dir, &["link", project]).unwrap();
    for name in names {
        if !own_directory_for(dir, Kind::Skill).join(name).exists() {
            flayer(dir, &["add", "skill", name, &format!("What {name} does")]).unwrap();
        }
        flayer(dir, &["install", "-p", project, name]).unwrap();
    }
    root
}

/// A made workspace holding nothing yet.
fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    dir
}

fn marker(root: &Path) -> PathBuf {
    root.join(MIND_DIR).join(MIND_CONFIG)
}

fn settings(root: &Path) -> PathBuf {
    root.join(MIND_DIR).join(SETTINGS_FILE)
}

/// What the workspace at `dir` records of `name` in `project`, by path.
fn recorded(dir: &Path, project: &str) -> Vec<String> {
    FlayerWorkspace::open(dir)
        .unwrap()
        .ledger()
        .unwrap()
        .copies(project, Kind::Skill)
        .unwrap()
}

/// Put every file under `work_tree` in its repository's index, as `git add
/// -A` would: gix in process, no git.
fn indexed(work_tree: &Path) {
    let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
    let tree = write_tree(&repository, work_tree);
    let mut index = repository.index_from_tree(&tree).unwrap();
    index.write(Default::default()).unwrap();
}

#[test]
fn mind_migrate_says_what_it_would_move_then_moves_it_and_then_has_nothing_to_do() {
    let dir = workspace();
    let root = older_in(dir.path(), "collapse", &["deploy", "review"]);
    mind(&root, &["add", "skill", "notes", "Notes of my own"]).unwrap();
    let before = fs::read(marker(&root)).unwrap();

    let dry = mind(&root, &["migrate"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would write .mind/settings.json for collapse: agents claude,codex,opencode, share shared\n\
         would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is\n\
         would copy deploy to .agents/skills/deploy, for codex and opencode\n\
         would copy deploy to .claude/skills/deploy, for claude\n\
         would remove deploy from skills/deploy\n\
         would copy review to .agents/skills/review, for codex and opencode\n\
         would copy review to .claude/skills/review, for claude\n\
         would remove review from skills/review\n\
         would leave skills/notes where it is: the project's own\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert!(dry.stderr.is_empty(), "{:?}", dry.stderr);
    assert!(dry.ok);
    assert_eq!(fs::read(marker(&root)).unwrap(), before);
    assert!(!settings(&root).exists());
    assert_eq!(
        recorded(dir.path(), "collapse"),
        ["skills/deploy", "skills/review"]
    );

    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.json for collapse: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         copied deploy to .agents/skills/deploy, for codex and opencode\n\
         copied deploy to .claude/skills/deploy, for claude\n\
         removed deploy from skills/deploy\n\
         copied review to .agents/skills/review, for codex and opencode\n\
         copied review to .claude/skills/review, for claude\n\
         removed review from skills/review\n\
         left skills/notes where it is: the project's own\n"
    );
    assert!(done.ok, "{:?}", done.stderr);
    assert!(!marker(&root).exists());
    assert_eq!(
        fs::read(root.join(MIND_DIR).join("tmp").join("mind.toml.v2")).unwrap(),
        before
    );
    assert_eq!(
        recorded(dir.path(), "collapse"),
        [
            ".agents/skills/deploy",
            ".agents/skills/review",
            ".claude/skills/deploy",
            ".claude/skills/review",
        ]
    );
    // Listed from where its agents read, the project's own beside them.
    let status = mind(&root, &["status"]).unwrap();
    assert_eq!(
        status.stdout,
        "deploy  installed  claude,codex,opencode  workspace\n\
         notes   authored   -\n\
         review  installed  claude,codex,opencode  workspace\n"
    );

    let again = mind(&root, &["migrate", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to migrate: collapse is marked by its settings already\n"
    );
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
}

#[test]
fn a_project_already_keeping_its_skills_where_claude_reads_keeps_that_copy() {
    let dir = workspace();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    fs::create_dir(root.join(MIND_DIR)).unwrap();
    fs::write(
        marker(&root),
        "version = 2\nname = \"collapse\"\n\n[directories]\nskills = \".claude/skills\"\n",
    )
    .unwrap();
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();

    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.json for collapse: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         copied deploy to .agents/skills/deploy, for codex and opencode\n"
    );
    assert!(root.join(".claude/skills/deploy/SKILL.md").is_file());
    assert_eq!(
        recorded(dir.path(), "collapse"),
        [".agents/skills/deploy", ".claude/skills/deploy"]
    );
}

#[test]
fn a_copy_edited_since_it_was_installed_is_left_where_it_is_and_said() {
    let dir = workspace();
    let root = older_in(dir.path(), "collapse", &["deploy"]);
    let copy = root.join("skills/deploy/SKILL.md");
    fs::write(&copy, skill_file("deploy", "Changed by hand")).unwrap();

    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.json for collapse: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         left skills/deploy where it is, no longer Mindflayer's: it was changed since it was \
         installed\n"
    );
    assert_eq!(
        done.stderr,
        [
            "warning: deploy was not copied to .agents/skills/deploy: no copy of it in collapse \
             still holds what was installed",
            "warning: deploy was not copied to .claude/skills/deploy: no copy of it in collapse \
             still holds what was installed",
        ]
    );
    assert!(!done.ok);
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        skill_file("deploy", "Changed by hand")
    );
    assert!(recorded(dir.path(), "collapse").is_empty());
}

#[test]
fn a_local_migration_of_a_project_git_tracks_says_how_to_stop_tracking_its_mind() {
    let dir = workspace();
    let root = older_in(dir.path(), "collapse", &["deploy"]);
    // Committed by design, as a project from before settings was.
    indexed(&root);

    let done = mind(
        &root,
        &["migrate", "--local", "--agents", "claude", "--yes"],
    )
    .unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.json for collapse: agents claude, share local\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         wrote .mind/.gitignore, keeping .mind out of git\n\
         copied deploy to .claude/skills/deploy, for claude, kept out of git\n\
         removed deploy from skills/deploy\n"
    );
    assert_eq!(
        done.stderr,
        [format!(
            "hint: git tracks {}, and a .gitignore keeps out only what git does not track; \
             `git rm -r --cached .mind` in {} stops that, and leaves the files where they are",
            root.join(MIND_DIR).display(),
            root.display()
        )]
    );
    assert!(done.ok);
    // The copy is kept out of git; `.mind` would be too, once untracked.
    assert!(!untracked(&root).contains(&String::from(".claude/")));
}

#[test]
fn a_project_no_workspace_links_is_moved_and_told_nothing_was_carried() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    older_project(&root);
    write_skill(&root, "notes", &skill_file("notes", "Mine"));

    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.json for collapse: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         left skills/notes where it is: the project's own\n"
    );
    assert_eq!(
        done.stderr,
        ["hint: no workspace links this project, so nothing installed in it was carried along"]
    );
    assert!(settings(&root).is_file());
}

#[test]
fn a_migration_refused_says_every_reason_and_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("ancient");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    // A version 1 marker, which kept every kind inside `.mind`.
    fs::write(marker(&root), "version = 1\nname = \"ancient\"\n").unwrap();

    let failure = mind(&root, &["migrate"]).unwrap_err();

    assert!(
        failure
            .error
            .to_string()
            .starts_with("nothing moved: .mind/skills: cannot hold a project's skills"),
        "{}",
        failure.error
    );
    assert_eq!(failure.warnings.len(), 1, "{:?}", failure.warnings);
    assert!(failure.warnings[0].starts_with("error: nothing moved: .mind/rules: "));
    assert!(marker(&root).is_file() && !settings(&root).exists());
}
