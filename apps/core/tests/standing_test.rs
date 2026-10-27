//! How what a project was given stands against what was installed (D-04):
//! current, outdated, edited, missing or orphaned, for a skill's copies, a
//! rule's section of the block in `AGENTS.md`, its file for Claude, and its
//! copy in a project a `mind.toml` marks.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{repository, skill, url};
use mindflayer_core::gather::{self, Request};
use mindflayer_core::install::{self, Installed, Standing};
use mindflayer_core::{
    claude_rules, create, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, Ledger,
    MindProject, Share,
};
use tempfile::TempDir;

/// A workspace and one project it links, below it, whose settings name every
/// agent.
struct Given {
    dir: TempDir,
    workspace: FlayerWorkspace,
    project: MindProject,
    ledger: Ledger,
}

/// [`Given`], with each of `skills` among the workspace's own.
fn given(skills: &[&str]) -> Given {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    for name in skills {
        create(&workspace.own(), Kind::Skill, name, "From the workspace").unwrap();
    }
    let ledger = workspace.ledger().unwrap();
    Given {
        dir,
        workspace,
        project,
        ledger,
    }
}

/// A workspace linking a project a `mind.toml` marks, below it.
fn older() -> Given {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("app")).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    Given {
        dir,
        workspace,
        project,
        ledger,
    }
}

impl Given {
    /// The workspace's own skill `name`'s manifest, which is what it offers.
    fn offered_skill(&self, name: &str) -> PathBuf {
        self.workspace
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join(name)
            .join("SKILL.md")
    }

    /// The workspace's own rule at `route`, holding `source`.
    fn rule(&self, route: &str, source: &str) -> PathBuf {
        let mut file = self.workspace.own().directory_for(Kind::Rule).unwrap();
        file.extend(route.split('/'));
        file.set_extension("md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        file
    }

    fn install(&self, kind: Kind, name: &str) -> Installed {
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            &self.project,
            kind,
            name,
            None,
        )
        .unwrap();
        install::install(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap()
        .installed
    }

    /// How the installation of `name` stands, as the ledger records it.
    fn standing(&self, kind: Kind, name: &str) -> Standing {
        install::standings(&self.workspace, &self.ledger, &self.project, kind)
            .unwrap()
            .into_iter()
            .find(|(installed, _)| installed == name)
            .map_or(Standing::Absent, |(_, standing)| standing)
    }

    /// How each offer of `name` stands, as the install screen surveys it.
    fn surveyed(&self, kind: Kind, name: &str) -> Vec<Standing> {
        install::survey(&self.workspace, &self.ledger, &self.project, kind)
            .unwrap()
            .into_iter()
            .filter(|candidate| candidate.name() == name)
            .map(|candidate| candidate.standing)
            .collect()
    }

    /// A skill's copies, in `.agents/skills` and `.claude/skills`.
    fn copies(&self, name: &str) -> [PathBuf; 2] {
        let root = self.project.root();
        [
            root.join(".agents/skills").join(name),
            root.join(".claude/skills").join(name),
        ]
    }

    fn agents_md(&self) -> PathBuf {
        self.project.root().join("AGENTS.md")
    }

    fn claude_file(&self, route: &str) -> PathBuf {
        self.project.root().join(claude_rules::file_for(route))
    }

    /// Take the hash out of every record, as a ledger from before hashes
    /// were kept holds none.
    fn forget_hashes(&self) {
        rusqlite::Connection::open(self.workspace.ledger_path())
            .unwrap()
            .execute("UPDATE installations SET content_hash = NULL", [])
            .unwrap();
    }

    fn recorded_hash(&self, kind: Kind, name: &str) -> Option<String> {
        self.ledger
            .installation("app", kind, name)
            .unwrap()
            .unwrap()
            .content_hash
    }
}

/// What somebody's editor does: add a line to the file.
fn type_into(file: &std::path::Path) {
    let mut text = fs::read_to_string(file).unwrap();
    text.push_str("Mine.\n");
    fs::write(file, text).unwrap();
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn a_skill_is_absent_until_installed_and_current_once_it_is() {
    let given = given(&["deploy"]);
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Absent);
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Absent]);

    given.install(Kind::Skill, "deploy");

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Current]);
}

