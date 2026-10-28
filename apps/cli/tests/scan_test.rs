//! `flayer scan`.

mod common;

use common::*;
use mindflayer_core::scan::{DEFAULT_DEPTH, MAX_LISTED};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// flayer scan
// ---------------------------------------------------------------------------

#[test]
fn scan_says_what_is_under_the_workspace_and_what_is_linked() {
    let dir = workspace_with_two();
    flayer(dir.path(), &["unlink", "beta"]).unwrap();
    fs::create_dir_all(dir.path().join("gamma/.git")).unwrap();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    let lines: Vec<&str> = outcome.stdout.lines().collect();
    assert_eq!(lines[0], "alpha  linked");
    assert_eq!(lines[1], "beta   not linked  mind project");
    assert_eq!(
        lines[2],
        "gamma  not linked  git repository, not a mind project yet"
    );
    assert!(outcome.stdout.contains("flayer link <path>"));
    assert!(outcome.stdout.contains("mind -C <path> init"));
}

#[test]
fn scan_with_everything_linked_has_no_advice_to_give() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert_eq!(outcome.stdout, "alpha  linked\nbeta   linked\n");
}

#[test]
fn scan_of_an_empty_workspace_says_so() {
    let dir = TempDir::new().unwrap();
    flayer(dir.path(), &["init"]).unwrap();

    let outcome = flayer(dir.path(), &["scan"]).unwrap();

    assert!(outcome
        .stdout
        .starts_with("no mind projects or git repositories under"));
}

#[test]
fn scan_spells_each_path_as_link_takes_it_from_where_it_runs() {
    let dir = workspace_with_two();
    let sub = dir.path().join("sub");
    fs::create_dir_all(sub.join("delta")).unwrap();
    new_project(&sub.join("delta"));

    let outcome = flayer(&sub, &["scan"]).unwrap();

    // What is outside where it ran is spelled whole; what is inside, as a
    // route from there.
    let alpha = dir.path().join("alpha").display().to_string();
    let beta = dir.path().join("beta").display().to_string();
    let width = alpha.len().max("delta".len());
    assert_eq!(
        outcome.stdout.lines().take(3).collect::<Vec<_>>(),
        vec![
            format!("{alpha:width$}  linked"),
            format!("{beta:width$}  linked"),
            format!("{:width$}  not linked  mind project", "delta"),
        ]
    );
    let linked = flayer(&sub, &["link", "delta"]).unwrap();
    assert_eq!(linked.stdout, "linked delta as sub/delta\n");
}

