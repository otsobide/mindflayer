//! The examples in `examples/`: a workspace of their own, holding two skills
//! and two rules, that any workspace can load and install from without
//! gathering anything first, and the README's walk-through of them, held to
//! what the commands print.

mod common;

use common::*;

/// The README, read when this suite compiles, so that changing it rebuilds
/// the suite holding its walk-through to the commands.
const README: &str = include_str!("../../../README.md");

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

/// `Projects`, a workspace linking its project `collapse`, a repository as
/// the README's is.
fn projects() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    fs::create_dir_all(projects.join("collapse")).unwrap();
    flayer(&projects, &["init"]).unwrap();
    new_project(&projects.join("collapse"));
    flayer(&projects, &["link", "collapse"]).unwrap();
    (dir, projects)
}

/// Runs `act` on `examples/` and fails if it wrote anything there. Every
/// command the suite runs on the folder goes through here, a read through
/// `-W` as much as a load or an install: `-W` opens the examples as the
/// workspace, which is where a ledger or a scratch folder would be made.
fn leaving_examples_alone<T>(act: impl FnOnce(&Path) -> T) -> T {
    let examples = examples();
    let before = snapshot(&examples);
    let result = act(&examples);
    assert!(
        snapshot(&examples) == before,
        "a command run on the examples wrote into examples/"
    );
    result
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

    let listed = leaving_examples_alone(|examples| {
        flayer(dir.path(), &["-W", &text(examples), "list"]).unwrap()
    });

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

    let validated = leaving_examples_alone(|examples| {
        flayer(dir.path(), &["-W", &text(examples), "validate"]).unwrap()
    });

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

    let shown = leaving_examples_alone(|examples| {
        flayer(
            dir.path(),
            &["-W", &text(examples), "show", "style/comments"],
        )
        .unwrap()
    });

    assert!(
        shown.stdout.contains("\npaths:\n  - **/*.rs\n"),
        "{}",
        shown.stdout
    );
}

#[test]
fn a_workspace_loads_the_examples_and_installs_commit_style_with_its_reference() {
    let (_dir, projects) = projects();

    let (loaded, offered, installed) = leaving_examples_alone(|examples| {
        (
            flayer(&projects, &["load", &text(examples)]).unwrap(),
            flayer(&projects, &["gather", "list"]).unwrap(),
            flayer(&projects, &["install", "-p", "collapse", "commit-style"]).unwrap(),
        )
    });

    assert!(loaded.ok, "{:?}", loaded.stderr);
    assert!(
        loaded.stdout.starts_with("loaded examples as ")
            && loaded.stdout.contains(": 2 skills to install\n"),
        "{}",
        loaded.stdout
    );
    // Only the skills: a rule cannot be installed yet, so none is offered.
    assert_eq!(
        first_words(&offered.stdout),
        ["code-review", "commit-style"],
        "{}",
        offered.stdout
    );
    assert!(installed.ok, "{:?}", installed.stderr);
    let copy = projects.join("collapse/skills/commit-style");
    let source = examples().join("skills/commit-style");
    for file in ["SKILL.md", "references/examples.md"] {
        assert_eq!(
            fs::read(copy.join(file)).unwrap(),
            fs::read(source.join(file)).unwrap(),
            "{file}"
        );
    }
}

#[test]
fn the_readme_walks_through_the_examples_as_the_commands_print_them() {
    let (_dir, projects) = projects();

    let (listed, loaded, installed) = leaving_examples_alone(|examples| {
        let examples = text(examples);
        (
            flayer(&projects, &["-W", &examples, "list"]).unwrap(),
            flayer(&projects, &["load", &examples]).unwrap(),
            flayer(&projects, &["install", "-p", "collapse", "commit-style"]).unwrap(),
        )
    });
    // The README shows stdout alone, which is all there is to show only when
    // nothing was said on stderr.
    for outcome in [&listed, &loaded, &installed] {
        assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    }

    // This machine's paths, written the README's way: the clone in
    // ~/src/mindflayer and the workspace in ~/Projects, in the home of `you`.
    // The entry is read back rather than worked out, since where no route
    // from the workspace reaches the clone, another drive say, it is absolute.
    let workspace = FlayerWorkspace::open(&projects).unwrap();
    let entry = mindflayer_core::paths::to_config_string(&workspace.config().loaded[0]).unwrap();
    let root = projects.display().to_string();
    let as_written = |stdout: &str| {
        stdout
            .replace(&entry, "../src/mindflayer/examples")
            .replace(&root, "/home/you/Projects")
    };
    let blocks = [
        format!(
            "$ flayer -W ~/src/mindflayer/examples list\n{}",
            as_written(&listed.stdout)
        ),
        format!(
            "$ cd ~/Projects\n$ flayer load ~/src/mindflayer/examples\n{}\n\
             $ flayer install -p collapse commit-style\n{}",
            as_written(&loaded.stdout),
            as_written(&installed.stdout)
        ),
    ];

    // A checkout on Windows may have given the README CRLF line endings.
    let readme = README.replace("\r\n", "\n");
    for block in blocks {
        assert!(
            readme.contains(&format!("\n```\n{block}```\n")),
            "the README should show, in a block of its own:\n{block}"
        );
    }
}
