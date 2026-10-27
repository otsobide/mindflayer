//! Installing rules: offered by the route they are filed under, rendered
//! into a project marked by its settings, a section of the block in its
//! `AGENTS.md` and Claude's files as Claude reads them, recorded in the
//! ledger; copied into a project a `mind.toml` marks; and never over what
//! somebody wrote.

mod common;

use std::fs;
use std::path::PathBuf;

use mindflayer_core::agents_md::{self, Section};
use mindflayer_core::claude_md::{self, Fallback, Reach};
use mindflayer_core::claude_rules::{self, Warning as FileWarning};
use mindflayer_core::install::rules::{self, Warning, Withheld};
use mindflayer_core::install::{
    self, InstallError, Installation, Installed, Removed, Standing, WORKSPACE_ORIGIN,
};
use mindflayer_core::ledger::{InstallRecord, Target};
use mindflayer_core::{
    hash, Agent, AgentHomes, Directories, FlayerWorkspace, Kind, Ledger, MindProject, Share,
};
use tempfile::TempDir;

/// A rule about every file.
const NO_FORCE_PUSH: &str = "Never force-push a branch somebody else may have pulled.\n";
/// Another, with front matter of its own.
const TONE: &str = "---\ndescription: How a review reads\n---\n\nSay what a change does before what is wrong with it.\n";

/// The own rule of `workspace` at `route`, holding `source`.
fn own_rule(workspace: &FlayerWorkspace, route: &str, source: &str) -> PathBuf {
    let file = workspace
        .own()
        .directory_for(Kind::Rule)
        .unwrap()
        .join(format!("{route}.md"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, source).unwrap();
    file
}

/// A workspace linking a project a `mind.toml` marks, which keeps its rules
/// in `rules`.
fn older() -> (TempDir, FlayerWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("app")).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, project, ledger)
}

fn copy_of(project: &MindProject, route: &str) -> PathBuf {
    let mut path = project.directory_for(Kind::Rule).unwrap();
    path.extend(route.split('/'));
    path.with_extension("md")
}

fn try_put(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Installed, InstallError> {
    let candidate = install::offered(workspace, ledger, project, Kind::Rule, route, None)?;
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .map(|done| done.installed)
}

fn put(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Installed {
    try_put(workspace, ledger, project, route).unwrap()
}

fn take(
    workspace: &FlayerWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Removed, InstallError> {
    install::uninstall(
        workspace,
        ledger,
        project,
        Kind::Rule,
        route,
        &AgentHomes::default(),
    )
}

#[test]
fn a_project_a_mind_toml_marks_gets_a_copy_of_the_rule_filed_by_its_route() {
    let (_dir, workspace, project, ledger) = older();
    let file = own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");

    let added = put(&workspace, &ledger, &project, "git/no-force-push");

    assert_eq!(
        added,
        Installed::Added {
            name: String::from("git/no-force-push"),
            paths: vec![copy.clone()],
            unhashed: None,
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), NO_FORCE_PUSH);
    assert!(!project.root().join("AGENTS.md").exists());
    let held = hash::bytes(NO_FORCE_PUSH.as_bytes());
    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap(),
        [InstallRecord {
            kind: Kind::Rule,
            name: String::from("git/no-force-push"),
            source_id: None,
            origin: Some(String::from(WORKSPACE_ORIGIN)),
            content_hash: Some(held.clone()),
            targets: vec![Target {
                agent: None,
                path: String::from("rules/git/no-force-push.md"),
                content_hash: Some(held),
            }],
        }]
    );
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Unchanged { .. }
    ));
    fs::write(&file, "Never force-push to main.\n").unwrap();
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Updated { .. }
    ));
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        "Never force-push to main.\n"
    );

    let removed = take(&workspace, &ledger, &project, "git/no-force-push").unwrap();

    assert!(
        matches!(&removed, Removed::Removed { paths, still_excluded: None, .. } if *paths == [copy.clone()]),
        "{removed:?}"
    );
    assert!(!copy.exists());
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

