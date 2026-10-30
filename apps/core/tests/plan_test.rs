//! A batch of assignments planned in core (#78): which offer of a name a
//! project holds, and what a selection of boxes amounts to.

mod common;

use std::fs;

use common::skill;
use mindflayer_core::install::{self, Candidate};
use mindflayer_core::ledger::Ledger;
use mindflayer_core::{
    create, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, MindProject, Share,
};
use tempfile::TempDir;

/// Under one directory: `home`, a workspace with a skill of its own,
/// `deploy`, linking a project for each of `names`, each naming every agent;
/// and `team`, a workspace it loads, with a `deploy` of its own too.
struct Fixture {
    _dir: TempDir,
    workspace: FlayerWorkspace,
    team: FlayerWorkspace,
    projects: Vec<MindProject>,
    ledger: Ledger,
}

fn given(names: &[&str]) -> Fixture {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path().join("home")).unwrap();
    let mut projects = Vec::new();
    for name in names {
        let root = dir.path().join("home").join(name);
        fs::create_dir(&root).unwrap();
        let (project, _) =
            MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
                .unwrap();
        workspace.link(&project).unwrap();
        projects.push(project);
    }
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    create(&team.own(), Kind::Skill, "deploy", "Ship it the team's way").unwrap();
    workspace.load(&team).unwrap();
    let ledger = workspace.ledger().unwrap();
    Fixture {
        _dir: dir,
        workspace,
        team,
        projects,
        ledger,
    }
}

impl Fixture {
    /// Every skill `project` is offered, as the install screen lists them.
    fn surveyed(&self, project: usize) -> Vec<Candidate> {
        install::survey(
            &self.workspace,
            &self.ledger,
            &self.projects[project],
            Kind::Skill,
        )
        .unwrap()
    }

    /// Install `name` into `project` from `from`.
    fn install(&self, project: usize, name: &str, from: &str) {
        let project = &self.projects[project];
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            project,
            Kind::Skill,
            name,
            Some(from),
        )
        .unwrap();
        install::install(
            &self.workspace,
            &self.ledger,
            project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }

    /// Each offer `project` is surveyed for, by its origin, and whether the
    /// project holds it.
    fn held(&self, project: usize) -> Vec<(String, bool)> {
        let candidates = self.surveyed(project);
        let held = install::holding(
            &self.workspace,
            &self.ledger,
            &self.projects[project],
            &candidates,
        )
        .unwrap();
        candidates.iter().map(Candidate::origin).zip(held).collect()
    }
}

/// Write `name`'s manifest in `workspace`'s own folder of skills anew.
fn rewrite(workspace: &FlayerWorkspace, name: &str, description: &str) {
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::write(folder.join(name).join("SKILL.md"), skill(name, description)).unwrap();
}

fn pairs(held: &[(&str, bool)]) -> Vec<(String, bool)> {
    held.iter()
        .map(|(origin, held)| ((*origin).to_owned(), *held))
        .collect()
}

// ---------------------------------------------------------------------------
// Which offer of a name a project holds
// ---------------------------------------------------------------------------

#[test]
fn a_name_the_project_does_not_hold_ticks_none_of_its_offers() {
    let fixture = given(&["alpha"]);

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", false)])
    );
}

#[test]
fn an_installation_ticks_the_offer_it_came_from_though_its_copy_is_neither_offers_now() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "load:../team");
    // The team moves on, so the copy is behind what it came from, and holds
    // what neither place offers: the content alone would tell neither, and
    // tick the first.
    rewrite(&fixture.team, "deploy", "Ship it the team's new way");
    let candidates = fixture.surveyed(0);
    assert_eq!(candidates[1].standing, install::Standing::Outdated);

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", true)])
    );
}

#[test]
fn an_installation_ticks_the_offer_it_came_from_though_another_offers_the_same() {
    let fixture = given(&["alpha"]);
    // Byte for byte the same in both places, so the content alone would
    // tick the first.
    rewrite(&fixture.workspace, "deploy", "Ship it");
    rewrite(&fixture.team, "deploy", "Ship it");
    fixture.install(0, "deploy", "load:../team");

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", true)])
    );
}

#[test]
fn somebodys_copy_ticks_every_offer_of_its_name() {
    let fixture = given(&["alpha"]);
    let theirs = fixture.projects[0].root().join(".claude/skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", true), ("load:../team", true)])
    );
}

#[test]
fn an_installation_whose_copies_have_gone_ticks_none_of_its_offers() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "workspace");
    for folder in [".claude/skills", ".agents/skills"] {
        fs::remove_dir_all(fixture.projects[0].root().join(folder).join("deploy")).unwrap();
    }

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", false)])
    );
}

#[test]
fn an_installation_nothing_it_came_from_offers_ticks_the_first_offer_of_its_name() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "load:../team");
    fs::remove_dir_all(
        fixture
            .team
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join("deploy"),
    )
    .unwrap();

    // Still there, so its box is still ticked, for unticking it to take it
    // out: the only box its name has.
    assert_eq!(fixture.held(0), pairs(&[("workspace", true)]));
}
