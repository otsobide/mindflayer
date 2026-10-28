//! `flayer adopt`: a skill somebody keeps in a project's agents' folders
//! brought into the workspace's library, its copies recorded as installed
//! from there and left byte for byte as they were, and what is refused.

mod common;

use common::*;

/// [`workspace_with_two`], with `notes` kept by hand in `alpha`'s
/// `.claude/skills`.
fn with_notes() -> TempDir {
    let dir = workspace_with_two();
    write_notes(&dir.path().join("alpha/.claude/skills/notes"), "Keep notes");
    dir
}

fn write_notes(folder: &Path, description: &str) {
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join("SKILL.md"), skill_file("notes", description)).unwrap();
}

/// Where the workspace keeps its own `notes`.
fn library(dir: &Path) -> PathBuf {
    own_directory_for(dir, Kind::Skill).join("notes")
}

/// Every file under `root` but git's own, with what it holds.
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, found: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort();
    found
}

#[test]
fn adopt_without_yes_says_what_it_would_do_and_writes_nothing() {
    let dir = with_notes();

    let outcome = flayer(dir.path(), &["adopt", "-p", "alpha", "notes"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "would copy notes from alpha into the library, at {}\n\
             would record notes in alpha as installed from workspace, at .claude/skills/notes\n\
             would record .agents/skills/notes missing, for `flayer update -p alpha` to put there\n\
             would keep .claude/skills/notes out of git\n\
             nothing adopted: add --yes to go ahead\n",
            library(dir.path()).display()
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);
    assert!(!library(dir.path()).exists());
    assert!(!dir.path().join(FLAYER_DIR).join("mindflayer.db").exists());
    let status = flayer(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status.stdout.contains("notes  foreign"),
        "{}",
        status.stdout
    );
}

#[test]
fn adopt_copies_into_the_library_records_the_copies_and_leaves_the_project_as_it_was() {
    let dir = with_notes();
    let alpha = dir.path().join("alpha");
    let before = files(&alpha);

    let outcome = flayer(dir.path(), &["adopt", "-p", "alpha", "notes", "--yes"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "copied notes from alpha into the library, at {}\n\
             recorded notes in alpha as installed from workspace, at .claude/skills/notes\n\
             recorded .agents/skills/notes missing: `flayer update -p alpha` puts it there\n\
             kept .claude/skills/notes out of git\n",
            library(dir.path()).display()
        )
    );
    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert_eq!(files(&alpha), before);
    assert_eq!(
        fs::read(library(dir.path()).join("SKILL.md")).unwrap(),
        fs::read(alpha.join(".claude/skills/notes/SKILL.md")).unwrap()
    );
    assert!(untracked(&alpha).iter().all(|path| !path.contains("notes")));
    let status = flayer(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status
            .stdout
            .contains("notes  missing   claude,opencode  workspace"),
        "{}",
        status.stdout
    );
}

#[test]
fn an_update_treats_an_adopted_copy_as_managed() {
    let dir = with_notes();
    flayer(dir.path(), &["adopt", "-p", "alpha", "notes", "--yes"]).unwrap();

    let put_back = flayer(dir.path(), &["update", "-p", "alpha", "--yes"]).unwrap();
    assert_eq!(
        put_back.stdout,
        "put back notes in alpha at .agents/skills/notes, from workspace\n"
    );
    let status = flayer(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status
            .stdout
            .contains("notes  current   claude,codex,opencode  workspace"),
        "{}",
        status.stdout
    );

    write_notes(&library(dir.path()), "Keep better notes");
    let updated = flayer(dir.path(), &["update", "-p", "alpha", "--yes"]).unwrap();
    assert_eq!(updated.stdout, "updated notes in alpha, from workspace\n");
    for copy in copies(&dir.path().join("alpha"), "notes") {
        assert_eq!(
            fs::read_to_string(copy.join("SKILL.md")).unwrap(),
            skill_file("notes", "Keep better notes")
        );
    }
}

#[test]
fn adopting_a_name_the_library_holds_is_refused_with_both_paths() {
    let dir = with_notes();
    flayer(
        dir.path(),
        &["add", "skill", "notes", "The library's notes"],
    )
    .unwrap();

    let failure = flayer(dir.path(), &["adopt", "-p", "alpha", "notes", "--yes"]).unwrap_err();

    let said = failure.error.to_string();
    assert!(
        said.contains(&library(dir.path()).display().to_string()),
        "{said}"
    );
    assert!(
        said.contains(
            &dir.path()
                .join("alpha/.claude/skills/notes")
                .display()
                .to_string()
        ),
        "{said}"
    );
    let status = flayer(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status.stdout.contains("notes  foreign"),
        "{}",
        status.stdout
    );
}

#[test]
fn a_rule_is_not_adopted() {
    let dir = with_notes();

    let failure = flayer(dir.path(), &["adopt", "-p", "alpha", "rule:git/commits"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        "rule `git/commits` is not adopted: only a skill somebody keeps in an agent's folder is"
    );
}

#[test]
fn what_gh_skill_says_of_where_it_came_from_is_shown() {
    let dir = workspace_with_two();
    let folder = dir.path().join("alpha/.claude/skills/notes");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        "---\nname: notes\ndescription: Keep notes\nmetadata:\n  github-repo: someone/skills\n  \
         github-ref: v1\n---\n\n# notes\n",
    )
    .unwrap();

    let outcome = flayer(dir.path(), &["adopt", "-p", "alpha", "notes"]).unwrap();

    assert!(
        outcome.stdout.contains(
            "\n  from gh skill, as its SKILL.md says: github-repo someone/skills, github-ref v1\n"
        ),
        "{}",
        outcome.stdout
    );
}

#[test]
fn a_copy_git_tracks_in_a_local_project_is_left_in_git_and_said() {
    let dir = with_notes();
    let alpha = dir.path().join("alpha");
    indexed(&alpha);

    let outcome = flayer(dir.path(), &["adopt", "-p", "alpha", "notes", "--yes"]).unwrap();

    assert!(!outcome.stdout.contains("out of git"), "{}", outcome.stdout);
    assert_eq!(
        outcome.stderr,
        ["note: git tracks .claude/skills/notes, so it stays in git though alpha is local: what \
          an update writes there shows in `git status`"]
    );
    assert!(outcome.ok);
    let exclude = fs::read_to_string(alpha.join(".git/info/exclude")).unwrap_or_default();
    assert!(!exclude.contains("notes"), "{exclude}");
}

#[test]
fn adopt_names_the_project_it_takes_from() {
    let dir = with_notes();

    let error = FlayerCli::try_parse_from(["flayer", "adopt", "notes"]).unwrap_err();

    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert!(flayer(dir.path(), &["adopt", "-p", "gamma", "notes"]).is_err());
}