#[test]
fn a_file_at_a_rules_place_nothing_recorded_is_somebodys_in_a_project_a_mind_toml_marks() {
    let (_dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::write(&copy, "Our own rule.\n").unwrap();

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Rule).unwrap();
    assert_eq!(surveyed.len(), 1);
    assert_eq!(surveyed[0].standing, Standing::Foreign);

    assert_eq!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Foreign {
            name: String::from("git/no-force-push"),
            path: copy.clone(),
        }
    );
    assert_eq!(
        take(&workspace, &ledger, &project, "git/no-force-push").unwrap(),
        Removed::Foreign {
            name: String::from("git/no-force-push"),
            path: copy.clone(),
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), "Our own rule.\n");
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

#[test]
fn a_rule_from_a_loaded_workspace_records_where_it_came_from() {
    let (dir, mut workspace, project, ledger) = older();
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", TONE);
    workspace.load(&team).unwrap();

    put(&workspace, &ledger, &project, "review/tone");

    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap()[0]
            .origin
            .as_deref(),
        Some("load:team")
    );
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        TONE
    );
}

#[test]
fn two_places_offering_one_route_are_told_apart_with_from() {
    let (dir, mut workspace, project, ledger) = older();
    own_rule(&workspace, "review/tone", TONE);
    let (team, _) = FlayerWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", "Be kind.\n");
    workspace.load(&team).unwrap();

    let twice = try_put(&workspace, &ledger, &project, "review/tone").unwrap_err();

    assert!(
        matches!(&twice, InstallError::OfferedTwice { origins, .. }
            if *origins == [String::from(WORKSPACE_ORIGIN), String::from("load:team")]),
        "{twice:?}"
    );
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "review/tone",
        Some("load:team"),
    )
    .unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        "Be kind.\n"
    );
}

#[cfg(unix)]
#[test]
fn a_rule_linked_out_of_the_workspace_that_offers_it_is_not_installed() {
    let (_dir, workspace, project, ledger) = older();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "token=hunter2\n").unwrap();
    let file = own_rule(&workspace, "leak", "placeholder\n");
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&secret, &file).unwrap();

    let refused = try_put(&workspace, &ledger, &project, "leak").unwrap_err();

    assert!(
        matches!(refused, InstallError::Outside { .. }),
        "{refused:?}"
    );
    assert!(!copy_of(&project, "leak").exists());
}

