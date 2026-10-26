//! The examples in `examples/`: a workspace of their own, holding two skills
//! and two rules, that any workspace can load and install from without
//! gathering anything first.

mod common;

use common::*;

/// `examples/` in this repository, used where it is rather than copied: what a
/// person loads is the folder in their clone, so that is the folder held to
/// the rules, and nothing done here may write into it.
fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/cli sits two folders below the repository")
        .join("examples")
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `Projects`, a workspace linking its project `collapse`.
fn projects() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    fs::create_dir_all(projects.join("collapse")).unwrap();
    flayer(&projects, &["init"]).unwrap();
    mind(&projects.join("collapse"), &["init"]).unwrap();
    flayer(&projects, &["link", "collapse"]).unwrap();
    (dir, projects)
}

/// Every file under `root` with what it holds, to tell whether anything was
/// written there.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let contents = fs::read(&path).unwrap();
                files.push((path, contents));
            }
        }
    }
    files.sort();
    files
}

/// The first word of each line: the kind, in a listing that has a kind
/// column, or the name in one that does not.
fn first_words(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

#[test]
fn the_examples_are_two_skills_and_two_rules() {
    let dir = TempDir::new().unwrap();

    let listed = flayer(dir.path(), &["-W", &text(&examples()), "list"]).unwrap();

    assert!(listed.ok, "{:?}", listed.stderr);
    let rows: Vec<Vec<&str>> = listed
        .stdout
        .lines()
        .map(|line| line.split_whitespace().take(2).collect())
        .collect();
    assert_eq!(
        rows,
        [
            ["skill", "code-review"],
            ["skill", "commit-style"],
            ["rule", "git/commits"],
            ["rule", "style/comments"],
        ],
        "{}",
        listed.stdout
    );
}

#[test]
fn every_example_validates_without_a_problem_or_a_warning() {
    // `validate` holds a skill to every rule `install` is strict about, its
    // name and its description among them, and says what a rule's front
    // matter leaves unread as a warning: so all of them, or none, pass here.
    let dir = TempDir::new().unwrap();

    let validated = flayer(dir.path(), &["-W", &text(&examples()), "validate"]).unwrap();

    assert!(validated.ok, "{}{:?}", validated.stdout, validated.stderr);
    assert_eq!(
        validated.stdout,
        "skill:code-review: ok\n\
         skill:commit-style: ok\n\
         rule:git/commits: ok\n\
         rule:style/comments: ok\n\
         \n\
         2 skills and 2 rules checked, 0 invalid\n"
    );
}

#[test]
fn the_rule_with_front_matter_says_which_files_it_is_about() {
    // Front matter that parsed as something other than a mapping would be
    // read as content, silently: what `show` declares is the proof it did not.
    let dir = TempDir::new().unwrap();

    let shown = flayer(
        dir.path(),
        &["-W", &text(&examples()), "show", "style/comments"],
    )
    .unwrap();

    assert!(
        shown.stdout.contains("\npaths:\n  - **/*.rs\n"),
        "{}",
        shown.stdout
    );
}

#[test]
fn a_workspace_loads_the_examples_and_installs_commit_style_with_its_reference() {
    let (_dir, projects) = projects();
    let examples = examples();
    let before = snapshot(&examples);

    let loaded = flayer(&projects, &["load", &text(&examples)]).unwrap();
    assert!(loaded.ok, "{:?}", loaded.stderr);
    assert!(
        loaded.stdout.starts_with("loaded examples as ")
            && loaded.stdout.contains(": 2 skills to install\n"),
        "{}",
        loaded.stdout
    );
    // Only the skills: a rule cannot be installed yet, so none is offered.
    let offered = flayer(&projects, &["gather", "list"]).unwrap();
    assert_eq!(
        first_words(&offered.stdout),
        ["code-review", "commit-style"],
        "{}",
        offered.stdout
    );

    let installed = flayer(&projects, &["install", "-p", "collapse", "commit-style"]).unwrap();

    assert!(installed.ok, "{:?}", installed.stderr);
    let copy = projects.join("collapse/skills/commit-style");
    let source = examples.join("skills/commit-style");
    for file in ["SKILL.md", "references/examples.md"] {
        assert_eq!(
            fs::read(copy.join(file)).unwrap(),
            fs::read(source.join(file)).unwrap(),
            "{file}"
        );
    }
    assert!(
        snapshot(&examples) == before,
        "loading the examples and installing from them wrote into examples/"
    );
}