#[test]
fn a_skill_whose_offer_changes_is_outdated_until_it_is_installed_again() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");

    type_into(&given.offered_skill("deploy"));

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Outdated);
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Outdated]);
    assert!(matches!(
        given.install(Kind::Skill, "deploy"),
        Installed::Updated { .. }
    ));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
}

#[test]
fn a_hand_edit_of_one_copy_makes_the_skill_edited_and_undoing_it_current_again() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");
    let manifest = agents.join("SKILL.md");
    let written = fs::read_to_string(&manifest).unwrap();

    type_into(&manifest);

    // One copy edited is enough: the one beside it is still what was
    // installed, and the two are one installation.
    assert!(claude.join("SKILL.md").is_file());
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Edited);
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Edited]);

    fs::write(&manifest, written).unwrap();

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
}

#[test]
fn a_file_added_to_an_installed_copy_is_an_edit_too() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let [agents, _] = given.copies("deploy");

    fs::write(agents.join("notes.md"), "Mine.\n").unwrap();

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Edited);
}

#[test]
fn an_edit_is_said_over_an_offer_that_moved_on_and_a_copy_gone() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");

    type_into(&agents.join("SKILL.md"));
    type_into(&given.offered_skill("deploy"));
    fs::remove_dir_all(&claude).unwrap();

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Edited);
}

#[test]
fn a_copy_gone_is_missing_as_is_every_copy_gone_and_an_install_puts_them_back() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");

    fs::remove_dir_all(&claude).unwrap();
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Missing);

    // Every copy gone is still an installation the ledger records, not a
    // skill the project never had.
    fs::remove_dir_all(&agents).unwrap();
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Missing);
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Missing]);

    assert!(matches!(
        given.install(Kind::Skill, "deploy"),
        Installed::Added { .. }
    ));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
}

#[test]
fn a_skill_the_place_it_came_from_no_longer_offers_is_orphaned() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");

    fs::remove_dir_all(given.offered_skill("deploy").parent().unwrap()).unwrap();

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Orphaned);
    // Nothing offers the name any more, so nothing on the install screen
    // carries it.
    assert!(given.surveyed(Kind::Skill, "deploy").is_empty());
}

#[test]
fn an_offer_of_the_name_from_another_place_carries_the_installations_standing() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    gather::gather(&given.workspace, &given.ledger, &Request::git(url(&source))).unwrap();

    fs::remove_dir_all(given.offered_skill("deploy").parent().unwrap()).unwrap();

    // The shelf offers a `deploy`, but not the one installed: the record's
    // origin is the workspace's own.
    assert_eq!(given.surveyed(Kind::Skill, "deploy"), [Standing::Orphaned]);
}

#[test]
fn a_record_from_before_hashes_is_current_only_while_its_copy_matches_its_offer() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    given.forget_hashes();
    let [agents, _] = given.copies("deploy");
    let offer = given.offered_skill("deploy");
    let offered = fs::read_to_string(&offer).unwrap();

    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);

    // With no hash to hold the copy to, an offer that moved on reads the
    // same as an edit, and is taken for one.
    type_into(&offer);
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Edited);
    fs::write(&offer, &offered).unwrap();

    type_into(&agents.join("SKILL.md"));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Edited);
}