#[test]
fn a_route_that_is_no_name_is_neither_installed_nor_removed() {
    let (_dir, workspace, project, ledger) = older();

    let refused = take(&workspace, &ledger, &project, "../escape").unwrap_err();

    assert!(
        matches!(refused, InstallError::UnsafeRoute { .. }),
        "{refused:?}"
    );
    assert!(install::is_installable_name(
        Kind::Rule,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(
        Kind::Skill,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(Kind::Rule, "Git Notes"));
}

/// What a team's `AGENTS.md` might hold before Mindflayer writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A rule about Rust files only.
const SCOPED: &str =
    "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n";

/// A workspace in a git work tree, linking one project below it whose
/// settings name `agents` and share as `share`, with rules of its own to
/// install.
struct Fixture {
    dir: TempDir,
    workspace: FlayerWorkspace,
    project: MindProject,
    ledger: Ledger,
}

fn fixture(share: Share, agents: &[Agent]) -> Fixture {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let (mut workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, agents, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    Fixture {
        dir,
        workspace,
        project,
        ledger,
    }
}

impl Fixture {
    /// The workspace's own rule at `route`, holding `source`.
    fn rule(&self, route: &str, source: &str) -> PathBuf {
        own_rule(&self.workspace, route, source)
    }

    fn install(&self, route: &str) -> Installation {
        self.try_install(route).unwrap()
    }

    fn try_install(&self, route: &str) -> Result<Installation, InstallError> {
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Rule,
            route,
            None,
        )?;
        install::install(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
        )
    }

    fn uninstall(&self, route: &str) -> Removed {
        install::uninstall(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Rule,
            route,
            &AgentHomes::default(),
        )
        .unwrap()
    }

    fn render(&self) -> rules::Rendered {
        rules::render(
            &self.workspace,
            &self.ledger,
            &self.project,
            &AgentHomes::default(),
        )
        .unwrap()
        .unwrap()
    }

    fn records(&self) -> Vec<InstallRecord> {
        self.ledger.records("app", Kind::Rule).unwrap()
    }

    fn agents_md(&self) -> PathBuf {
        self.project.root().join("AGENTS.md")
    }

    fn text(&self) -> String {
        fs::read_to_string(self.agents_md()).unwrap()
    }

    fn claude_file(&self, route: &str) -> PathBuf {
        self.project.root().join(claude_rules::file_for(route))
    }

    /// Whether git, reading the exclude file, holds `route` in the project
    /// out of the work tree.
    fn ignored(&self, route: &str) -> bool {
        let repository = gix::open_opts(self.dir.path(), gix::open::Options::isolated()).unwrap();
        let index = repository.index_or_empty().unwrap();
        let mut excludes = repository
            .excludes(&index, None, Default::default())
            .unwrap();
        excludes
            .at_path(
                format!("app/{route}").as_str(),
                Some(gix::index::entry::Mode::FILE),
            )
            .unwrap()
            .is_excluded()
    }
}

/// The hash a section of `body` for `route` is recorded with.
fn section_hash(route: &str, body: &str) -> String {
    Section::new(route, body).unwrap().hash()
}

/// A target of the section of a rule, by its hash.
fn in_block(hash: String) -> Target {
    Target {
        agent: None,
        path: String::from("AGENTS.md"),
        content_hash: Some(hash),
    }
}

/// A target of a rule's file for Claude, holding `source`.
fn for_claude(route: &str, source: &str) -> Target {
    Target {
        agent: Some(String::from("claude")),
        path: format!(".claude/rules/mindflayer/{route}.md"),
        content_hash: Some(hash::bytes(source.as_bytes())),
    }
}

fn record(route: &str, source: &str, targets: Vec<Target>) -> InstallRecord {
    InstallRecord {
        kind: Kind::Rule,
        name: route.to_owned(),
        source_id: None,
        origin: Some(String::from(WORKSPACE_ORIGIN)),
        content_hash: Some(hash::bytes(source.as_bytes())),
        targets,
    }
}

#[test]
fn two_rules_rendered_again_and_one_taken_out_leave_the_block_and_the_ledger_alike() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("review/tone", TONE);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let first = fixture.install("review/tone");
    let second = fixture.install("git/no-force-push");

    assert!(
        matches!(&first.installed, Installed::Added { name, .. } if name == "review/tone"),
        "{first:?}"
    );
    assert!(
        matches!(&second.installed, Installed::Added { paths, .. } if *paths == [fixture.agents_md()]),
        "{second:?}"
    );
    let both = format!(
        "{TEAM}\n<!-- mindflayer:begin -->\n\
         <!-- mindflayer:rule git/no-force-push -->\n{NO_FORCE_PUSH}\n\
         <!-- mindflayer:rule review/tone -->\n\
         Say what a change does before what is wrong with it.\n\
         <!-- mindflayer:end -->\n"
    );
    assert_eq!(fixture.text(), both);
    // Claude reads the block itself, so no rule about every file is a file
    // of its own as well.
    let rendered = second.rendered.unwrap();
    assert_eq!(rendered.import.reach, Reach::Native);
    assert!(rendered.files.files.is_empty());
    assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
    let recorded = vec![
        record(
            "git/no-force-push",
            NO_FORCE_PUSH,
            vec![in_block(section_hash("git/no-force-push", NO_FORCE_PUSH))],
        ),
        record(
            "review/tone",
            TONE,
            vec![in_block(section_hash(
                "review/tone",
                "Say what a change does before what is wrong with it.",
            ))],
        ),
    ];
    assert_eq!(fixture.records(), recorded);

    let again = fixture.render();

    assert_eq!(again.block.unwrap().written, agents_md::Written::Unchanged);
    assert_eq!(fixture.text(), both);
    assert_eq!(fixture.records(), recorded);

    let removed = fixture.uninstall("review/tone");

    assert!(
        matches!(&removed, Removed::Removed { paths, .. } if *paths == [fixture.agents_md()]),
        "{removed:?}"
    );
    assert_eq!(
        fixture.text(),
        format!(
            "{TEAM}\n<!-- mindflayer:begin -->\n\
             <!-- mindflayer:rule git/no-force-push -->\n{NO_FORCE_PUSH}\
             <!-- mindflayer:end -->\n"
        )
    );
    assert_eq!(fixture.records(), recorded[..1]);
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
}

#[test]
fn installing_a_rule_again_that_is_as_it_was_changes_nothing_and_says_so() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let file = fixture.rule("review/tone", TONE);
    fixture.install("review/tone");

    let again = fixture.install("review/tone");
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );

    fs::write(&file, "Say what a change does, kindly.\n").unwrap();
    let changed = fixture.install("review/tone");

    assert!(
        matches!(changed.installed, Installed::Updated { .. }),
        "{changed:?}"
    );
    assert!(fixture.text().contains("Say what a change does, kindly.\n"));
}

