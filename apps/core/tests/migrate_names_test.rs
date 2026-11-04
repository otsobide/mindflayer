//! Moving a workspace a Mindflayer build made to Mindweaver's names (D-26):
//! what the plan says, what the move does to the workspace and to each
//! project it links, and that it moves nothing it did not write, nothing
//! onto a name taken, and puts back what it did when a step fails.
//!
//! Every workspace and project here is what a Mindflayer build left, made
//! by core's own commands and then left as that build left it by the mother
//! of what it wrote (`MindflayerMother`), never by hand.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::gather::{self, Request};
use mindweaver_core::install::{self, Standing};
use mindweaver_core::migrate::{self, LeftWhy, MigrateError, Refusal};
use mindweaver_core::mother::{GitRepositoryMother, MindflayerMother};
use mindweaver_core::{
    Agent, AgentHomes, Directories, Kind, MindProject, Names, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A rule about some files only, which Claude is given as a file.
fn scoped(says: &str) -> String {
    format!("---\npaths:\n  - \"src/**\"\n---\n{says}\n")
}

/// A workspace a Mindflayer build made, in a git work tree, linking `app`,
/// a local project for Claude and Codex with a `CLAUDE.md` of its own, into
/// which it installed a rule about every file (`team`, a section of the
/// block) and one about some (`rust`, a file for Claude), and onto whose
/// shelf it gathered a skill: everything a move to the new names reaches.
struct Fixture {
    dir: TempDir,
    _source: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let mut workspace = MindflayerMother::create(dir.path());
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("CLAUDE.md"), "# Team\n").unwrap();
    let (project, _) = MindProject::init_settings(
        &root,
        &[Agent::Claude, Agent::Codex],
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    workspace.link(&project).unwrap();
    let rules = workspace.own().directory_for(Kind::Rule).unwrap();
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("team.md"), "Say what a change does first.\n").unwrap();
    fs::write(rules.join("rust.md"), scoped("Return errors.")).unwrap();
    let ledger = workspace.ledger().unwrap();
    for route in ["team", "rust"] {
        let candidate =
            install::offered(&workspace, &ledger, &project, Kind::Rule, route, None).unwrap();
        install::install(
            &workspace,
            &ledger,
            &project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }
    let source = GitRepositoryMother::with_skills(&["deploy"]);
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(GitRepositoryMother::url(&source)),
    )
    .unwrap();
    drop(ledger);
    MindflayerMother::installed_into(&workspace, &project);
    Fixture {
        dir,
        _source: source,
        workspace,
        project,
    }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn app(&self, route: &str) -> PathBuf {
        self.project.root().join(route)
    }

    fn text(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn exclude(&self) -> String {
        self.text(&self.root().join(".git/info/exclude"))
    }

    /// Every file under the folder, by its route, with what it holds, but
    /// the lock a move takes, which is no change of what is written.
    fn files(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, at: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(at).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                // Nor the ledger, which SQLite lays out again as it writes
                // whatever it holds: what it records is compared instead.
                if ["lock", "lock.pid", "mindflayer.db", "mindweaver.db"].contains(&name.as_str()) {
                    continue;
                }
                if path.is_dir() {
                    walk(root, &path, into);
                } else {
                    let route = path.strip_prefix(root).unwrap().to_path_buf();
                    into.insert(route, fs::read(&path).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(self.root(), self.root(), &mut files);
        files
    }

    /// What the ledger records of the project's rules and of the shelf,
    /// by what each says rather than by the bytes of the file, which SQLite
    /// lays out again as it writes, whatever it holds.
    fn records(&self) -> (Vec<Vec<String>>, Vec<String>) {
        let workspace = WeaverWorkspace::open(self.root()).unwrap();
        let ledger = workspace.ledger_for_reading().unwrap();
        let rules = ledger
            .records("app", Kind::Rule)
            .unwrap()
            .into_iter()
            .map(|record| {
                record
                    .targets
                    .into_iter()
                    .map(|target| target.path)
                    .collect()
            })
            .collect();
        let shelf = ledger
            .gathered()
            .unwrap()
            .into_iter()
            .map(|gathered| gathered.path)
            .collect();
        (rules, shelf)
    }

    fn standing(&self, workspace: &WeaverWorkspace, route: &str) -> Standing {
        let ledger = workspace.ledger().unwrap();
        install::standings(workspace, &ledger, &self.project, Kind::Rule)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == route)
            .map_or(Standing::Absent, |(_, standing)| standing)
    }
}

#[test]
fn the_fixture_is_what_a_mindflayer_build_left() {
    let fixture = fixture();

    assert!(fixture.root().join(".mindflayer/flayer.toml").is_file());
    assert!(fixture.root().join(".mindflayer/mindflayer.db").is_file());
    assert!(fixture
        .text(&fixture.app("AGENTS.md"))
        .contains("<!-- mindflayer:begin -->"));
    assert!(fixture
        .text(&fixture.app("CLAUDE.md"))
        .starts_with("<!-- mindflayer:begin -->\n@AGENTS.md\n"));
    assert!(fixture.app(".claude/rules/mindflayer/rust.md").is_file());
    assert!(fixture.exclude().contains("# mindflayer:begin"));
    assert!(fixture
        .exclude()
        .contains("/app/.claude/rules/mindflayer/rust.md"));
    assert!(!fixture.root().join(".mindweaver").exists());
}

#[test]
fn the_plan_says_everything_that_would_move_and_moves_nothing() {
    let fixture = fixture();
    let before = fixture.files();

    let plan = migrate::names_plan(&fixture.workspace).unwrap();

    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    let folder = plan.folder.as_ref().unwrap();
    assert_eq!(folder.from, fixture.root().join(".mindflayer"));
    assert_eq!(folder.to, fixture.root().join(".mindweaver"));
    let renamed: Vec<(PathBuf, PathBuf)> = plan
        .files
        .iter()
        .map(|renamed| (renamed.from.clone(), renamed.to.clone()))
        .collect();
    assert_eq!(
        renamed,
        [
            (
                fixture.root().join(".mindflayer/flayer.toml"),
                fixture.root().join(".mindweaver/weaver.toml")
            ),
            (
                fixture.root().join(".mindflayer/mindflayer.db"),
                fixture.root().join(".mindweaver/mindweaver.db")
            ),
        ]
    );
    let keys: Vec<&str> = plan
        .directories
        .iter()
        .map(|repointed| repointed.key.as_str())
        .collect();
    assert_eq!(keys, ["skills", "rules"]);
    assert_eq!(
        plan.directories[1].to,
        PathBuf::from(".mindweaver/library/rules")
    );
    assert_eq!(plan.shelf, 1);
    let project = &plan.projects[0];
    assert_eq!(
        project.blocks,
        [fixture.app("AGENTS.md"), fixture.app("CLAUDE.md")]
    );
    assert_eq!(project.rules.len(), 1);
    assert_eq!(project.rules[0].route, "rust");
    assert_eq!(
        project.rules[0].to,
        fixture.app(".claude/rules/mindweaver/rust.md")
    );
    assert_eq!(plan.excludes, [fixture.root().join(".git/info/exclude")]);
    assert_eq!(fixture.files(), before);
}

#[test]
fn moving_takes_the_workspace_and_its_project_to_the_new_names() {
    let fixture = fixture();
    let mut workspace = fixture.workspace.clone();

    let moved = migrate::names(&mut workspace).unwrap();

    assert!(!moved.plan.is_empty());
    let root = fixture.root();
    assert!(!root.join(".mindflayer").exists());
    assert_eq!(workspace.names(), Names::Mindweaver);
    assert_eq!(WeaverWorkspace::open(root).unwrap(), workspace);
    let marker = fixture.text(&root.join(".mindweaver/weaver.toml"));
    assert!(
        marker.contains("skills = \".mindweaver/library/skills\""),
        "{marker}"
    );
    assert!(marker.contains("rules = \".mindweaver/library/rules\""));
    assert!(root.join(".mindweaver/mindweaver.db").is_file());
    assert!(!root.join(".mindweaver/mindflayer.db").exists());
    let ledger = workspace.ledger().unwrap();
    let shelf: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.path)
        .collect();
    assert!(
        shelf.iter().all(|path| path.starts_with(".mindweaver/")),
        "{shelf:?}"
    );
    assert!(root.join(&shelf[0]).join("SKILL.md").is_file());

    let agents = fixture.text(&fixture.app("AGENTS.md"));
    assert!(agents.contains("<!-- mindweaver:begin -->"), "{agents}");
    assert!(!agents.contains("mindflayer"), "{agents}");
    assert_eq!(
        fixture.text(&fixture.app("CLAUDE.md")),
        "<!-- mindweaver:begin -->\n@AGENTS.md\n<!-- mindweaver:end -->\n\n# Team\n"
    );
    assert_eq!(
        fixture.text(&fixture.app(".claude/rules/mindweaver/rust.md")),
        scoped("Return errors.")
    );
    assert!(!fixture.app(".claude/rules/mindflayer").exists());
    let exclude = fixture.exclude();
    assert!(exclude.contains("# mindweaver:begin"), "{exclude}");
    assert!(
        exclude.contains("/app/.claude/rules/mindweaver/rust.md"),
        "{exclude}"
    );
    assert!(!exclude.contains("mindflayer"), "{exclude}");
    let paths: Vec<String> = ledger
        .records("app", Kind::Rule)
        .unwrap()
        .into_iter()
        .flat_map(|record| record.targets)
        .map(|target| target.path)
        .collect();
    assert!(
        paths.contains(&String::from(".claude/rules/mindweaver/rust.md")),
        "{paths:?}"
    );
    drop(ledger);
    assert_eq!(fixture.standing(&workspace, "rust"), Standing::Current);
    assert_eq!(fixture.standing(&workspace, "team"), Standing::Current);

    // Moved once, there is nothing left to move.
    assert!(migrate::names_plan(&workspace).unwrap().is_empty());
    let again = migrate::names(&mut workspace).unwrap();
    assert!(again.plan.is_empty());
}

#[test]
fn a_rule_file_changed_since_it_was_written_is_left_where_it_is_with_its_record() {
    let fixture = fixture();
    let edited = fixture.app(".claude/rules/mindflayer/rust.md");
    fs::write(&edited, "Mine now.\n").unwrap();
    let mut workspace = fixture.workspace.clone();

    let plan = migrate::names_plan(&workspace).unwrap();
    assert!(plan.projects[0].rules.is_empty());
    assert_eq!(plan.projects[0].left[0].why, LeftWhy::Edited);
    assert_eq!(plan.projects[0].left[0].path, edited);

    migrate::names(&mut workspace).unwrap();

    assert_eq!(fixture.text(&edited), "Mine now.\n");
    assert!(!fixture.app(".claude/rules/mindweaver/rust.md").exists());
    assert_eq!(fixture.standing(&workspace, "rust"), Standing::Edited);
    assert!(fixture
        .exclude()
        .contains("/app/.claude/rules/mindflayer/rust.md"));
}

#[test]
fn a_name_taken_where_the_folder_would_go_refuses_the_whole_move() {
    let fixture = fixture();
    fs::create_dir(fixture.root().join(".mindweaver")).unwrap();
    let before = fixture.files();
    let mut workspace = fixture.workspace.clone();

    let refused = migrate::names(&mut workspace).unwrap_err();

    match &refused {
        MigrateError::Refused(refusals) => assert!(
            matches!(&refusals[..], [Refusal::Taken { to, .. }] if *to == fixture.root().join(".mindweaver")),
            "{refusals:?}"
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(refused.code(), "migrate_refused");
    assert_eq!(fixture.files(), before);
    assert_eq!(workspace.names(), Names::Mindflayer);
}

#[test]
fn a_rule_file_where_the_carried_one_would_go_refuses_the_whole_move() {
    let fixture = fixture();
    let there = fixture.app(".claude/rules/mindweaver/rust.md");
    fs::create_dir_all(there.parent().unwrap()).unwrap();
    fs::write(&there, "Somebody's.\n").unwrap();
    let before = fixture.files();
    let mut workspace = fixture.workspace.clone();

    let refused = migrate::names(&mut workspace).unwrap_err();

    assert!(
        matches!(&refused, MigrateError::Refused(refusals) if matches!(&refusals[..], [Refusal::Taken { to, .. }] if *to == there)),
        "{refused:?}"
    );
    assert_eq!(fixture.files(), before);
}

#[test]
fn a_ledger_another_process_has_open_is_not_moved() {
    let fixture = fixture();
    // Another connection holds it open, as a panel's read in flight does,
    // keeping its log beside it.
    let ledger = fixture.root().join(".mindflayer/mindflayer.db");
    let held = mindweaver_core::Ledger::open_existing(&ledger)
        .unwrap()
        .unwrap();
    held.gathered().unwrap();

    let plan = migrate::names_plan(&fixture.workspace).unwrap();

    assert!(
        matches!(&plan.refusals[..], [Refusal::LedgerOpen { .. }]),
        "{:?}",
        plan.refusals
    );
    assert_eq!(plan.refusals[0].code(), "migrate_ledger_open");
    drop(held);
    assert!(migrate::names_plan(&fixture.workspace)
        .unwrap()
        .refusals
        .is_empty());
}

/// The last step, the folder's own rename, failing, as it does in a root
/// nobody may write in: every step before it is put back, so the workspace
/// and its project read as a Mindflayer build left them.
#[cfg(unix)]
#[test]
fn a_move_whose_last_step_fails_puts_back_every_step_before_it() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture();
    let before = fixture.files();
    let records = fixture.records();
    let mut workspace = fixture.workspace.clone();
    let root = fixture.root().to_path_buf();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();

    let failed = migrate::names(&mut workspace);

    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let failed = failed.unwrap_err();
    assert!(matches!(failed, MigrateError::Names { .. }), "{failed:?}");
    assert_eq!(failed.code(), "migrate_rename_failed");
    assert!(failed.left_behind_names().is_empty(), "{failed:?}");
    let after = fixture.files();
    let changed: Vec<&PathBuf> = before
        .keys()
        .chain(after.keys())
        .filter(|route| before.get(*route) != after.get(*route))
        .collect();
    assert!(changed.is_empty(), "{changed:?}");
    assert_eq!(fixture.records(), records);
    let reopened = WeaverWorkspace::open(&root).unwrap();
    assert_eq!(reopened.names(), Names::Mindflayer);
    assert_eq!(fixture.standing(&reopened, "rust"), Standing::Current);
}

#[test]
fn a_workspace_under_mindweaver_s_names_plans_nothing() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();

    assert!(migrate::names_plan(&workspace).unwrap().is_empty());
    assert!(migrate::names(&mut workspace).unwrap().plan.is_empty());
    let unmade = WeaverWorkspace::unmade(dir.path().join("home")).unwrap();
    assert!(migrate::names_plan(&unmade).unwrap().is_empty());
}
