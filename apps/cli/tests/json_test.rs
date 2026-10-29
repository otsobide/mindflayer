//! The reading commands' `--json`: `list`, at either level, read back into
//! structs of this suite's own, each held to the entries the text lists, the
//! warnings it says and the exit status it has.

// Every field of the structs below is named so that one renamed or added is
// refused, which is what they are for: not every test reads every one.
#![allow(dead_code)]

mod common;

use serde::Deserialize;
use serde_json::Value;

use common::*;

// ---------------------------------------------------------------------------
// What `list --json` prints, field by field: the snapshot's holders. Nothing
// else is allowed, so a field renamed or added fails here first.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    library: Option<Library>,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    directories: Vec<Directory>,
    artifacts: Vec<Held>,
    templates: Vec<Template>,
    failures: Vec<Said>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    entry: Option<String>,
    path: String,
    name: Option<String>,
    marker: Option<String>,
    id: Option<String>,
    agents: Vec<String>,
    settings: Option<Settings>,
    stale: Option<Said>,
    directories: Vec<Directory>,
    artifacts: Vec<Held>,
    templates: Vec<Template>,
    unreached: Vec<Value>,
    missed: Vec<Value>,
    misplaced: Vec<Said>,
    failures: Vec<Said>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    agents: Vec<String>,
    share: String,
    edit_tracked: bool,
    targets: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory {
    kind: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    kind: String,
    name: String,
    summary: Option<String>,
    state: Option<String>,
    agents: Vec<String>,
    origin: Option<String>,
    targets: Vec<String>,
    issues: Vec<Value>,
    unread: Option<Unread>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unread {
    path: String,
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Template {
    name: String,
    kind: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    code: String,
    message: String,
}

/// Every row a listing names, as `holder kind name`, `workspace` the holder
/// of the library's own, in the order it lists them: holder by holder.
fn entries(listing: &Listing) -> Vec<String> {
    let own = listing
        .library
        .iter()
        .flat_map(|library| library.artifacts.iter().map(|held| ("workspace", held)));
    let projects = listing.projects.iter().flat_map(|project| {
        let name = project.name.as_deref().unwrap_or("?");
        project.artifacts.iter().map(move |held| (name, held))
    });
    own.chain(projects)
        .map(|(holder, held)| format!("{holder} {} {}", held.kind, held.name))
        .collect()
}

/// The first `columns` words of every row a table prints, sorted.
fn rows(stdout: &str, columns: usize) -> Vec<String> {
    let mut rows: Vec<String> = stdout
        .lines()
        .map(|line| {
            line.split_whitespace()
                .take(columns)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    rows.sort();
    rows
}

/// [`entries`], sorted as [`rows`] is, to hold one against the other.
fn sorted(mut entries: Vec<String>) -> Vec<String> {
    entries.sort();
    entries
}

/// Each warning line an outcome said on stderr, without the `warning: `
/// the envelope leaves out.
fn warned(outcome: &Outcome) -> Vec<&str> {
    outcome
        .stderr
        .iter()
        .filter_map(|line| line.text.strip_prefix("warning: "))
        .collect()
}

/// A workspace whose library holds the skill `tidy`, linking `alpha`, which
/// authors the skill `alpha` and the rule `git/commits` and received `tidy`
/// from the library, and `beta`, which authors the skill `beta`.
fn workspace_with_rows() -> TempDir {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_rule(&alpha, "git/commits.md", "# Commits\n\nSay why.\n");
    flayer(dir.path(), &["add", "skill", "tidy", "Tidy it"]).unwrap();
    flayer(dir.path(), &["install", "-p", "alpha", "tidy"]).unwrap();
    dir
}

#[test]
fn flayer_list_as_json_holds_the_entries_the_text_lists() {
    let dir = workspace_with_rows();

    let text = flayer(dir.path(), &["list"]).unwrap();
    let json = flayer(dir.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(
        entries(&listing),
        [
            "workspace skill tidy",
            "alpha skill alpha",
            "alpha skill tidy",
            "alpha rule git/commits",
            "beta skill beta",
        ]
    );
    assert_eq!(sorted(entries(&listing)), rows(&text.stdout, 3));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(json.stderr.is_empty(), "{:?}", json.stderr);
    assert!(json.ok && text.ok);
    // As the snapshot holds them: how each stands, from where, read by whom.
    let alpha = &listing.projects[0];
    assert_eq!(alpha.entry.as_deref(), Some("alpha"));
    let tidy = alpha
        .artifacts
        .iter()
        .find(|held| held.name == "tidy")
        .unwrap();
    assert_eq!(tidy.state.as_deref(), Some("current"));
    assert_eq!(tidy.origin.as_deref(), Some("workspace"));
    assert_eq!(tidy.summary.as_deref(), Some("Tidy it"));
    let library = listing.library.as_ref().unwrap();
    assert_eq!(library.artifacts[0].state, None);
}

#[test]
fn a_kind_narrows_what_list_as_json_holds_as_it_narrows_the_text() {
    let dir = workspace_with_rows();

    let text = flayer(dir.path(), &["list", "rules"]).unwrap();
    let json = flayer(dir.path(), &["list", "rules", "--json"]).unwrap();

    let (listing, _): (Listing, _) = enveloped(&json);
    assert_eq!(entries(&listing), ["alpha rule git/commits"]);
    assert_eq!(text.stdout, "git/commits  Commits\n");
    for project in &listing.projects {
        assert!(project
            .templates
            .iter()
            .all(|template| template.kind == "rule"));
        assert!(project
            .directories
            .iter()
            .all(|folder| folder.kind == "rule"));
    }
}

#[test]
fn mind_list_as_json_is_flayer_list_of_its_project() {
    let dir = workspace_with_rows();
    let alpha = dir.path().join("alpha");

    let here = mind(&alpha, &["list", "--json"]).unwrap();
    let named = flayer(dir.path(), &["list", "-p", "alpha", "--json"]).unwrap();
    let text = mind(&alpha, &["list"]).unwrap();

    assert_eq!(here, named);
    let (listing, _): (Listing, _) = enveloped(&here);
    assert!(listing.library.is_none());
    assert_eq!(listing.projects.len(), 1);
    assert_eq!(sorted(entries(&listing)), {
        let mut rows: Vec<String> = rows(&text.stdout, 2)
            .into_iter()
            .map(|row| format!("alpha {row}"))
            .collect();
        rows.sort();
        rows
    });
}

/// A file that does not read is a warning in the envelope, with the code
/// stderr gives it, and fails the JSON as it fails the text; the snapshot
/// lists it all the same, by its place, saying why.
#[test]
fn an_artifact_that_does_not_read_is_a_warning_and_fails_list_as_json_too() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_skill(&alpha, "broken", "no front matter\n");

    for (text, json) in [
        (
            flayer(dir.path(), &["list"]).unwrap(),
            flayer(dir.path(), &["list", "--json"]).unwrap(),
        ),
        (
            mind(&alpha, &["list"]).unwrap(),
            mind(&alpha, &["list", "--json"]).unwrap(),
        ),
    ] {
        let (listing, warnings): (Listing, _) = enveloped(&json);
        assert_eq!(warnings, warned(&text));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("broken"), "{warnings:?}");
        assert_eq!(warning_codes(&json), stderr_codes(&text));
        assert_eq!(
            warning_codes(&json),
            [Some(String::from("front_matter_missing"))]
        );
        assert!(json.stderr.is_empty(), "{:?}", json.stderr);
        assert!(!text.ok);
        assert_eq!(json.ok, text.ok);
        let broken = listing
            .projects
            .iter()
            .flat_map(|project| &project.artifacts)
            .find(|held| held.name == "broken")
            .expect("listed by its place");
        let unread = broken.unread.as_ref().unwrap();
        assert_eq!(unread.code, "front_matter_missing");
        assert_eq!(unread.path, "skills/broken");
    }
}

/// An entry that opens no project is a warning, as the text says it, and a
/// project of its own in the data, saying why.
#[test]
fn a_stale_entry_is_a_warning_and_a_project_saying_why_in_list_as_json() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("beta")).unwrap();

    let text = flayer(dir.path(), &["list"]).unwrap();
    let json = flayer(dir.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(warnings, warned(&text));
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert_eq!(listing.projects.len(), 2);
    let stale = listing.projects[1].stale.as_ref().expect("stale");
    assert_eq!(listing.projects[1].entry.as_deref(), Some("beta"));
    assert_eq!(stale.code, "not_a_project");
    assert!(!json.ok && !text.ok);
}

/// A hint is advice for whoever reads the terminal, so it stays on stderr
/// under `--json`, outside the envelope, and fails nothing.
#[test]
fn the_hint_about_an_unmade_default_workspace_stays_on_stderr() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let json = flayer_at_home(elsewhere.path(), home.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert!(listing.projects.is_empty());
    assert!(warnings.is_empty());
    assert_eq!(json.stderr.len(), 1, "{:?}", json.stderr);
    assert!(json.stderr[0].text.starts_with("hint: "));
    assert!(json.ok);
    assert!(!home.path().join(".mindflayer").exists());
}

/// `mind list --json` reads how each stands against the workspace `mind`
/// consults, which the text never reads: one that cannot be read is a
/// warning beside the listing, with the project read alone, as `mind
/// status` says it.
#[test]
fn a_workspace_above_that_cannot_be_read_is_a_warning_beside_mind_list_as_json() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    let marker = dir.path().join(FLAYER_DIR).join(FLAYER_CONFIG);
    let mut written = fs::read_to_string(&marker).unwrap();
    written.push_str("garbage = [\n");
    fs::write(&marker, written).unwrap();

    let text = mind(&alpha, &["list"]).unwrap();
    let json = mind(&alpha, &["list", "--json"]).unwrap();

    assert!(text.ok);
    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(entries(&listing), ["alpha skill alpha"]);
    assert_eq!(listing.projects[0].entry, None);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(&marker.display().to_string()));
    assert!(!json.ok);
}

#[test]
fn a_workspace_named_that_is_none_refuses_mind_list_as_json() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    let nowhere = TempDir::new().unwrap();

    let named = nowhere.path().to_string_lossy();
    let failure = mind(dir.path(), &["list", "--json", "-W", &named]).unwrap_err();

    assert!(
        matches!(*failure.error, CliError::Chosen { .. }),
        "{}",
        failure.error
    );
}
