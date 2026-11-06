//! A gathered source's own commands: `mindweaver gather sources`, `rename`,
//! `update` and `remove`.

mod common;

use common::*;

/// A workspace in a folder of its own, and a repository offering `skills`,
/// each a `SKILL.md` saying it is about itself.
fn gathered_from(skills: &[&str]) -> (TempDir, TempDir, String) {
    let files: Vec<(String, String)> = skills
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                skill_file(name, &format!("About {name}")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, contents)| (path.as_str(), contents.as_str()))
        .collect();
    let source = repository(&borrowed);
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    (dir, source, url)
}

/// What `gather sources --json` prints of one source, every name it has.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Listed {
    kind: String,
    url: String,
    reference: Option<String>,
    subdirectory: String,
    revision: Option<String>,
    directory: String,
    name: String,
    pin: Option<String>,
    gathered_at: Option<i64>,
    artifacts: usize,
}

/// The commit `repository`'s HEAD is at, whole.
fn head(repository: &TempDir) -> String {
    gix::open(repository.path())
        .unwrap()
        .head_id()
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// gather sources
// ---------------------------------------------------------------------------

#[test]
fn gather_sources_lists_each_source_by_its_name_with_what_it_holds() {
    let (dir, source, url) = gathered_from(&["deploy", "review"]);
    let pin = head(&source);
    weaver(dir.path(), &["gather", "git", &url, "--name", "team"]).unwrap();
    let branch = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = branch.trim().rsplit('/').next().unwrap().to_owned();
    weaver(
        dir.path(),
        &[
            "gather", "git", &url, "--ref", &branch, "--pin", &pin, "--name", "pinned",
        ],
    )
    .unwrap();

    let outcome = weaver(dir.path(), &["gather", "sources"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    let lines: Vec<&str> = outcome.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", outcome.stdout);
    // In the order the ledger lists them, by URL, then branch: the one at
    // the remote's HEAD first.
    let words =
        |line: &str| -> Vec<String> { line.split_whitespace().map(str::to_owned).collect() };
    let (team, pinned) = (words(lines[0]), words(lines[1]));
    assert_eq!(
        team[..2],
        [String::from("team"), url.clone()],
        "{}",
        lines[0]
    );
    assert_eq!(team[2..4], ["2", "skills"], "{}", lines[0]);
    assert_eq!(team[4], "gathered", "{}", lines[0]);
    assert!(lines[0].ends_with(" UTC"), "{}", lines[0]);
    assert_eq!(
        pinned[..6],
        [
            String::from("pinned"),
            url.clone(),
            String::from("ref"),
            branch.clone(),
            String::from("pinned"),
            pin[..7].to_owned()
        ],
        "{}",
        lines[1]
    );
    // The columns line up: a branch and a pin only one source has leave
    // the other's cells empty.
    assert_eq!(lines[0].find("2 skills"), lines[1].find("2 skills"));
}

#[test]
fn gather_sources_json_prints_each_source_in_the_envelope() {
    let (dir, source, url) = gathered_from(&["deploy"]);
    weaver(dir.path(), &["gather", "git", &url]).unwrap();

    let outcome = weaver(dir.path(), &["gather", "sources", "--json"]).unwrap();

    let (listed, warnings): (Vec<Listed>, _) = enveloped(&outcome);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(listed.len(), 1, "{listed:?}");
    let listed = &listed[0];
    assert_eq!(listed.kind, "git");
    assert_eq!(listed.url, url);
    assert_eq!(listed.reference, None);
    assert_eq!(listed.subdirectory, "skills");
    assert_eq!(listed.revision.as_deref(), Some(head(&source).as_str()));
    assert!(!listed.directory.is_empty());
    assert!(!listed.name.is_empty());
    assert_eq!(listed.pin, None);
    assert!(listed.gathered_at.is_some_and(|at| at > 0), "{listed:?}");
    assert_eq!(listed.artifacts, 1);
}

#[test]
fn a_source_whose_gather_failed_is_listed_as_never_gathered() {
    let (dir, _source, url) = gathered_from(&["deploy"]);
    // Registered, then refused: it has no `agents` folder to gather from.
    weaver(dir.path(), &["gather", "git", &url, "--path", "agents"]).unwrap_err();

    let text = weaver(dir.path(), &["gather", "sources"]).unwrap();
    let json = weaver(dir.path(), &["gather", "sources", "--json"]).unwrap();

    assert!(
        text.stdout.trim_end().ends_with("0 skills  never gathered"),
        "{}",
        text.stdout
    );
    let (listed, _): (Vec<Listed>, _) = enveloped(&json);
    assert_eq!((listed[0].gathered_at, listed[0].artifacts), (None, 0));
}

#[test]
fn no_sources_says_how_to_gather_one_and_makes_no_ledger() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(dir.path(), &["gather", "sources"]).unwrap();
    let json = weaver(dir.path(), &["gather", "sources", "--json"]).unwrap();

    assert!(outcome.ok);
    assert_eq!(
        outcome.stdout,
        "no sources yet\n  take some with `mindweaver gather git <url>`\n"
    );
    let (listed, _): (Vec<Listed>, _) = enveloped(&json);
    assert!(listed.is_empty());
    assert!(!dir.path().join(WEAVER_DIR).join("mindweaver.db").exists());
}

// ---------------------------------------------------------------------------
// gather rename
// ---------------------------------------------------------------------------

#[test]
fn gather_rename_calls_a_source_its_new_name_which_from_then_takes() {
    let (dir, _source, url) = gathered_from(&["deploy"]);
    weaver(dir.path(), &["gather", "git", &url, "--name", "acme"]).unwrap();
    let project = dir.path().join("collapse");
    fs::create_dir(&project).unwrap();
    new_project(&project);
    weaver(dir.path(), &["link", "collapse"]).unwrap();

    let renamed = weaver(dir.path(), &["gather", "rename", "acme", "team"]).unwrap();

    assert!(renamed.ok, "{:?}", renamed.stderr);
    assert_eq!(
        renamed.stdout,
        format!("renamed source acme to team, gathered from {url}\n")
    );
    let listed = weaver(dir.path(), &["gather", "list"]).unwrap();
    assert!(listed.stdout.contains("deploy  team"), "{}", listed.stdout);
    let installed = weaver(
        dir.path(),
        &["install", "-p", "collapse", "deploy", "--from", "team"],
    )
    .unwrap();
    assert!(installed.ok, "{:?}", installed.stderr);
    // The name it had names nothing now.
    let old = weaver(
        dir.path(),
        &["install", "-p", "collapse", "deploy", "--from", "acme"],
    )
    .unwrap_err();
    assert!(old.error.to_string().contains("acme"), "{}", old.error);
}

#[test]
fn gather_rename_to_the_name_it_has_says_nothing_changed() {
    let (dir, _source, url) = gathered_from(&["deploy"]);
    weaver(dir.path(), &["gather", "git", &url, "--name", "acme"]).unwrap();

    let renamed = weaver(dir.path(), &["gather", "rename", "acme", "acme"]).unwrap();

    assert_eq!(
        renamed.stdout,
        "acme is called that already: nothing changed\n"
    );
}

#[test]
fn gather_rename_refuses_a_name_no_source_can_have_or_another_has_with_gathers_codes() {
    let (dir, _source, url) = gathered_from(&["deploy"]);
    let branch = {
        let head = fs::read_to_string(Path::new(&url).join(".git").join("HEAD")).unwrap();
        head.trim().rsplit('/').next().unwrap().to_owned()
    };
    weaver(dir.path(), &["gather", "git", &url, "--name", "acme"]).unwrap();
    weaver(
        dir.path(),
        &["gather", "git", &url, "--ref", &branch, "--name", "other"],
    )
    .unwrap();

    let invalid = weaver(dir.path(), &["gather", "rename", "acme", "team/v3"]).unwrap_err();
    let taken = weaver(dir.path(), &["gather", "rename", "acme", "other"]).unwrap_err();
    let unknown = weaver(dir.path(), &["gather", "rename", "nobody", "team"]).unwrap_err();
    // The codes a gather given the same `--name` refuses it with.
    let gathered = weaver(dir.path(), &["gather", "git", &url, "--name", "other"]).unwrap_err();

    assert_eq!(invalid.error.code(), "source_name_invalid");
    assert_eq!(taken.error.code(), "source_name_taken");
    assert_eq!(taken.error.code(), gathered.error.code());
    assert_eq!(
        taken.error.to_string(),
        format!("`other` is the name of the source at {url}#{branch} already: rename this one to another")
    );
    assert_eq!(unknown.error.code(), "source_not_found");
    let names = weaver(dir.path(), &["gather", "sources"]).unwrap();
    let called: Vec<&str> = names
        .stdout
        .lines()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(called, ["acme", "other"], "nothing renamed");
}

#[test]
fn gather_rename_in_a_workspace_that_never_gathered_makes_no_ledger() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let failure = weaver(dir.path(), &["gather", "rename", "acme", "team"]).unwrap_err();

    assert_eq!(failure.error.code(), "source_not_found");
    assert!(!dir.path().join(WEAVER_DIR).join("mindweaver.db").exists());
}
