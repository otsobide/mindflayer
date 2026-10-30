//! Bringing what was installed up to date with where it came from, and
//! putting back what has gone (D-04): planned without writing, applied
//! under the workspace's lock, an edit left alone unless forced and an
//! orphan said rather than refused.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::copy;
use mindflayer_core::install::{self, Edits, Installed, Standing};
use mindflayer_core::status::State;
use mindflayer_core::update::{self, Applied, Does, Plan, Step, Updates};
use mindflayer_core::{
    claude_rules, create, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, Ledger,
    MindProject, Reference, Share,
};
use tempfile::TempDir;

/// A workspace linking the projects it names, below it, each marked by
/// settings naming every agent.
struct Given {
    dir: TempDir,
    workspace: FlayerWorkspace,
    projects: Vec<MindProject>,
    ledger: Ledger,
}

/// [`Given`] with one project, `app`, shared.
fn given() -> Given {
    given_with(&["app"], Share::Shared)
}

fn given_with(names: &[&str], share: Share) -> Given {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let mut projects = Vec::new();
    for name in names {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        let (project, _) =
            MindProject::init_settings(&root, &Agent::ALL, share, &Directories::default()).unwrap();
        workspace.link(&project).unwrap();
        projects.push(project);
    }
    let ledger = workspace.ledger().unwrap();
    Given {
        dir,
        workspace,
        projects,
        ledger,
    }
}

impl Given {
    fn project(&self) -> &MindProject {
        &self.projects[0]
    }

    /// The workspace's own skill `name`, its folder.
    fn skill(&self, name: &str) -> PathBuf {
        create(
            &self.workspace.own(),
            Kind::Skill,
            name,
            "From the workspace",
        )
        .unwrap();
        self.offered(name)
    }

