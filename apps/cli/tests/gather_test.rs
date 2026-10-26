//! `flayer gather`: taking skills from a git repository onto the shelf.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// gather
//
// The repository is built here rather than fetched, for the reason the core
// suite builds one: a test that needs the network fails for reasons that have
// nothing to do with the code.
// ---------------------------------------------------------------------------

#[test]
fn gather_reports_the_source_and_what_it_took() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it")),
        (
            "skills/commit-style/SKILL.md",
            &skill_file("commit-style", "How we commit"),
        ),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stdout.contains("added"), "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains("commit-style  How we commit"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome
            .stdout
            .contains("2 skills: 2 added, 0 updated, 0 unchanged"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn gathering_again_says_nothing_moved() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let again = flayer(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        again
            .stdout
            .contains("1 skill: 0 added, 0 updated, 1 unchanged"),
        "{}",
        again.stdout
    );
}

#[test]
fn a_skill_that_cannot_be_read_is_a_warning_and_a_non_zero_exit() {
    let source = repository(&[
        ("skills/good/SKILL.md", &skill_file("good", "Fine")),
        ("skills/broken/SKILL.md", "no front matter\n"),
    ]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn a_skill_with_nested_metadata_is_still_listed_with_a_warning() {
    let source = repository(&[(
        "skills/nested/SKILL.md",
        "---\nname: nested\ndescription: Nested metadata\nmetadata:\n  owner:\n    team: ops\n---\n",
    )]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let outcome = flayer(dir.path(), &["gather", "git", &url]).unwrap();

    assert!(
        outcome.stdout.contains("added  nested  Nested metadata"),
        "{}",
        outcome.stdout
    );
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    assert!(outcome.stderr[0].starts_with("warning: "));
    assert!(
        outcome.stderr[0].ends_with(
            "SKILL.md: `metadata` entry `owner` is not a string, so it is ignored, and left in the file as written"
        ),
        "{}",
        outcome.stderr[0]
    );
    assert!(!outcome.ok);
    let listed = flayer(dir.path(), &["gather", "list"]).unwrap();
    assert!(listed.stdout.contains("nested"), "{}", listed.stdout);
}

#[test]
fn validate_reports_a_gathered_skill_without_hiding_it() {
    let source = repository(&[
        ("skills/a--b/SKILL.md", &skill_file("a--b", "Doubled")),
        ("skills/fine/SKILL.md", &skill_file("fine", "Nothing wrong")),
    ]);
    let dir = workspace_with_two();
    let url = source.path().to_string_lossy().into_owned();
    let gathered = flayer(dir.path(), &["gather", "git", &url]).unwrap();
    assert!(
        gathered.stdout.contains("added  a--b"),
        "{}",
        gathered.stdout
    );

    let outcome = flayer(dir.path(), &["validate"]).unwrap();

    let shelf = fs::read_dir(dir.path().join(FLAYER_DIR).join("skills"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .into_string()
        .unwrap();
    assert_eq!(
        outcome.stdout,
        format!(
            "a--b (shelf/{shelf}): 1 problem\n  \
             - `a--b` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens\n\
             alpha (alpha): ok\n\
             beta (beta): ok\n\
             fine (shelf/{shelf}): ok\n\
             \n\
             4 skills checked, 1 invalid\n"
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(!outcome.ok);
    // One project's report is that project's, and the shelf is nobody's.
    let one = flayer(dir.path(), &["validate", "-p", "alpha"]).unwrap();
    assert_eq!(one.stdout, "alpha: ok\n\n1 skill checked, 0 invalid\n");
}

#[test]
fn a_gathered_name_cannot_put_a_carriage_return_on_the_terminal() {
    // A source writes the name and the metadata keys, and a carriage return
    // in either would let a warning or a report line overwrite itself with a
    // line that looks like a verdict.
    let source = repository(&[(
        "skills/evil/SKILL.md",
        "---\nname: \"evil\\rfine (shelf/x): ok\"\ndescription: Evil\nmetadata:\n  \"k\\rok\": [1]\n---\n",
    )]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let gathered = flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let validated = flayer(dir.path(), &["validate"]).unwrap();

    assert!(!gathered.stderr.is_empty());
    for line in gathered.stderr.iter().chain([&validated.stdout]) {
        assert!(!line.contains('\r'), "{line:?}");
    }
    assert!(
        validated
            .stdout
            .starts_with("evil fine (shelf/x): ok: 3 problems"),
        "{}",
        validated.stdout
    );
}

#[test]
fn gather_list_names_where_each_skill_came_from() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("deploy"));
    assert!(outcome.stdout.contains(&url), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("Ship it"));
}

#[test]
fn an_empty_shelf_says_how_to_fill_it() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.starts_with("nothing gathered yet"));
    assert!(outcome.stdout.contains("flayer gather git"));
}

#[test]
fn gathering_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = flayer(dir.path(), &["gather", "list"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("flayer init"));
}

#[test]
fn gather_is_reachable_the_long_way_round_too() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    let short = flayer(dir.path(), &["gather", "git", &url]).unwrap();
    let long = mind(dir.path(), &["flayer", "gather", "list"]).unwrap();

    assert!(short.ok);
    assert!(long.stdout.contains("deploy"), "{}", long.stdout);
}

#[test]
fn gather_list_tells_two_branches_of_one_repository_apart() {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();

    // The same URL at its default branch and at a named one: two sources, and
    // a listing that printed the URL alone would show one origin twice.
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    flayer(dir.path(), &["gather", "git", &url]).unwrap();
    flayer(dir.path(), &["gather", "git", &url, "--ref", &branch]).unwrap();

    let outcome = flayer(dir.path(), &["gather", "list"]).unwrap();

    assert_eq!(outcome.stdout.lines().count(), 2, "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains(&format!("{url}#{branch}")),
        "{}",
        outcome.stdout
    );
}

// ---------------------------------------------------------------------------
// credentials an older build stored
// ---------------------------------------------------------------------------

/// Leave in the workspace at `dir` what a Mindflayer from before the refusal
/// left after gathering its one source from `url`: the URL in the source, in
/// the log, and in the clone's config and reflog.
fn left_by_an_older_build(dir: &Path, url: &str) {
    let workspace = FlayerWorkspace::open(dir).unwrap();
    let database = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    database
        .execute("UPDATE sources SET url = ?1", [url])
        .unwrap();
    database
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (0, 'gather', ?1, 'ok', '1 added, 0 updated, 0 unchanged, 0 failed')",
            [url],
        )
        .unwrap();

    for clone in fs::read_dir(workspace.cache_dir()).unwrap() {
        let git = clone.unwrap().path().join(".git");
        let config: String = fs::read_to_string(git.join("config"))
            .unwrap()
            .lines()
            .map(|line| match line.trim_start().strip_prefix("url = ") {
                Some(_) => format!("\turl = {url}\n"),
                None => format!("{line}\n"),
            })
            .collect();
        fs::write(git.join("config"), config).unwrap();
        let mut reflog = fs::read_to_string(git.join("logs").join("HEAD")).unwrap();
        reflog.push_str(&format!(
            "0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n"
        ));
        fs::write(git.join("logs").join("HEAD"), reflog).unwrap();
    }
}

/// Everything the workspace at `dir` keeps that could hold a URL: its
/// database, byte for byte, and every text file in its cache.
fn kept_by(dir: &Path) -> String {
    fn text_under(root: &Path, into: &mut String) {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                text_under(&path, into);
            } else if let Ok(text) = fs::read_to_string(&path) {
                into.push_str(&text);
            }
        }
    }
    let workspace = FlayerWorkspace::open(dir).unwrap();
    let mut kept =
        String::from_utf8_lossy(&fs::read(workspace.ledger_path()).unwrap()).into_owned();
    text_under(&workspace.cache_dir(), &mut kept);
    kept
}

