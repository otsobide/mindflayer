//! `mind migrate`, `flayer migrate -p` and `flayer migrate --all-projects`:
//! moving a project an older Mindflayer made from its `mind.toml` to a
//! `settings.toml`, and what is installed into it to where its agents read,
//! or from its `settings.json` to a `settings.toml` saying the same.

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
        "would write .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n\
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
        "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n\
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
        "nothing to migrate: collapse is marked by its settings.toml already\n"
    );
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
}

#[test]
fn flayer_migrate_moves_one_project_it_links_by_name() {
    let dir = workspace();
    let root = older_in(dir.path(), "collapse", &["deploy"]);

    let done = flayer(
        dir.path(),
        &["migrate", "-p", "collapse", "--agents", "claude", "--yes"],
    )
    .unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.toml for collapse: agents claude, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         copied deploy to .claude/skills/deploy, for claude\n\
         removed deploy from skills/deploy\n"
    );
    assert!(settings(&root).is_file());
    assert_eq!(recorded(dir.path(), "collapse"), [".claude/skills/deploy"]);
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
        "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n\
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
        "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n\
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
fn every_older_project_is_listed_without_yes_and_nothing_is_written() {
    let dir = workspace();
    let alpha = older_in(dir.path(), "alpha", &["deploy"]);
    let beta = older_in(dir.path(), "beta", &[]);
    let gamma = dir.path().join("gamma");
    fs::create_dir(&gamma).unwrap();
    new_project(&gamma);
    flayer(dir.path(), &["link", "gamma"]).unwrap();

    let dry = flayer(dir.path(), &["migrate", "--all-projects"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would write .mind/settings.toml for alpha: agents claude,codex,opencode, share shared\n\
         would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is\n\
         would copy deploy to .agents/skills/deploy, for codex and opencode\n\
         would copy deploy to .claude/skills/deploy, for claude\n\
         would remove deploy from skills/deploy\n\
         \n\
         would write .mind/settings.toml for beta: agents claude,codex,opencode, share shared\n\
         would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert!(dry.ok, "{:?}", dry.stderr);
    for root in [&alpha, &beta] {
        assert!(marker(root).is_file(), "{}", root.display());
        assert!(!settings(root).exists(), "{}", root.display());
    }
    assert!(alpha.join("skills/deploy/SKILL.md").is_file());
    assert!(!alpha.join(".claude").exists());
    assert_eq!(recorded(dir.path(), "alpha"), ["skills/deploy"]);
}

#[test]
fn every_older_project_is_moved_on_its_own_and_one_refused_is_named() {
    let dir = workspace();
    let alpha = older_in(dir.path(), "alpha", &["deploy"]);
    let beta = older_in(dir.path(), "beta", &[]);
    // A copy of the marker an earlier migration kept, which refuses beta.
    let kept = beta.join(MIND_DIR).join("tmp").join("mind.toml.v2");
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::write(&kept, "").unwrap();

    let done = flayer(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.toml for alpha: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         copied deploy to .agents/skills/deploy, for codex and opencode\n\
         copied deploy to .claude/skills/deploy, for claude\n\
         removed deploy from skills/deploy\n"
    );
    assert_eq!(
        done.stderr,
        [format!(
            "error: {}: nothing moved: {}: already there, kept by an earlier migration; move it \
             aside to migrate again",
            beta.display(),
            kept.display()
        )]
    );
    assert!(!done.ok);
    assert!(settings(&alpha).is_file());
    assert!(marker(&beta).is_file() && !settings(&beta).exists());

    let again = flayer(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();
    assert!(!again.ok, "beta is still refused");
    fs::remove_file(&kept).unwrap();
    flayer(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();
    let none = flayer(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();
    assert_eq!(
        none.stdout,
        "nothing to migrate: every project the workspace links is marked by its settings.toml already\n"
    );
}

#[test]
fn flayer_migrate_naming_nothing_says_what_there_is_to_migrate() {
    let dir = workspace();

    let failure = flayer(dir.path(), &["migrate", "--yes"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        "say what to migrate: --library moves the workspace's own into its library, -p <project> \
         moves a project it links from its mind.toml or settings.json to a settings.toml, and \
         --all-projects moves every one"
    );
    for refused in [
        &["migrate", "--library", "--all-projects"][..],
        &["migrate", "-p", "alpha", "--all-projects"],
        &["migrate", "--library", "--local"],
        &["migrate", "-p", "alpha", "--local", "--shared"],
    ] {
        assert!(
            FlayerCli::try_parse_from(line("flayer", refused, dir.path())).is_err(),
            "{refused:?}"
        );
    }
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
        "wrote .mind/settings.toml for collapse: agents claude, share local\n\
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
        "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared\n\
         moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n\
         left skills/notes where it is: the project's own\n"
    );
    assert_eq!(
        done.stderr,
        [
            "hint: no workspace here or above links this project, so nothing installed in it was \
             carried along; -W names one that links it from elsewhere"
        ]
    );
    assert!(settings(&root).is_file());
}

#[test]
fn a_project_linked_from_beside_is_told_how_to_reach_its_workspace() {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    fs::create_dir(&hub).unwrap();
    flayer(&hub, &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    older_project(&root);
    flayer(&hub, &["link", "../collapse"]).unwrap();
    flayer(&hub, &["add", "skill", "deploy", "What deploy does"]).unwrap();
    flayer(&hub, &["install", "-p", "collapse", "deploy"]).unwrap();

    // Walking up from the project never reaches it, so its records are not
    // read, and the hint names what does reach it.
    let unreached = mind(&root, &["migrate"]).unwrap();

    assert!(unreached
        .stdout
        .contains("would leave skills/deploy where it is: the project's own\n"));
    assert_eq!(
        unreached.stderr,
        [
            "hint: no workspace here or above links this project, so nothing installed in it \
             would be carried along; -W names one that links it from elsewhere"
        ]
    );

    let reached = mind(&root, &["migrate", "-W", hub.to_str().unwrap()]).unwrap();

    assert!(reached
        .stdout
        .contains("would remove deploy from skills/deploy\n"));
    assert!(reached.stderr.is_empty(), "{:?}", reached.stderr);
}

#[test]
fn a_project_the_workspace_above_does_not_link_is_told_how_to_link_it() {
    let dir = workspace();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    older_project(&root);

    let dry = mind(&root, &["migrate"]).unwrap();

    assert_eq!(
        dry.stderr,
        [format!(
            "hint: the workspace at {} does not link this project, so nothing installed in it \
             would be carried along; `mind link` links it, and -W names one that links it from \
             elsewhere",
            dir.path().display()
        )]
    );
}

#[test]
fn a_project_whose_workspace_cannot_be_read_is_not_moved() {
    let dir = workspace();
    let root = older_in(dir.path(), "collapse", &["deploy"]);
    let config = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    let written = fs::read_to_string(&config).unwrap();
    fs::write(&config, format!("{written}garbage = [\n")).unwrap();
    let before = fs::read(marker(&root)).unwrap();

    // Its records say deploy is the workspace's: moved without them, the
    // copy would be left as the project's own, and nothing carry it later.
    for line in [&["migrate"][..], &["migrate", "--yes"]] {
        let failure = mind(&root, line).unwrap_err();

        assert!(
            failure.error.to_string().starts_with(
                "nothing moved: what the workspace records of the project decides which of its \
                 copies move, and it cannot be read: "
            ),
            "{}",
            failure.error
        );
        assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
    }
    assert_eq!(fs::read(marker(&root)).unwrap(), before);
    assert!(!settings(&root).exists());
    assert!(root.join("skills").join("deploy").is_dir());

    fs::write(&config, written).unwrap();
    mind(&root, &["migrate", "--yes"]).unwrap();
    // Once it is, a moved project has nothing to read it for.
    let broken = fs::read_to_string(&config).unwrap();
    fs::write(&config, format!("{broken}garbage = [\n")).unwrap();
    let again = mind(&root, &["migrate", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to migrate: collapse is marked by its settings.toml already\n"
    );
    assert_eq!(again.stderr.len(), 1, "{:?}", again.stderr);
    assert!(again.stderr[0].starts_with("warning: "));
    assert!(!again.ok);
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

// ---------------------------------------------------------------------------
// From a settings.json
// ---------------------------------------------------------------------------

fn json(root: &Path) -> PathBuf {
    root.join(MIND_DIR).join(SETTINGS_JSON_FILE)
}

#[test]
fn mind_migrate_moves_a_settings_json_to_a_settings_toml_saying_the_same() {
    let dir = workspace();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    json_project(&root);
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    let before = fs::read_to_string(json(&root)).unwrap();
    let held = MindProject::open(&root)
        .unwrap()
        .settings()
        .unwrap()
        .clone();
    let copies = recorded(dir.path(), "collapse");

    // `--agents` and `--shared` are what a mind.toml is given: a
    // settings.json keeps its own, and says so on the first line.
    let dry = mind(&root, &["migrate", "--agents", "claude", "--shared"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would write .mind/settings.toml for collapse: agents claude,codex,opencode, share local\n\
         would move .mind/settings.json to .mind/tmp/settings.json.v3, as it is\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert!(dry.stderr.is_empty(), "{:?}", dry.stderr);
    assert!(dry.ok);
    assert_eq!(fs::read_to_string(json(&root)).unwrap(), before);
    assert!(!settings(&root).exists());

    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share local\n\
         moved .mind/settings.json to .mind/tmp/settings.json.v3, as it was\n"
    );
    assert!(done.stderr.is_empty(), "{:?}", done.stderr);
    assert!(done.ok);
    assert!(!json(&root).exists());
    assert_eq!(
        fs::read_to_string(root.join(MIND_DIR).join("tmp").join("settings.json.v3")).unwrap(),
        before
    );
    let moved = MindProject::open(&root).unwrap();
    assert_eq!(moved.settings().unwrap().id, held.id);
    assert_eq!(moved.settings().unwrap().agents, held.agents);
    assert_eq!(
        fs::read_to_string(settings(&root)).unwrap(),
        moved.settings().unwrap().text().unwrap()
    );
    // No copy moved, and none needed to: the agents are the ones they were.
    assert_eq!(recorded(dir.path(), "collapse"), copies);
    assert_eq!(
        mind(&root, &["status"]).unwrap().stdout,
        "deploy  installed  claude,codex,opencode  workspace\n"
    );
    // Local still, so git is offered nothing.
    assert!(untracked(&root).is_empty(), "{:?}", untracked(&root));

    let again = mind(&root, &["migrate", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to migrate: collapse is marked by its settings.toml already\n"
    );
}

#[test]
fn a_settings_json_moves_beside_a_workspace_that_cannot_be_read_and_says_so() {
    let dir = workspace();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    json_project(&root);
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    let config = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    let written = fs::read_to_string(&config).unwrap();
    fs::write(&config, format!("{written}garbage = [\n")).unwrap();

    // What it records moves nothing here, so it is not needed to go ahead.
    let done = mind(&root, &["migrate", "--yes"]).unwrap();

    assert!(
        done.stdout
            .starts_with("wrote .mind/settings.toml for collapse:"),
        "{}",
        done.stdout
    );
    assert_eq!(done.stderr.len(), 1, "{:?}", done.stderr);
    assert!(done.stderr[0].starts_with("warning: "), "{:?}", done.stderr);
    assert!(!done.ok);
    assert!(settings(&root).is_file() && !json(&root).exists());
}

#[test]
fn every_project_on_an_older_marker_is_moved_by_all_projects() {
    let dir = workspace();
    let alpha = older_in(dir.path(), "alpha", &[]);
    let beta = dir.path().join("beta");
    fs::create_dir(&beta).unwrap();
    json_project(&beta);
    flayer(dir.path(), &["link", "beta"]).unwrap();

    let dry = flayer(dir.path(), &["migrate", "--all-projects"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would write .mind/settings.toml for alpha: agents claude,codex,opencode, share shared\n\
         would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is\n\
         \n\
         would write .mind/settings.toml for beta: agents claude,codex,opencode, share local\n\
         would move .mind/settings.json to .mind/tmp/settings.json.v3, as it is\n\
         nothing changed: add --yes to go ahead\n"
    );

    flayer(dir.path(), &["migrate", "--all-projects", "--yes"]).unwrap();

    for root in [&alpha, &beta] {
        assert!(settings(root).is_file(), "{}", root.display());
        assert!(!marker(root).exists() && !json(root).exists());
    }
    let none = flayer(dir.path(), &["migrate", "-p", "beta", "--yes"]).unwrap();
    assert_eq!(
        none.stdout,
        "nothing to migrate: beta is marked by its settings.toml already\n"
    );
}