    fn offered(&self, name: &str) -> PathBuf {
        self.workspace
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join(name)
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

    fn install_into(&self, project: &MindProject, kind: Kind, name: &str) {
        let candidate =
            install::offered(&self.workspace, &self.ledger, project, kind, name, None).unwrap();
        install::install(
            &self.workspace,
            &self.ledger,
            project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }

    fn install(&self, kind: Kind, name: &str) {
        self.install_into(self.project(), kind, name);
    }

    fn standing(&self, kind: Kind, name: &str) -> Standing {
        install::standings(&self.workspace, &self.ledger, self.project(), kind)
            .unwrap()
            .into_iter()
            .find(|(installed, _)| installed == name)
            .map_or(Standing::Absent, |(_, standing)| standing)
    }

    fn plan(&self, typed: &[&str], edits: Edits) -> Plan {
        update::plan(&self.workspace, &self.projects, &references(typed), edits).unwrap()
    }

    fn apply(&self, typed: &[&str], edits: Edits) -> Applied {
        update::apply(
            &self.workspace,
            &self.projects,
            &references(typed),
            edits,
            &AgentHomes::default(),
        )
        .unwrap()
    }

    /// A skill's copies in the first project, in `.agents/skills` and
    /// `.claude/skills`.
    fn copies(&self, name: &str) -> [PathBuf; 2] {
        copies(self.project(), name)
    }

    fn agents_md(&self) -> PathBuf {
        self.project().root().join("AGENTS.md")
    }

    fn claude_file(&self, route: &str) -> PathBuf {
        self.project().root().join(claude_rules::file_for(route))
    }
}

fn references(typed: &[&str]) -> Vec<Reference> {
    typed.iter().map(|typed| Reference::parse(typed)).collect()
}

fn copies(project: &MindProject, name: &str) -> [PathBuf; 2] {
    let root = project.root();
    [
        root.join(".agents/skills").join(name),
        root.join(".claude/skills").join(name),
    ]
}

/// What somebody's editor does: add a line to the file.
fn type_into(file: &Path) {
    let mut text = fs::read_to_string(file).unwrap();
    text.push_str("Mine.\n");
    fs::write(file, text).unwrap();
}

/// Every installation a plan names, as `(project, name, step)`.
fn steps(plan: &Plan) -> Vec<(&str, &str, &Step)> {
    plan.projects
        .iter()
        .flat_map(|project| {
            project
                .installations
                .iter()
                .map(move |update| (project.name.as_str(), update.name.as_str(), &update.step))
        })
        .collect()
}

/// Whether the copy at `copy` holds what the skill at `offer` does, names
/// and bytes alike.
fn same(offer: &Path, copy: &Path) -> bool {
    copy::same_leaving(offer, copy, &[]).unwrap()
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn an_outdated_skill_is_written_again_in_every_copy_and_then_there_is_nothing_to_do() {
    let given = given();
    let offer = given.skill("deploy");
    given.skill("review");
    given.install(Kind::Skill, "deploy");
    given.install(Kind::Skill, "review");
    type_into(&offer.join("SKILL.md"));

    let planned = given.plan(&[], Edits::Kept);
    let applied = given.apply(&[], Edits::Kept);
    let again = given.plan(&[], Edits::Kept);

    assert_eq!(
        steps(&planned),
        [
            ("app", "deploy", &Step::Refresh),
            ("app", "review", &Step::Keep)
        ]
    );
    let deploy = &applied.plan.projects[0].installations[0];
    assert!(
        matches!(
            deploy.done,
            Some(Ok(ref done)) if matches!(done.installed, Installed::Updated { .. })
        ),
        "{deploy:?}"
    );
    assert!(applied.plan.projects[0].installations[1].done.is_none());
    for copy in given.copies("deploy") {
        assert!(same(&offer, &copy), "{}", copy.display());
    }
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
    assert!(again.is_empty());
}

#[test]
fn a_dry_run_writes_nothing() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");
    type_into(&offer.join("SKILL.md"));
    fs::remove_dir_all(&claude).unwrap();
    let held = fs::read(agents.join("SKILL.md")).unwrap();

    let planned = given.plan(&[], Edits::Kept);

    assert_eq!(
        steps(&planned),
        [("app", "deploy", &Step::PutBack(vec![claude.clone()]))]
    );
    assert!(!claude.exists());
    assert_eq!(fs::read(agents.join("SKILL.md")).unwrap(), held);
}

#[test]
fn a_deleted_copy_is_put_back_byte_for_byte_and_the_other_brought_up_to_date() {
    let given = given();
    let offer = given.skill("deploy");
    fs::write(offer.join("run.sh"), "#!/bin/sh\necho ship\n").unwrap();
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");
    let written = fs::read(agents.join("run.sh")).unwrap();
    fs::remove_dir_all(&agents).unwrap();

    let applied = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&applied.plan),
        [("app", "deploy", &Step::PutBack(vec![agents.clone()]))]
    );
    assert_eq!(fs::read(agents.join("run.sh")).unwrap(), written);
    assert!(same(&claude, &agents));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
}

#[test]
fn every_copy_gone_is_put_back_too() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    for copy in given.copies("deploy") {
        fs::remove_dir_all(copy.parent().unwrap()).unwrap();
    }

    given.apply(&[], Edits::Kept);

    for copy in given.copies("deploy") {
        assert!(same(&offer, &copy), "{}", copy.display());
    }
}

#[test]
fn an_edited_copy_is_left_alone_with_every_other_until_forced() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    let [agents, claude] = given.copies("deploy");
    type_into(&offer.join("SKILL.md"));
    type_into(&claude.join("SKILL.md"));
    let edited = fs::read(claude.join("SKILL.md")).unwrap();
    let behind = fs::read(agents.join("SKILL.md")).unwrap();

    let kept = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&kept.plan),
        [("app", "deploy", &Step::Leave(vec![claude.clone()]))]
    );
    assert!(kept.plan.is_empty());
    assert_eq!(fs::read(claude.join("SKILL.md")).unwrap(), edited);
    assert_eq!(fs::read(agents.join("SKILL.md")).unwrap(), behind);

    let forced = given.apply(&[], Edits::Forced);

    assert_eq!(
        steps(&forced.plan),
        [("app", "deploy", &Step::WriteOver(vec![claude.clone()]))]
    );
    assert!(same(&offer, &claude));
    assert!(same(&offer, &agents));
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
}