#[test]
fn a_token_an_older_build_stored_is_never_listed_and_a_command_that_writes_takes_it_out_once() {
    let first = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let second = repository(&[(
        "skills/review/SKILL.md",
        &skill_file("review", "Look again"),
    )]);
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();
    flayer(
        dir.path(),
        &["gather", "git", &first.path().to_string_lossy()],
    )
    .unwrap();
    left_by_an_older_build(dir.path(), "https://me:ghp_t0ken@example.com/acme/skills");

    // Listing reads, so it rewrites nothing, and it shows nothing it should
    // not.
    let listed = flayer(dir.path(), &["gather", "list"]).unwrap();
    assert_eq!(
        listed.stdout,
        "deploy  https://example.com/acme/skills  Ship it\n"
    );
    assert!(listed.stderr.is_empty(), "{:?}", listed.stderr);
    assert!(kept_by(dir.path()).contains("ghp_t0ken"));

    // The next command that writes takes it out of the ledger and the clone,
    // and says so.
    let second = second.path().to_string_lossy().into_owned();
    let gathered = flayer(dir.path(), &["gather", "git", &second]).unwrap();
    assert!(gathered.ok, "{:?}", gathered.stderr);
    assert_eq!(
        gathered.stderr,
        [
            "note: https://example.com/acme/skills: the credential an older Mindflayer stored in \
             this URL is taken out; gathering it again needs a git credential helper that has it",
            "note: 1 action log entry: the credential an older Mindflayer stored in it is taken out",
        ]
    );
    assert!(!kept_by(dir.path()).contains("ghp_t0ken"));
    let listed = flayer(dir.path(), &["gather", "list"]).unwrap();
    assert!(
        listed
            .stdout
            .contains("deploy  https://example.com/acme/skills  Ship it\n"),
        "{}",
        listed.stdout
    );

    // Said once: there is nothing left to say the second time.
    let again = flayer(dir.path(), &["gather", "git", &second]).unwrap();
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
}
