//! `targets`: installing a kind into folders of a project's own, for every
//! agent it names, or back where each agent reads, and carrying what is
//! installed into it along, at both levels.

mod common;

use common::*;

/// A workspace keeping `deploy` of its own, and `collapse`, a repository
/// made a project naming every agent and linked, with `deploy` installed
/// where they read.
fn installed() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    new_project(&root);
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    (dir, root)
}

fn settings(root: &Path) -> String {
    fs::read_to_string(root.join(MIND_DIR).join(SETTINGS_FILE)).unwrap()
}

/// What the workspace at `dir` records of `deploy` in `collapse`, by path.
fn recorded(dir: &Path) -> Vec<String> {
    FlayerWorkspace::open(dir)
        .unwrap()
        .ledger()
        .unwrap()
        .copies("collapse", Kind::Skill)
        .unwrap()
}

/// What `status` says of a project installing skills in `.skills` alone.
const UNREAD: &str = "claude, codex and opencode read none of the folders collapse installs \
                      skills in (.skills), so none of them sees what is installed there until \
                      pointed at one";

#[test]
fn setting_targets_says_what_it_would_move_then_moves_every_copy_there() {
    let (dir, root) = installed();
    let before = settings(&root);

    let dry = mind(&root, &["targets", "set", "skills", ".skills"]).unwrap();

    assert_eq!(
        dry.stdout,
        "would set the targets of skills to .skills in collapse, where they were the folders its \
         agents read\n\
         would copy deploy to .skills/deploy, for claude, codex and opencode, kept out of git\n\
         would remove deploy from .agents/skills/deploy\n\
         would remove deploy from .claude/skills/deploy\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert_eq!(
        dry.stderr,
        [
            "note: claude, codex and opencode would read none of the folders collapse would \
          install skills in (.skills), so none of them would see what is installed there until \
          pointed at one"
        ]
    );
    assert!(dry.ok);
    assert_eq!(settings(&root), before);
    assert_eq!(
        recorded(dir.path()),
        [".agents/skills/deploy", ".claude/skills/deploy"]
    );

    let done = mind(&root, &["targets", "set", "skill", "./.skills/", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "set the targets of skills to .skills in collapse, where they were the folders its agents \
         read\n\
         copied deploy to .skills/deploy, for claude, codex and opencode, kept out of git\n\
         removed deploy from .agents/skills/deploy\n\
         removed deploy from .claude/skills/deploy\n"
    );
    assert_eq!(done.stderr, [format!("note: {UNREAD}")]);
    assert!(done.ok);
    // At the version it was, the one a `settings.toml` holds whatever it
    // sets: no line says a version moved.
    assert!(settings(&root).ends_with("\n[targets]\nskills = [\".skills\"]\n"));
    assert!(settings(&root).contains("\nversion = 5\n"));
    assert_eq!(
        copies(&root, "deploy"),
        [root.join(".skills").join("deploy")]
    );
    assert!(!root.join(".claude/skills/deploy").exists());
    assert!(!root.join(".agents/skills/deploy").exists());
    assert_eq!(recorded(dir.path()), [".skills/deploy"]);
    // Local, so git is offered nothing.
    assert!(untracked(&root).is_empty(), "{:?}", untracked(&root));

    let again = mind(&root, &["targets", "set", "skills", ".skills", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to change: collapse installs skills in .skills already\n"
    );
}

#[test]
fn an_install_into_a_project_installing_skills_in_one_folder_writes_only_there() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    new_project(&root);
    flayer(dir.path(), &["link", "collapse"]).unwrap();
    mind(&root, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    flayer(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    let done = mind(&root, &["install", "deploy"]).unwrap();

    assert_eq!(
        done.stdout,
        "installed deploy into collapse, from workspace\n"
    );
    assert!(root.join(".skills/deploy/SKILL.md").is_file());
    assert!(!root.join(".claude").exists());
    assert!(!root.join(".agents").exists());
    assert_eq!(recorded(dir.path()), [".skills/deploy"]);
}

#[test]
fn resetting_moves_the_copies_back_and_leaves_somebodys_where_it_is_said() {
    let (dir, root) = installed();
    mind(&root, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    let notes = root.join(".skills/notes");
    fs::create_dir_all(&notes).unwrap();
    fs::write(notes.join("SKILL.md"), skill_file("notes", "Mine")).unwrap();

    let done = mind(&root, &["targets", "reset", "skills", "--yes"]).unwrap();

    assert_eq!(
        done.stdout,
        "set the targets of skills to the folders its agents read in collapse, where they were \
         .skills\n\
         copied deploy to .agents/skills/deploy, for codex and opencode, kept out of git\n\
         copied deploy to .claude/skills/deploy, for claude, kept out of git\n\
         removed deploy from .skills/deploy\n\
         left .skills/notes where it is: Mindflayer did not install it\n"
    );
    assert!(done.stderr.is_empty(), "{:?}", done.stderr);
    assert!(done.ok);
    assert!(notes.join("SKILL.md").is_file());
    assert!(!root.join(".skills/deploy").exists());
    assert_eq!(
        recorded(dir.path()),
        [".agents/skills/deploy", ".claude/skills/deploy"]
    );
    assert!(!settings(&root).contains("\n[targets]\n"));

    let again = mind(&root, &["targets", "reset", "skills"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to change: collapse installs skills where its agents read already\n"
    );
}

#[test]
fn somebodys_skill_at_a_new_target_is_left_and_warned_about() {
    let (dir, root) = installed();
    let theirs = root.join(".skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill_file("deploy", "Theirs")).unwrap();

    let dry = mind(&root, &["targets", "set", "skills", ".skills"]).unwrap();
    assert!(
        dry.stdout.contains(
            "would leave .skills/deploy as it is: Mindflayer did not put it there, so deploy \
             would not be copied there\n"
        ),
        "{}",
        dry.stdout
    );

    let done = mind(&root, &["targets", "set", "skills", ".skills", "--yes"]).unwrap();

    // Said by the code an install refused over somebody's copy is said by.
    assert!(done.stderr.iter().any(|line| {
        *line
            == "warning: collapse already has a deploy at .skills/deploy that Mindflayer did not \
                put there, so it was left alone, and deploy was not copied there"
            && line.code == Some("foreign")
    }));
    assert!(!done.ok);
    assert_eq!(
        fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        skill_file("deploy", "Theirs")
    );
    assert!(recorded(dir.path()).is_empty());
}

#[test]
fn a_target_outside_the_project_inside_mind_or_absolute_is_refused() {
    let (_dir, root) = installed();
    let before = settings(&root);
    let elsewhere = TempDir::new().unwrap();
    let absolute = elsewhere.path().to_str().unwrap().to_owned();

    for (folder, because) in [
        ("../shared", "it is not inside the project"),
        (".mind/skills", "it is inside .mind"),
        (".git", "it is inside .git"),
        (absolute.as_str(), "it is not inside the project"),
    ] {
        for yes in [false, true] {
            let mut args = vec!["targets", "set", "skills", ".skills", folder];
            if yes {
                args.push("--yes");
            }
            let failure = mind(&root, &args).unwrap_err();
            let said = failure.error.to_string();
            assert!(
                said.starts_with(&format!(
                    "{folder}: cannot be where a project's skills are installed, because {because}"
                )),
                "{said}"
            );
        }
    }
    let rules = mind(&root, &["targets", "set", "rules", ".rules", "-y"]).unwrap_err();
    assert!(
        rules
            .error
            .to_string()
            .ends_with("because rules are not installed into a project as folders yet"),
        "{}",
        rules.error
    );
    assert_eq!(settings(&root), before);
    assert!(!root.join(".skills").exists());
}

#[test]
fn status_warns_that_no_agent_reads_the_targets_once_they_are_set() {
    let (dir, root) = installed();
    mind(&root, &["targets", "set", "skills", ".skills", "-y"]).unwrap();

    let here = mind(&root, &["status"]).unwrap();
    let across = flayer(dir.path(), &["status"]).unwrap();

    assert_eq!(here.stdout, "deploy  current  -  workspace\n");
    for outcome in [&here, &across] {
        assert_eq!(outcome.stderr, [format!("warning: {UNREAD}")]);
        assert!(!outcome.ok);
    }
}

/// Point every agent of the project at `root` at its `.skills`, as the
/// reference does: each folder they read made a link there.
#[cfg(unix)]
fn link_the_agents_to_skills(root: &Path) {
    for folder in [".claude", ".agents"] {
        let skills = root.join(folder).join("skills");
        if skills.exists() {
            fs::remove_dir_all(&skills).unwrap();
        }
        fs::create_dir_all(root.join(folder)).unwrap();
        std::os::unix::fs::symlink(root.join(".skills"), &skills).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn an_agent_pointed_at_the_target_by_a_link_reads_it() {
    let (_dir, root) = installed();
    mind(&root, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    link_the_agents_to_skills(&root);

    let status = mind(&root, &["status"]).unwrap();

    assert_eq!(
        status.stdout,
        "deploy  current  claude,codex,opencode  workspace\n"
    );
    assert!(status.stderr.is_empty(), "{:?}", status.stderr);
    assert!(status.ok);
}

#[cfg(unix)]
#[test]
fn resetting_with_the_links_still_there_keeps_the_copy_mindflayers() {
    let (dir, root) = installed();
    mind(&root, &["targets", "set", "skills", ".skills", "-y"]).unwrap();
    link_the_agents_to_skills(&root);

    let done = mind(&root, &["targets", "reset", "skills", "--yes"]).unwrap();

    assert!(done.ok, "{:?}", done.stderr);
    // The copy stays where the agents read it, recorded by the spelling
    // they are sent copies to now, which is the one discovery reads.
    assert!(root.join(".skills/deploy/SKILL.md").is_file());
    assert_eq!(recorded(dir.path()), [".agents/skills/deploy"]);
    let status = mind(&root, &["status"]).unwrap();
    assert_eq!(
        status.stdout,
        "deploy  current  claude,codex,opencode  workspace\n"
    );
    assert!(status.stderr.is_empty(), "{:?}", status.stderr);

    let source = own_directory_for(dir.path(), Kind::Skill).join("deploy/SKILL.md");
    let edited = fs::read_to_string(&source).unwrap() + "\nOne more step.\n";
    fs::write(&source, edited).unwrap();
    let again = flayer(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    assert_eq!(again.stdout, "updated deploy in collapse, from workspace\n");
    assert!(again.ok, "{:?}", again.stderr);
}

#[test]
fn flayer_targets_changes_a_project_the_workspace_manages_and_needs_one_named() {
    let (dir, root) = installed();

    let done = flayer(
        dir.path(),
        &[
            "targets", "-p", "collapse", "set", "skills", ".skills", "-y",
        ],
    )
    .unwrap();
    assert!(done
        .stdout
        .starts_with("set the targets of skills to .skills in collapse"));
    assert!(root.join(".skills/deploy").is_dir());

    // `-p` on the other side of `reset` names it all the same.
    let reset = flayer(
        dir.path(),
        &["targets", "reset", "skills", "--project", "collapse", "-y"],
    )
    .unwrap();
    assert!(reset.stdout.contains("removed deploy from .skills/deploy"));

    let failure = flayer(dir.path(), &["targets", "reset", "skills"]).unwrap_err();
    assert!(
        matches!(*failure.error, CliError::TargetsWhere),
        "{}",
        failure.error
    );
}

#[test]
fn a_project_no_workspace_links_changes_its_targets_and_is_told_what_was_not_carried() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());

    let done = mind(
        dir.path(),
        &["targets", "set", "skills", ".skills", "--yes"],
    )
    .unwrap();

    assert!(done
        .stdout
        .starts_with("set the targets of skills to .skills"));
    assert!(
        done.stderr.iter().any(|line| *line
            == "hint: no workspace links this project, so nothing installed in it was carried \
                along"),
        "{:?}",
        done.stderr
    );
}