#[test]
fn an_orphaned_installation_is_said_and_left_as_the_project_holds_it() {
    let given = given();
    let offer = given.skill("deploy");
    given.skill("review");
    given.install(Kind::Skill, "deploy");
    given.install(Kind::Skill, "review");
    let [_, claude] = given.copies("deploy");
    let held = fs::read(claude.join("SKILL.md")).unwrap();
    fs::remove_dir_all(&offer).unwrap();
    type_into(&given.offered("review").join("SKILL.md"));

    let applied = given.apply(&[], Edits::Forced);

    assert_eq!(
        steps(&applied.plan),
        [
            ("app", "deploy", &Step::Orphaned),
            ("app", "review", &Step::Refresh)
        ]
    );
    assert!(applied.plan.projects[0].installations[1]
        .done
        .as_ref()
        .is_some_and(Result::is_ok));
    assert_eq!(fs::read(claude.join("SKILL.md")).unwrap(), held);
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Orphaned);
}

#[test]
fn an_edited_copy_nothing_offers_any_more_is_orphaned_even_forced() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    let [_, claude] = given.copies("deploy");
    type_into(&claude.join("SKILL.md"));
    fs::remove_dir_all(&offer).unwrap();

    let planned = given.plan(&[], Edits::Forced);

    assert_eq!(steps(&planned), [("app", "deploy", &Step::Orphaned)]);
}

#[test]
fn references_pick_what_is_updated_and_one_naming_nothing_is_said() {
    let given = given();
    let deploy = given.skill("deploy");
    let review = given.skill("review");
    given.install(Kind::Skill, "deploy");
    given.install(Kind::Skill, "review");
    type_into(&deploy.join("SKILL.md"));
    type_into(&review.join("SKILL.md"));

    let applied = given.apply(&["skill:deploy", "rule:deploy", "typo"], Edits::Kept);

    assert_eq!(steps(&applied.plan), [("app", "deploy", &Step::Refresh)]);
    assert_eq!(applied.plan.unmatched, ["rule:deploy", "typo"]);
    assert_eq!(given.standing(Kind::Skill, "deploy"), Standing::Current);
    assert_eq!(given.standing(Kind::Skill, "review"), Standing::Outdated);
}