#[test]
fn the_first_install_over_a_record_from_before_hashes_stores_its_hash() {
    let given = given(&["deploy"]);
    given.install(Kind::Skill, "deploy");
    let hashed = given.recorded_hash(Kind::Skill, "deploy");
    given.forget_hashes();
    assert_eq!(given.recorded_hash(Kind::Skill, "deploy"), None);

    assert!(matches!(
        given.install(Kind::Skill, "deploy"),
        Installed::Unchanged { .. }
    ));

    assert_eq!(given.recorded_hash(Kind::Skill, "deploy"), hashed);
    // Held to its hash from then on, so an offer that moves on is no edit.
    type_into(&given.offered_skill("deploy"));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Outdated);
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/// A rule about every file, which goes to the block of `AGENTS.md`.
const NO_FORCE_PUSH: &str = "Never force-push a branch somebody else may have pulled.\n";
/// A rule about some files, which goes to a file for Claude alone.
const ERRORS: &str = "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors, never panic.\n";

#[test]
fn a_rule_is_current_once_installed_and_outdated_once_its_offer_changes() {
    let given = given(&[]);
    let offer = given.rule("git/no-force-push", NO_FORCE_PUSH);
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Absent
    );

    given.install(Kind::Rule, "git/no-force-push");
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Current
    );
    assert_eq!(
        given.surveyed(Kind::Rule, "git/no-force-push"),
        [Standing::Current]
    );

    type_into(&offer);
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Outdated
    );
    assert_eq!(
        given.surveyed(Kind::Rule, "git/no-force-push"),
        [Standing::Outdated]
    );

    given.install(Kind::Rule, "git/no-force-push");
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Current
    );
}

#[test]
fn a_section_edited_by_hand_makes_the_rule_edited_and_undoing_it_current_again() {
    let given = given(&[]);
    given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.install(Kind::Rule, "git/no-force-push");
    let rendered = fs::read_to_string(given.agents_md()).unwrap();

    fs::write(
        given.agents_md(),
        rendered.replace(NO_FORCE_PUSH, "Force-push whenever you like.\n"),
    )
    .unwrap();

    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Edited
    );
    assert_eq!(
        given.surveyed(Kind::Rule, "git/no-force-push"),
        [Standing::Edited]
    );

    // A file whose newlines an editor turned into another kind holds the
    // same section, and is no edit of it.
    fs::write(given.agents_md(), rendered.replace('\n', "\r\n")).unwrap();
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Current
    );
}

#[test]
fn a_file_for_claude_edited_by_hand_makes_the_rule_edited() {
    let given = given(&[]);
    given.rule("rust/errors", ERRORS);
    given.install(Kind::Rule, "rust/errors");
    assert_eq!(given.standing(Kind::Rule, "rust/errors"), Standing::Current);

    type_into(&given.claude_file("rust/errors"));

    assert_eq!(given.standing(Kind::Rule, "rust/errors"), Standing::Edited);
}

#[test]
fn a_rules_section_or_file_gone_is_missing() {
    let given = given(&[]);
    given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.rule("rust/errors", ERRORS);
    given.install(Kind::Rule, "git/no-force-push");
    given.install(Kind::Rule, "rust/errors");

    fs::write(given.agents_md(), "# Ours\n").unwrap();
    fs::remove_file(given.claude_file("rust/errors")).unwrap();

    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Missing
    );
    assert_eq!(given.standing(Kind::Rule, "rust/errors"), Standing::Missing);
}

#[test]
fn a_rule_the_place_it_came_from_no_longer_offers_is_orphaned() {
    let given = given(&[]);
    let offer = given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.install(Kind::Rule, "git/no-force-push");

    fs::remove_file(offer).unwrap();

    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Orphaned
    );
}

#[test]
fn a_rules_copy_in_a_project_a_mind_toml_marks_is_told_by_its_own_hash() {
    let given = older();
    given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.install(Kind::Rule, "git/no-force-push");
    let copy = given.project.root().join("rules/git/no-force-push.md");
    assert_eq!(
        given.surveyed(Kind::Rule, "git/no-force-push"),
        [Standing::Current]
    );

    type_into(&copy);
    assert_eq!(
        given.surveyed(Kind::Rule, "git/no-force-push"),
        [Standing::Edited]
    );

    fs::remove_file(&copy).unwrap();
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Missing
    );
    assert!(given.dir.path().join("app").is_dir());
}