#[test]
fn a_render_gives_each_rule_as_it_is_where_it_was_installed_from() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let file = fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    fs::write(&file, "Never force-push to main.\n").unwrap();

    let rendered = fixture.render();

    assert_eq!(rendered.block.unwrap().written, agents_md::Written::Changed);
    assert!(fixture.text().contains("Never force-push to main.\n"));
    assert_eq!(
        fixture.records()[0].targets,
        [in_block(section_hash(
            "git/no-force-push",
            "Never force-push to main.\n"
        ))]
    );
}

#[test]
fn an_agents_md_the_user_wrote_stays_once_the_last_rule_is_taken_out() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");

    fixture.uninstall("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    assert!(fixture.records().is_empty());
}

#[test]
fn an_agents_md_mindflayer_made_goes_with_the_last_rule_and_is_kept_out_of_git_meanwhile() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert_eq!(
        done.rendered.unwrap().block.unwrap().written,
        agents_md::Written::Created
    );
    assert!(fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(fixture.ignored("AGENTS.md"));

    fixture.uninstall("git/no-force-push");

    assert!(!fixture.agents_md().exists());
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn an_agents_md_mindflayer_made_and_somebody_wrote_in_stays_and_is_theirs() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    let written = fixture.text() + "\nA line of mine.\n";
    fs::write(fixture.agents_md(), &written).unwrap();

    fixture.uninstall("git/no-force-push");

    assert_eq!(fixture.text(), "\nA line of mine.\n");
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    // Theirs to commit, if they like.
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn an_agents_md_that_was_there_in_a_local_project_is_not_kept_out_of_git() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    fixture.install("git/no-force-push");

    assert!(fixture.text().starts_with(TEAM));
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn a_tracked_agents_md_in_a_local_project_is_left_alone_and_claude_gets_the_fallback() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    let rendered = done.rendered.unwrap();
    assert_eq!(rendered.block, None);
    assert_eq!(rendered.import.reach, Reach::Fallback(Fallback::NoBlock));
    assert_eq!(
        claude_md::reach(&fixture.project, &AgentHomes::default(), false).unwrap(),
        Reach::Fallback(Fallback::NoBlock)
    );
    assert_eq!(
        fs::read_to_string(fixture.claude_file("git/no-force-push")).unwrap(),
        NO_FORCE_PUSH
    );
    assert!(fixture.ignored(".claude/rules/mindflayer/git/no-force-push.md"));
    assert_eq!(
        rendered.warnings,
        [Warning::Withheld {
            file: fixture.agents_md(),
            why: Withheld::Tracked,
            missed: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
    assert_eq!(
        rendered.warnings[0].to_string(),
        format!(
            "{}: git tracks it, and the project is local without edit_tracked, so no rules \
             are written there, and codex and opencode read none of them",
            fixture.agents_md().display()
        )
    );
    assert_eq!(
        fixture.records(),
        [record(
            "git/no-force-push",
            NO_FORCE_PUSH,
            vec![for_claude("git/no-force-push", NO_FORCE_PUSH)]
        )]
    );
}

#[test]
fn a_tracked_agents_md_in_a_shared_project_takes_the_block() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert!(fixture
        .text()
        .contains("<!-- mindflayer:rule git/no-force-push -->"));
    assert!(done.rendered.unwrap().warnings.is_empty());
}

#[test]
fn a_block_left_in_a_file_that_can_no_longer_be_edited_is_said() {
    let mut fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    common::indexed(fixture.dir.path());
    // Turned local by hand, as somebody might edit the settings.
    let settings = fixture.project.mind_dir().join("settings.json");
    let text = fs::read_to_string(&settings).unwrap();
    fs::write(&settings, text.replace("\"shared\"", "\"local\"")).unwrap();
    fixture.project = MindProject::open(fixture.project.root()).unwrap();
    let before = fixture.text();

    let rendered = fixture.render();

    assert_eq!(fixture.text(), before);
    assert_eq!(
        rendered.warnings[0],
        Warning::Left {
            file: fixture.agents_md(),
            why: Withheld::Tracked,
        }
    );
    assert_eq!(rendered.import.reach, Reach::Fallback(Fallback::NoBlock));
    assert!(fixture.claude_file("git/no-force-push").is_file());
}

#[test]
fn a_rule_about_some_files_is_a_file_for_claude_alone_and_says_so() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);

    let done = fixture.install("rust/errors");

    // No section, so no block, and no file made to hold one.
    assert!(!fixture.agents_md().exists());
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert_eq!(
        fs::read_to_string(fixture.claude_file("rust/errors")).unwrap(),
        SCOPED
    );
    let rendered = done.rendered.unwrap();
    assert_eq!(
        rendered.files.warnings,
        [FileWarning::ClaudeOnly {
            route: String::from("rust/errors"),
            missed: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
    assert_eq!(
        fixture.records(),
        [record(
            "rust/errors",
            SCOPED,
            vec![for_claude("rust/errors", SCOPED)]
        )]
    );

    fixture.uninstall("rust/errors");

    assert!(!fixture.claude_file("rust/errors").exists());
    assert!(!fixture
        .project
        .root()
        .join(".claude/rules/mindflayer")
        .exists());
    assert!(fixture.records().is_empty());
}

#[test]
fn claude_given_the_rules_by_its_own_files_has_each_once() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    // A `CLAUDE.md` only above the project: Claude reads no `AGENTS.md`
    // there, and the import has nowhere of the project's to go.
    fs::write(fixture.dir.path().join("CLAUDE.md"), "# Above\n").unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    let rendered = done.rendered.unwrap();
    assert!(
        matches!(rendered.import.reach, Reach::Fallback(Fallback::Above(_))),
        "{:?}",
        rendered.import.reach
    );
    assert!(fixture
        .text()
        .contains("<!-- mindflayer:rule git/no-force-push -->"));
    assert_eq!(
        fs::read_to_string(fixture.claude_file("git/no-force-push")).unwrap(),
        NO_FORCE_PUSH
    );
    assert_eq!(
        fixture.records()[0].targets,
        [
            for_claude("git/no-force-push", NO_FORCE_PUSH),
            in_block(section_hash("git/no-force-push", NO_FORCE_PUSH)),
        ]
    );
}

#[test]
fn a_project_that_names_claude_alone_still_gets_the_block_claude_reads() {
    let fixture = fixture(Share::Shared, &[Agent::Claude]);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert!(fixture.text().contains(NO_FORCE_PUSH));
    assert!(done.rendered.unwrap().warnings.is_empty());
}

#[test]
fn a_rule_no_agent_of_the_project_reads_is_installed_and_said() {
    let fixture = fixture(Share::Shared, &[Agent::Codex]);
    fixture.rule("rust/errors", SCOPED);

    let done = fixture.install("rust/errors");

    // Codex reads no `.claude/rules`, and the project does not name Claude.
    assert!(!fixture.claude_file("rust/errors").exists());
    assert_eq!(
        done.rendered.unwrap().warnings,
        [Warning::Unreached {
            route: String::from("rust/errors")
        }]
    );
    assert_eq!(fixture.records()[0].targets, []);
}

#[test]
fn a_section_this_workspace_did_not_write_is_somebodys_and_refuses_the_render() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("review/tone", TONE);
    let theirs = format!(
        "{TEAM}\n<!-- mindflayer:begin -->\n<!-- mindflayer:rule review/tone -->\nTheirs.\n<!-- mindflayer:end -->\n"
    );
    fs::write(fixture.agents_md(), &theirs).unwrap();

    let surveyed = install::survey(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        Kind::Rule,
    )
    .unwrap();
    let tone = surveyed
        .iter()
        .find(|candidate| candidate.name() == "review/tone")
        .unwrap();
    assert_eq!(tone.standing, Standing::Foreign);

    let refused = fixture.install("review/tone");
    assert_eq!(
        refused.installed,
        Installed::Foreign {
            name: String::from("review/tone"),
            path: fixture.agents_md(),
        }
    );
    let other = fixture.try_install("git/no-force-push").unwrap_err();
    assert!(
        matches!(&other, InstallError::NotRecorded { route, .. } if route == "review/tone"),
        "{other:?}"
    );
    assert_eq!(fixture.text(), theirs);
    assert!(fixture.records().is_empty());
}

#[test]
fn a_file_for_claude_nothing_recorded_is_somebodys_and_refuses_the_install() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);
    let file = fixture.claude_file("rust/errors");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, SCOPED).unwrap();

    let refused = fixture.install("rust/errors");

    assert_eq!(
        refused.installed,
        Installed::Foreign {
            name: String::from("rust/errors"),
            path: file.clone(),
        }
    );
    assert_eq!(
        fixture.uninstall("rust/errors"),
        Removed::Foreign {
            name: String::from("rust/errors"),
            path: file,
        }
    );
    assert!(fixture.records().is_empty());
}