#[test]
fn a_record_from_before_origins_is_put_back_from_the_place_its_copy_tells_or_left() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    // A workspace it loads offers one by the same name, unlike its own.
    let team = given.dir.path().join("team");
    fs::create_dir(&team).unwrap();
    let (team, _) = FlayerWorkspace::init(&team).unwrap();
    create(&team.own(), Kind::Skill, "deploy", "From the team").unwrap();
    let mut workspace = FlayerWorkspace::open(given.dir.path()).unwrap();
    workspace.load(&team).unwrap();
    let given = Given { workspace, ..given };
    // As a ledger from before origins and hashes were kept holds it.
    let forget = || {
        rusqlite::Connection::open(given.workspace.ledger_path())
            .unwrap()
            .execute(
                "UPDATE installations SET origin = NULL, content_hash = NULL",
                [],
            )
            .unwrap();
    };
    let [agents, claude] = given.copies("deploy");
    forget();
    fs::remove_dir_all(&claude).unwrap();

    let told = given.apply(&[], Edits::Kept);
    let put_back = same(&offer, &claude);
    forget();
    fs::remove_dir_all(&agents).unwrap();
    fs::remove_dir_all(&claude).unwrap();
    let untold = given.plan(&[], Edits::Kept);

    // The copy left holds the workspace's own, so that is where it came
    // from; once none is left, which of the two it was is not guessed.
    assert_eq!(
        steps(&told.plan),
        [("app", "deploy", &Step::PutBack(vec![claude.clone()]))]
    );
    assert!(put_back);
    match &steps(&untold)[..] {
        [("app", "deploy", Step::Unsure(origins))] => {
            assert_eq!(origins.len(), 2, "{origins:?}");
            assert_eq!(origins[0], "workspace");
            assert!(origins[1].starts_with("load:"), "{origins:?}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_project_asked_is_updated_each_once() {
    let given = given_with(&["app", "web"], Share::Shared);
    let offer = given.skill("deploy");
    for project in &given.projects {
        given.install_into(project, Kind::Skill, "deploy");
    }
    type_into(&offer.join("SKILL.md"));
    let mut asked = given.projects.clone();
    asked.push(MindProject::open(given.dir.path().join("app")).unwrap());

    let applied = update::apply(
        &given.workspace,
        &asked,
        &[],
        Edits::Kept,
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(
        steps(&applied.plan),
        [
            ("app", "deploy", &Step::Refresh),
            ("web", "deploy", &Step::Refresh)
        ]
    );
    for project in &given.projects {
        for copy in copies(project, "deploy") {
            assert!(same(&offer, &copy), "{}", copy.display());
        }
    }
}

#[test]
fn a_local_project_a_git_clean_took_is_marked_again_and_its_copies_put_back_out_of_git() {
    let given = given_with(&["app"], Share::Local);
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    let root = given.project().root().to_path_buf();
    let exclude = given.dir.path().join(".git/info/exclude");
    // What a `git clean -dX` takes: every folder git ignores, the project's
    // `.mind` among them, since a local project keeps it out of git whole.
    // A `git clean -X` takes none of them, folders it does not go into.
    for folder in [".agents", ".claude", ".mind"] {
        fs::remove_dir_all(root.join(folder)).unwrap();
    }
    assert!(MindProject::open(&root).is_err());
    // Marked again as it was, with an id of its own.
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
            .unwrap();
    let new = project.settings().unwrap().id.to_string();

    update::apply(
        &given.workspace,
        &[project],
        &[],
        Edits::Kept,
        &AgentHomes::default(),
    )
    .unwrap();

    for copy in given.copies("deploy") {
        assert!(same(&offer, &copy), "{}", copy.display());
    }
    // Kept out of git under the new id; the old id's group is left behind
    // beside it (architecture.md, open questions), naming the same places.
    let excluded = fs::read_to_string(&exclude).unwrap();
    let group =
        format!("# project {new}\n/app/.agents/skills/deploy/\n/app/.claude/skills/deploy/\n");
    assert!(excluded.contains(&group), "{excluded}");
}

#[test]
fn a_workspace_that_never_installed_anything_has_nothing_to_update_and_gets_no_ledger() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();

    let applied = update::apply(
        &workspace,
        &[project],
        &[],
        Edits::Kept,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(applied.plan.is_empty());
    assert!(applied.plan.projects[0].installations.is_empty());
    assert!(!workspace.ledger_path().exists());
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

const NO_FORCE_PUSH: &str = "Never force-push a branch somebody else may have pulled.\n";
const ERRORS: &str = "Return errors as values.\n";
const COMMENTS: &str = "Comments say why, not what.\n";

#[test]
fn a_projects_rules_are_brought_up_to_date_together_an_orphan_kept_as_it_is() {
    let given = given();
    let push = given.rule("git/no-force-push", NO_FORCE_PUSH);
    let errors = given.rule("rust/errors", ERRORS);
    let comments = given.rule("style/comments", COMMENTS);
    for route in ["git/no-force-push", "rust/errors", "style/comments"] {
        given.install(Kind::Rule, route);
    }
    fs::write(&push, "Never force-push, ever.\n").unwrap();
    fs::write(&errors, "Return errors; never panic.\n").unwrap();
    fs::remove_file(&comments).unwrap();

    let applied = given.apply(&["rule:git/no-force-push"], Edits::Kept);

    // The one asked for, the other outdated one with it, and not the
    // orphan, which nothing could write.
    assert_eq!(
        steps(&applied.plan),
        [
            ("app", "git/no-force-push", &Step::Refresh),
            ("app", "rust/errors", &Step::Refresh)
        ]
    );
    let rendered = applied.plan.projects[0].rendered.as_ref().unwrap();
    assert!(rendered.is_ok(), "{rendered:?}");
    let block = fs::read_to_string(given.agents_md()).unwrap();
    assert!(block.contains("Never force-push, ever."), "{block}");
    assert!(block.contains("Return errors; never panic."), "{block}");
    assert!(block.contains(COMMENTS), "{block}");
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Current
    );
    assert_eq!(given.standing(Kind::Rule, "rust/errors"), Standing::Current);
    assert_eq!(
        given.standing(Kind::Rule, "style/comments"),
        Standing::Orphaned
    );
}

#[test]
fn a_rule_edited_in_its_section_is_kept_unless_forced() {
    let given = given();
    let push = given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.install(Kind::Rule, "git/no-force-push");
    let edited = fs::read_to_string(given.agents_md())
        .unwrap()
        .replace(NO_FORCE_PUSH, "Ours now.\n");
    fs::write(given.agents_md(), &edited).unwrap();
    fs::write(&push, "Never force-push, ever.\n").unwrap();

    let kept = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&kept.plan),
        [(
            "app",
            "git/no-force-push",
            &Step::Leave(vec![given.agents_md()])
        )]
    );
    assert!(kept.plan.projects[0].rendered.is_none());
    assert_eq!(fs::read_to_string(given.agents_md()).unwrap(), edited);

    let forced = given.apply(&[], Edits::Forced);

    let rendered = forced.plan.projects[0].rendered.as_ref().unwrap();
    assert!(rendered.is_ok(), "{rendered:?}");
    let block = fs::read_to_string(given.agents_md()).unwrap();
    assert!(block.contains("Never force-push, ever."), "{block}");
    assert!(!block.contains("Ours now."), "{block}");
    assert_eq!(
        given.standing(Kind::Rule, "git/no-force-push"),
        Standing::Current
    );
}

/// [`given`], with a `CLAUDE.local.md` and no `CLAUDE.md`, so Claude reads
/// no block and each rule has a section and a file for Claude, and two
/// rules installed, `alpha/one` and `alpha/two`, each changed where it is
/// offered since: their offers.
fn two_rules_in_both_copies() -> (Given, PathBuf, PathBuf) {
    let given = given();
    fs::write(given.project().root().join("CLAUDE.local.md"), "Hers.\n").unwrap();
    let one = given.rule("alpha/one", "Rule one.\n");
    let two = given.rule("alpha/two", "Rule two.\n");
    given.install(Kind::Rule, "alpha/one");
    given.install(Kind::Rule, "alpha/two");
    assert!(given.claude_file("alpha/one").is_file());
    assert!(fs::read_to_string(given.agents_md())
        .unwrap()
        .contains("Rule one.\n"));
    fs::write(&one, "Rule one, changed.\n").unwrap();
    fs::write(&two, "Rule two, changed.\n").unwrap();
    (given, one, two)
}

#[test]
fn a_rule_edited_in_its_file_for_claude_keeps_its_section_while_another_is_updated() {
    let (given, _, _) = two_rules_in_both_copies();
    type_into(&given.claude_file("alpha/one"));
    let edited = fs::read(given.claude_file("alpha/one")).unwrap();

    let applied = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&applied.plan),
        [
            (
                "app",
                "alpha/one",
                &Step::Leave(vec![given.claude_file("alpha/one")])
            ),
            ("app", "alpha/two", &Step::Refresh)
        ]
    );
    let rendered = applied.plan.projects[0].rendered.as_ref().unwrap();
    assert!(rendered.is_ok(), "{rendered:?}");
    // Every copy of the edited rule as it was, so no agent reads the rule
    // as offered now while Claude reads somebody's edit of it.
    let block = fs::read_to_string(given.agents_md()).unwrap();
    assert!(block.contains("Rule one.\n"), "{block}");
    assert!(!block.contains("Rule one, changed."), "{block}");
    assert_eq!(fs::read(given.claude_file("alpha/one")).unwrap(), edited);
    assert!(block.contains("Rule two, changed."), "{block}");
    assert_eq!(
        fs::read_to_string(given.claude_file("alpha/two")).unwrap(),
        "Rule two, changed.\n"
    );
    assert_eq!(given.standing(Kind::Rule, "alpha/one"), Standing::Edited);
    assert_eq!(given.standing(Kind::Rule, "alpha/two"), Standing::Current);

    let forced = given.apply(&["rule:alpha/one"], Edits::Forced);

    let rendered = forced.plan.projects[0].rendered.as_ref().unwrap();
    assert!(rendered.is_ok(), "{rendered:?}");
    let block = fs::read_to_string(given.agents_md()).unwrap();
    assert!(block.contains("Rule one, changed."), "{block}");
    assert_eq!(
        fs::read_to_string(given.claude_file("alpha/one")).unwrap(),
        "Rule one, changed.\n"
    );
    assert_eq!(given.standing(Kind::Rule, "alpha/one"), Standing::Current);
}

