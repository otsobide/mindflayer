//! `mindweaver doctor` and `mind doctor`: what each prints of what keeps a
//! project's agents from what it gives them, its envelope read back name by
//! name, and the exit status a reading command gives for what it found,
//! every agent home a folder in the fixture or none.

mod common;

use common::*;

/// A workspace linking `app`, a project made by `mind init --agents
/// <agents>`, in a git work tree when `git`.
fn linked(agents: &str, git: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("app");
    fs::create_dir(&root).unwrap();
    if git {
        gix::init(&root).unwrap();
    }
    mind(&root, &["init", "--agents", agents]).unwrap();
    weaver(dir.path(), &["link", "app"]).unwrap();
    dir
}

#[test]
fn doctor_prints_each_finding_with_its_fix_and_fails() {
    let dir = linked("codex", false);
    let app = dir.path().join("app");

    let outcome = weaver(dir.path(), &["doctor"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "app: {}: in no git work tree, so Codex reads its AGENTS.md only when started in \
             that folder itself\n  fix: make the project a git repository (`git init`), or \
             start Codex in that folder\n\n1 project checked, 1 finding\n",
            app.display()
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(!outcome.ok, "a finding fails the command, as validate does");
}

#[test]
fn doctor_with_nothing_to_find_says_so_and_succeeds() {
    let dir = linked("claude", true);

    let outcome = weaver(dir.path(), &["doctor"]).unwrap();

    assert_eq!(outcome.stdout, "1 project checked, nothing found\n");
    assert!(outcome.ok);
}

#[test]
fn mind_doctor_says_its_own_project_without_naming_it() {
    let dir = linked("codex", false);
    let app = dir.path().join("app");

    let here = mind(&app, &["doctor"]).unwrap();
    let picked = weaver(dir.path(), &["doctor", "-p", "app"]).unwrap();

    let expected = format!(
        "{}: in no git work tree, so Codex reads its AGENTS.md only when started in that \
         folder itself\n  fix: make the project a git repository (`git init`), or start Codex \
         in that folder\n\n1 project checked, 1 finding\n",
        app.display()
    );
    assert_eq!(here.stdout, expected);
    assert_eq!(picked.stdout, expected);
    assert!(!here.ok && !picked.ok);
}

#[test]
fn a_workspace_that_links_nothing_has_nothing_to_examine() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(dir.path(), &["doctor"]).unwrap();

    assert!(
        outcome.stdout.contains("manages no projects yet"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.ok);
}

/// What `doctor --json` holds, every name it is promised, nothing else.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    projects: Vec<Examined>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Examined {
    entry: Option<String>,
    name: Option<String>,
    path: String,
    findings: Vec<Finding>,
    failures: Vec<Said>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Finding {
    code: String,
    agents: Vec<String>,
    path: String,
    message: String,
    fix: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    code: String,
    message: String,
}

#[test]
fn doctor_json_prints_the_report_in_the_envelope_with_the_text_s_exit_status() {
    let dir = linked("codex", false);
    let app = dir.path().join("app");

    let outcome = weaver(dir.path(), &["doctor", "--json"]).unwrap();

    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert!(warnings.is_empty());
    let [examined] = report.projects.as_slice() else {
        panic!("one project: {report:?}");
    };
    assert_eq!(examined.entry.as_deref(), Some("app"));
    assert_eq!(examined.name.as_deref(), Some("app"));
    assert_eq!(examined.path, app.display().to_string());
    assert!(examined.failures.is_empty());
    let [finding] = examined.findings.as_slice() else {
        panic!("one finding: {examined:?}");
    };
    assert_eq!(finding.code, "doctor_codex_no_git_root");
    assert_eq!(finding.agents, vec!["codex"]);
    assert_eq!(finding.path, app.display().to_string());
    assert!(finding.message.starts_with(&finding.path));
    assert!(finding.fix.contains("git init"));
    assert!(!outcome.ok);
}

#[test]
fn what_doctor_cannot_read_is_a_coded_warning_beside_what_it_found() {
    let dir = linked("codex", true);
    let codex = dir.path().join("codex-home");
    fs::create_dir(&codex).unwrap();
    fs::write(codex.join("config.toml"), "[projects\n").unwrap();
    let home = format!("--codex-home={}", codex.display());

    let outcome = weaver(dir.path(), &["doctor", "--json", &home]).unwrap();

    let (report, warnings): (Report, _) = enveloped(&outcome);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(report.projects[0].failures.len(), 1);
    assert_eq!(
        warning_codes(&outcome),
        vec![Some(report.projects[0].failures[0].code.clone())]
    );
    assert_eq!(report.projects[0].failures[0].message, warnings[0]);
    assert!(warnings[0].contains("config.toml"), "{warnings:?}");
    assert!(!outcome.ok);
}

#[test]
fn doctor_reads_codex_s_trust_from_the_home_its_option_names() {
    let dir = linked("codex", true);
    let app = dir.path().join("app");
    fs::create_dir_all(app.join(".codex")).unwrap();
    fs::write(app.join(".codex/config.toml"), "").unwrap();
    let codex = dir.path().join("codex-home");
    fs::create_dir(&codex).unwrap();
    fs::write(codex.join("config.toml"), "").unwrap();
    let home = format!("--codex-home={}", codex.display());

    let untrusted = weaver(dir.path(), &["doctor", &home]).unwrap();
    fs::write(
        codex.join("config.toml"),
        format!(
            "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            app.display()
        ),
    )
    .unwrap();
    let trusted = weaver(dir.path(), &["doctor", &home]).unwrap();

    assert!(
        untrusted.stdout.starts_with(&format!(
            "app: {}: Codex reads it only in a project it trusts, and {} does not mark {} \
             `trusted`\n",
            app.join(".codex/config.toml").display(),
            codex.join("config.toml").display(),
            app.display()
        )),
        "{}",
        untrusted.stdout
    );
    assert_eq!(trusted.stdout, "1 project checked, nothing found\n");
}

#[test]
fn a_variable_set_where_doctor_runs_is_none_it_finds_and_one_unset_is() {
    let dir = linked("claude", true);
    let app = dir.path().join("app");
    // Set wherever a test runs; the other set nowhere.
    for (name, variable) in [
        ("tools", "PATH"),
        ("tracker", "MINDWEAVER_DOCTOR_TEST_UNSET"),
    ] {
        weaver(
            dir.path(),
            &[
                "add",
                "mcp",
                name,
                "A server",
                "--command",
                "server",
                "--secret-env",
                variable,
            ],
        )
        .unwrap();
        mind(&app, &["install", &format!("mcp:{name}")]).unwrap();
    }

    let outcome = weaver(dir.path(), &["doctor"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "app: {}: `tracker` is given MINDWEAVER_DOCTOR_TEST_UNSET (a secret) where Claude \
             Code starts it, and it is not set here\n  fix: set MINDWEAVER_DOCTOR_TEST_UNSET \
             where the agents start `tracker`: no file holds its value\n\n1 project checked, 1 \
             finding\n",
            app.join(".mcp.json").display()
        )
    );
}