#[test]
fn a_rule_no_longer_offered_refuses_the_render_until_it_is_taken_out() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let tone = fixture.rule("review/tone", TONE);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("review/tone");
    fs::remove_file(tone).unwrap();
    let before = fixture.text();

    let refused = fixture.try_install("git/no-force-push").unwrap_err();

    assert!(
        matches!(&refused, InstallError::NoLongerOffered { name, origin }
            if name == "review/tone" && origin.as_deref() == Some(WORKSPACE_ORIGIN)),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), before);

    fixture.uninstall("review/tone");
    fixture.install("git/no-force-push");

    assert!(!fixture.text().contains("review/tone"));
    assert_eq!(fixture.records().len(), 1);
}

#[test]
fn a_block_that_cannot_be_read_refuses_the_install_with_nothing_recorded() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    let broken = "<!-- mindflayer:begin -->\nLeft open.\n";
    fs::write(fixture.agents_md(), broken).unwrap();

    let refused = fixture.try_install("git/no-force-push").unwrap_err();

    assert!(matches!(refused, InstallError::AgentsMd(_)), "{refused:?}");
    assert_eq!(fixture.text(), broken);
    assert!(fixture.records().is_empty());
}

#[test]
fn rendering_a_project_a_mind_toml_marks_renders_nothing() {
    let (_dir, workspace, project, ledger) = older();

    let rendered = rules::render(&workspace, &ledger, &project, &AgentHomes::default()).unwrap();

    assert_eq!(rendered, None);
    assert!(!project.root().join("AGENTS.md").exists());
}