#[test]
fn a_rule_edited_in_its_section_keeps_its_file_for_claude_while_another_is_updated() {
    let (given, _, _) = two_rules_in_both_copies();
    let edited = fs::read_to_string(given.agents_md())
        .unwrap()
        .replace("Rule one.\n", "Ours now.\n");
    fs::write(given.agents_md(), &edited).unwrap();

    // Named the other only, so the edited one is not asked, and still
    // rendered with it.
    let applied = given.apply(&["rule:alpha/two"], Edits::Kept);

    assert_eq!(steps(&applied.plan), [("app", "alpha/two", &Step::Refresh)]);
    let rendered = applied.plan.projects[0].rendered.as_ref().unwrap();
    assert!(rendered.is_ok(), "{rendered:?}");
    let block = fs::read_to_string(given.agents_md()).unwrap();
    assert!(block.contains("Ours now.\n"), "{block}");
    assert!(block.contains("Rule two, changed."), "{block}");
    assert_eq!(
        fs::read_to_string(given.claude_file("alpha/one")).unwrap(),
        "Rule one.\n"
    );
    assert_eq!(given.standing(Kind::Rule, "alpha/one"), Standing::Edited);
    assert_eq!(given.standing(Kind::Rule, "alpha/two"), Standing::Current);
}

