//! What doctor finds in a project and around it, a check a test: each
//! project, each agent home and each variable set built here, in a
//! temporary folder, never the real home or an agent's own.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::doctor::{Examined, Machine, Report};
use mindweaver_core::{Agent, AgentHomes, Directories, MindProject, Share, WeaverWorkspace};
use tempfile::TempDir;

/// A workspace at the root of a git work tree, linking `app`, a project
/// below it whose settings name `agents`, and a home of its own holding
/// each agent's folder, empty.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
}

fn fixture(agents: &[Agent]) -> Fixture {
    fixture_in(agents, true)
}

/// [`fixture`], in a git work tree only when `git`.
fn fixture_in(agents: &[Agent], git: bool) -> Fixture {
    let dir = TempDir::new().unwrap();
    if git {
        gix::init(dir.path()).unwrap();
    }
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, agents, Share::Local, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    Fixture {
        dir,
        workspace,
        project,
    }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.project.root()
    }

    /// The fixture's own home, where each agent's folder is.
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// Every agent's folder in the fixture's home, none of them made.
    fn homes(&self) -> AgentHomes {
        AgentHomes::defaults_under(&self.home())
    }

    /// Codex's own config in the fixture's home.
    fn codex_config(&self) -> PathBuf {
        self.homes().codex_home.unwrap().join("config.toml")
    }

    fn write(&self, path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// What doctor finds in `app`, with the fixture's homes and `set` the
    /// variables set.
    fn examined_with(&self, set: &[&str]) -> Examined {
        let machine = Machine::new(self.homes(), set.iter().map(|name| (*name).to_owned()));
        let mut report = Report::of_project(&self.project, Some(&self.workspace), &machine);
        report.projects.remove(0)
    }

    fn examined(&self) -> Examined {
        self.examined_with(&[])
    }
}

/// The code of each finding, in order.
fn codes(examined: &Examined) -> Vec<&'static str> {
    examined
        .findings
        .iter()
        .map(|finding| finding.code)
        .collect()
}

/// Trust `root` in Codex's config, as `level`.
fn trusting(root: &Path, level: &str) -> String {
    format!(
        "[projects.\"{}\"]\ntrust_level = \"{level}\"\n",
        root.display()
    )
}

#[test]
fn a_codex_project_with_a_config_nothing_trusts_is_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(&fixture.codex_config(), "model = \"o4\"\n");

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_untrusted"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, fixture.codex_config());
    assert_eq!(finding.agents, vec![Agent::Codex]);
    assert!(
        finding.fix.contains(&format!(
            "`[projects.\"{}\"]` in {}",
            fixture.root().display(),
            fixture.codex_config().display()
        )),
        "{}",
        finding.fix
    );
    assert!(examined.failures.is_empty(), "{:?}", examined.failures);
}

#[test]
fn a_codex_project_trusted_or_with_no_config_is_not_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.codex_config(), "");
    assert!(codes(&fixture.examined()).is_empty());

    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "trusted"),
    );
    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_codex_project_its_user_marks_untrusted_is_found_whatever_it_holds() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "untrusted"),
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_distrusted"]);
    assert_eq!(examined.findings[0].path, fixture.codex_config());
}

#[test]
fn trust_is_not_asked_of_a_project_that_does_not_name_codex() {
    let fixture = fixture(&[Agent::Claude]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "untrusted"),
    );

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_codex_config_that_does_not_parse_is_a_failure_and_no_finding() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(&fixture.codex_config(), "[projects\n");

    let examined = fixture.examined();

    assert!(codes(&examined).is_empty());
    assert_eq!(examined.failures.len(), 1, "{:?}", examined.failures);
}

#[test]
fn with_no_codex_home_a_config_nothing_trusts_says_so() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    let machine = Machine::new(AgentHomes::default(), Vec::new());

    let report = Report::of_project(&fixture.project, Some(&fixture.workspace), &machine);

    let finding = &report.projects[0].findings[0];
    assert_eq!(finding.code, "doctor_codex_untrusted");
    assert_eq!(finding.path, fixture.root().join(".codex/config.toml"));
    assert!(finding.fix.contains("--codex-home"), "{}", finding.fix);
}

#[test]
fn every_project_a_workspace_links_is_examined_once_and_a_stale_one_says_why() {
    let mut fixture = fixture(&[Agent::Claude]);
    let gone = fixture.dir.path().join("gone");
    let (other, _) = MindProject::init_settings(
        &gone,
        &[Agent::Claude],
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    fixture.workspace.link(&other).unwrap();
    fs::remove_dir_all(&gone).unwrap();
    let machine = Machine::new(fixture.homes(), Vec::new());

    let report = Report::of_workspace(&fixture.workspace, &machine);

    let entries: Vec<Option<&str>> = report
        .projects
        .iter()
        .map(|examined| examined.entry.as_deref())
        .collect();
    assert_eq!(entries, vec![Some("app"), Some("gone")]);
    assert!(report.projects[0].failures.is_empty());
    assert_eq!(report.projects[1].name, None);
    assert_eq!(report.projects[1].failures.len(), 1);
}