#[test]
fn scan_looks_under_each_folder_given_taken_from_where_it_runs() {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir(&ws).unwrap();
    flayer(&ws, &["init"]).unwrap();
    fs::create_dir_all(ws.join("inside/.git")).unwrap();
    let work = dir.path().join("work");
    let alpha = work.join("acme/alpha");
    fs::create_dir_all(&alpha).unwrap();
    new_project(&alpha);
    let other = dir.path().join("other");
    fs::create_dir_all(other.join("beta/.git")).unwrap();
    let before = registry(&ws);

    // `work` from where it runs, as `link` takes a path; `other` whole.
    let outcome = flayer(&ws, &["scan", "../work", other.to_str().unwrap()]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    let alpha = alpha.display().to_string();
    let beta = other.join("beta").display().to_string();
    let width = alpha.len().max(beta.len());
    let lines: Vec<&str> = outcome.stdout.lines().take(2).collect();
    let mut expected = vec![
        format!("{alpha:width$}  not linked  mind project"),
        format!("{beta:width$}  not linked  git repository, not a mind project yet"),
    ];
    expected.sort();
    assert_eq!(lines, expected);
    // Not under the workspace, which it was not told to look under.
    assert!(!outcome.stdout.contains("inside"), "{}", outcome.stdout);
    // It links nothing.
    assert_eq!(registry(&ws), before);
}

#[test]
fn scan_goes_as_deep_as_it_is_told_and_three_levels_unless_told() {
    let dir = workspace_with_two();
    fs::create_dir_all(dir.path().join("org/team/deep/.git")).unwrap();
    fs::create_dir_all(dir.path().join("a/b/c/d/deeper/.git")).unwrap();

    let routes = |args: &[&str]| -> Vec<String> {
        let outcome = flayer(dir.path(), args).unwrap();
        outcome
            .stdout
            .lines()
            .take_while(|line| !line.is_empty())
            .map(|line| line.split_whitespace().next().unwrap().to_owned())
            .collect()
    };

    assert_eq!(routes(&["scan", "--depth", "1"]), vec!["alpha", "beta"]);
    assert_eq!(
        routes(&["scan"]),
        vec!["alpha", "beta", "org/team/deep"],
        "{DEFAULT_DEPTH} levels"
    );
    assert_eq!(
        routes(&["scan", "--depth", "5"]),
        vec!["a/b/c/d/deeper", "alpha", "beta", "org/team/deep"]
    );
}

#[test]
fn scan_of_folders_holding_nothing_says_where_it_looked() {
    let dir = workspace_with_two();
    let empty = dir.path().join("empty");
    fs::create_dir_all(empty.join("plain")).unwrap();

    let outcome = flayer(dir.path(), &["scan", "empty"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "no mind projects or git repositories under {}\n",
            empty.display()
        )
    );
}

#[test]
fn scan_of_a_folder_that_is_not_there_warns_and_still_reports_the_rest() {
    let dir = workspace_with_two();

    let outcome = flayer(dir.path(), &["scan", "wrok", "."]).unwrap();

    assert!(!outcome.ok);
    assert_eq!(outcome.stdout, "alpha  linked\nbeta   linked\n");
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    assert!(
        outcome.stderr[0].starts_with(&format!(
            "warning: {}: cannot be listed",
            dir.path().join("wrok").display()
        )),
        "{:?}",
        outcome.stderr
    );
}

/// What `scan --json` holds, field by field: nothing else is allowed, so a
/// name changed in core fails here first. Every name is a contract.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    roots: Vec<String>,
    depth: usize,
    found: Vec<Found>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Found {
    path: String,
    route: String,
    id: Option<String>,
    mind: bool,
    linked: bool,
    moved: Option<String>,
}

#[test]
fn scan_json_is_the_envelope_with_each_one_found_and_its_id() {
    let dir = workspace_with_two();
    flayer(dir.path(), &["unlink", "beta"]).unwrap();
    fs::create_dir_all(dir.path().join("gamma/.git")).unwrap();

    let outcome = flayer(dir.path(), &["scan", "--json", "--depth", "2"]).unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(report.roots, vec![dir.path().display().to_string()]);
    assert_eq!(report.depth, 2);
    assert!(!report.truncated);
    let id = |name: &str| {
        MindProject::open(dir.path().join(name))
            .unwrap()
            .settings()
            .map(|settings| settings.id.to_string())
    };
    // Each one found, field by field.
    type Seen<'a> = (
        &'a str,
        &'a str,
        Option<String>,
        bool,
        bool,
        Option<&'a str>,
    );
    let found: Vec<Seen> = report
        .found
        .iter()
        .map(|found| {
            (
                found.path.as_str(),
                found.route.as_str(),
                found.id.clone(),
                found.mind,
                found.linked,
                found.moved.as_deref(),
            )
        })
        .collect();
    let path = |name: &str| dir.path().join(name).display().to_string();
    let (alpha, beta, gamma) = (path("alpha"), path("beta"), path("gamma"));
    assert_eq!(
        found,
        vec![
            (alpha.as_str(), "alpha", id("alpha"), true, true, None),
            (beta.as_str(), "beta", id("beta"), true, false, None),
            (gamma.as_str(), "gamma", None, false, false, None),
        ]
    );
    assert!(id("alpha").is_some());
}

#[test]
fn scan_json_says_what_it_could_not_read_and_that_it_stopped_as_warnings() {
    let dir = workspace_with_two();
    let crowded = dir.path().join("crowded");
    // The root and every folder in it but the last are listed, which is
    // the whole budget.
    for index in 0..MAX_LISTED {
        fs::create_dir_all(crowded.join(format!("d{index:04}"))).unwrap();
    }

    let outcome = flayer(dir.path(), &["scan", "--json", "wrok", "crowded"]).unwrap();

    // Failed as the text would have, and nothing said beside the envelope.
    assert!(!outcome.ok);
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert!(report.truncated);
    assert!(report.found.is_empty(), "{:?}", report.found);
    assert_eq!(
        report.roots,
        vec![
            dir.path().join("wrok").display().to_string(),
            crowded.display().to_string()
        ]
    );
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("wrok: cannot be listed"),
        "{warnings:?}"
    );
    assert!(
        warnings[1].starts_with(&format!("stopped after listing {MAX_LISTED} folders")),
        "{warnings:?}"
    );
}

#[test]
fn a_scan_that_stops_early_says_so_on_stderr() {
    let dir = workspace_with_two();
    let crowded = dir.path().join("crowded");
    for index in 0..MAX_LISTED {
        fs::create_dir_all(crowded.join(format!("d{index:04}"))).unwrap();
    }
    fs::create_dir_all(dir.path().join("other/alpha/.git")).unwrap();

    let outcome = flayer(dir.path(), &["scan", "crowded", "other"]).unwrap();

    assert!(!outcome.ok);
    // The second folder is still looked under.
    assert!(
        outcome.stdout.starts_with("other/alpha  not linked"),
        "{}",
        outcome.stdout
    );
    assert_eq!(
        outcome.stderr,
        vec![format!(
            "warning: stopped after listing {MAX_LISTED} folders; \
             name a folder further down to scan past it, or link one by its path"
        )]
    );
}

#[test]
fn flayer_scan_looks_three_levels_down_unless_told() {
    let parsed = FlayerCli::try_parse_from(["flayer", "scan"]).unwrap();

    assert!(matches!(
        parsed.command,
        Some(mindflayer_cli::FlayerCommand::Scan {
            ref roots,
            depth: DEFAULT_DEPTH,
            json: false,
        }) if roots.is_empty()
    ));
}