#[test]
fn a_rules_section_and_its_file_for_claude_gone_are_put_back() {
    let given = given();
    given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.rule(
        "rust/errors",
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors.\n",
    );
    given.install(Kind::Rule, "git/no-force-push");
    given.install(Kind::Rule, "rust/errors");
    let block = fs::read(given.agents_md()).unwrap();
    let file = fs::read(given.claude_file("rust/errors")).unwrap();
    fs::remove_file(given.agents_md()).unwrap();
    fs::remove_file(given.claude_file("rust/errors")).unwrap();

    let applied = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&applied.plan),
        [
            (
                "app",
                "git/no-force-push",
                &Step::PutBack(vec![given.agents_md()])
            ),
            (
                "app",
                "rust/errors",
                &Step::PutBack(vec![given.claude_file("rust/errors")])
            )
        ]
    );
    assert_eq!(fs::read(given.agents_md()).unwrap(), block);
    assert_eq!(fs::read(given.claude_file("rust/errors")).unwrap(), file);
}

#[test]
fn a_rule_copied_into_a_project_a_mind_toml_marks_is_updated_as_its_copy() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("app")).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    let given = Given {
        dir,
        workspace,
        projects: vec![project],
        ledger,
    };
    let offer = given.rule("git/no-force-push", NO_FORCE_PUSH);
    given.install(Kind::Rule, "git/no-force-push");
    fs::write(&offer, "Never force-push, ever.\n").unwrap();

    let applied = given.apply(&[], Edits::Kept);

    assert_eq!(
        steps(&applied.plan),
        [("app", "git/no-force-push", &Step::Refresh)]
    );
    let copy = given.project().root().join("rules/git/no-force-push.md");
    assert_eq!(
        fs::read_to_string(copy).unwrap(),
        "Never force-push, ever.\n"
    );
}

// ---------------------------------------------------------------------------
// Every project at once, as a front end reads it
// ---------------------------------------------------------------------------

/// `alpha` holding a skill of each state an update says something of, and
/// one current, `beta` holding only a current one, and `gone`, linked and
/// removed since.
fn surveyed() -> Given {
    let given = given_with(&["alpha", "beta", "gone"], Share::Shared);
    for name in ["current", "edited", "missing", "orphaned", "outdated"] {
        given.skill(name);
        given.install(Kind::Skill, name);
    }
    given.install_into(&given.projects[1], Kind::Skill, "current");
    type_into(&given.offered("outdated").join("SKILL.md"));
    type_into(&given.offered("edited").join("SKILL.md"));
    type_into(&given.copies("edited")[1].join("SKILL.md"));
    fs::remove_dir_all(&given.copies("missing")[0]).unwrap();
    fs::remove_dir_all(given.offered("orphaned")).unwrap();
    fs::remove_dir_all(given.projects[2].root()).unwrap();
    given
}

#[test]
fn every_project_is_surveyed_for_what_an_update_does_and_why_it_leaves_what_it_leaves() {
    let given = surveyed();

    let updates = Updates::read(&given.workspace).unwrap();

    let json = serde_json::to_value(&updates).unwrap();
    assert_eq!(
        json["projects"],
        serde_json::json!([{
            "entry": "alpha",
            "name": "alpha",
            "installations": [
                {
                    "kind": "skill",
                    "name": "edited",
                    "origin": "workspace",
                    "state": "edited",
                    "does": "leave",
                    "copies": [".claude/skills/edited"],
                    "left": {
                        "code": "edited",
                        "message": "skill `edited` in alpha was changed since it was installed, \
                            at .claude/skills/edited, so an update leaves it alone, every copy \
                            of it; forcing the update writes over the change",
                    },
                    "forced": true,
                },
                {
                    "kind": "skill",
                    "name": "missing",
                    "origin": "workspace",
                    "state": "missing",
                    "does": "put_back",
                    "copies": [".agents/skills/missing"],
                    "left": null,
                    "forced": false,
                },
                {
                    "kind": "skill",
                    "name": "orphaned",
                    "origin": "workspace",
                    "state": "orphaned",
                    "does": "orphaned",
                    "copies": [],
                    "left": {
                        "code": "no_longer_offered",
                        "message": "skill `orphaned` in alpha is no longer offered by \
                            workspace, so an update leaves it as it is; uninstalling it takes \
                            it out",
                    },
                    "forced": false,
                },
                {
                    "kind": "skill",
                    "name": "outdated",
                    "origin": "workspace",
                    "state": "outdated",
                    "does": "refresh",
                    "copies": [],
                    "left": null,
                    "forced": false,
                },
            ],
            "writes": true,
        }])
    );
    assert_eq!(
        json["states"],
        serde_json::json!([
            {"state": "outdated", "count": 1},
            {"state": "edited", "count": 1},
            {"state": "missing", "count": 1},
            {"state": "orphaned", "count": 1},
        ])
    );
    assert_eq!(json["writes"], true);
    let unread = json["unread"].as_array().unwrap();
    assert_eq!(unread.len(), 1, "{json}");
    assert_eq!(unread[0]["code"], "not_a_project", "{json}");
}

#[test]
fn surveying_every_project_writes_nothing() {
    let given = surveyed();
    let ledger = given.workspace.ledger_path();
    let before = fs::read(&ledger).unwrap();
    let missing = &given.copies("missing")[0];

    Updates::read(&given.workspace).unwrap();

    assert_eq!(fs::read(&ledger).unwrap(), before);
    assert!(!missing.exists());
}

/// What an update leaves alone and has nothing to write from is listed, yet
/// writes nothing: `update --all --yes` would only say it.
#[test]
fn a_workspace_whose_every_installation_is_left_writes_nothing() {
    let given = given();
    let offer = given.skill("deploy");
    given.install(Kind::Skill, "deploy");
    type_into(&given.copies("deploy")[1].join("SKILL.md"));
    fs::remove_dir_all(offer).unwrap();

    let updates = Updates::read(&given.workspace).unwrap();

    assert!(!updates.writes);
    assert!(!updates.projects[0].writes);
    let pending = &updates.projects[0].installations[0];
    assert_eq!(
        serde_json::to_value(pending).unwrap()["left"]["code"],
        "no_longer_offered"
    );
}

/// An edited copy nothing offers any more is orphaned, as an update steps
/// it, and still edited, as it stands: taken out only by a forced
/// uninstall, which it says it needs, as it says the forced update an
/// edited one still offered needs.
#[test]
fn an_edited_copy_is_said_to_need_forcing_whether_or_not_anything_offers_it() {
    let given = given();
    for name in ["kept", "dropped"] {
        given.skill(name);
        given.install(Kind::Skill, name);
        type_into(&given.copies(name)[1].join("SKILL.md"));
    }
    fs::remove_dir_all(given.offered("dropped")).unwrap();

    let updates = Updates::read(&given.workspace).unwrap();

    let said: Vec<(&str, State, Does, bool)> = updates.projects[0]
        .installations
        .iter()
        .map(|pending| {
            (
                pending.name.as_str(),
                pending.state,
                pending.does,
                pending.forced,
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("dropped", State::Edited, Does::Orphaned, true),
            ("kept", State::Edited, Does::Leave, true),
        ]
    );
}

#[test]
fn a_workspace_with_nothing_to_update_lists_no_project_and_counts_none() {
    let given = given();
    given.skill("deploy");
    given.install(Kind::Skill, "deploy");

    let updates = Updates::read(&given.workspace).unwrap();

    assert!(updates.projects.is_empty());
    assert!(!updates.writes);
    assert!(updates.states.iter().all(|counted| counted.count == 0));
}
