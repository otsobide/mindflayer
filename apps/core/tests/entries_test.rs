//! How an MCP server's entries stand in a project, an entry an agent: told
//! by what the ledger records writing, what each agent's config file holds
//! and what the library offers now, each built here as files in a
//! temporary folder, and said where the installed view lists them
//! (`status`, D-23).

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::config_entry::{ConfigFile, Format, Value};
use mindweaver_core::create::Start;
use mindweaver_core::install::{self, Standing};
use mindweaver_core::ledger::{EntryTarget, InstallRecord};
use mindweaver_core::lifecycle::{self, Record};
use mindweaver_core::mother::{McpMother, ProjectMother, SkillMother, WorkspaceMother};
use mindweaver_core::status::{EntryStatus, State, Status};
use mindweaver_core::value::{ArtifactName, Origin};
use mindweaver_core::{
    claude_mcp, codex_mcp, opencode_mcp, Agent, AgentHomes, Kind, Ledger, McpDefinition,
    MindProject, Reference, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A workspace linking `app`, a project every agent works in, with the
/// server `name` among its own, as `text` defines it.
struct World {
    _dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    ledger: Ledger,
    name: String,
}

impl World {
    fn new(text: impl FnOnce(&str) -> String) -> World {
        World::of(text, ProjectMother::create)
    }

    /// As [`World::new`], with `app` authoring servers of its own in `mcp`.
    fn authoring(text: impl FnOnce(&str) -> String) -> World {
        World::of(text, ProjectMother::with_mcp)
    }

    fn of(
        text: impl FnOnce(&str) -> String,
        make: fn(&Path, &[Agent], Share) -> MindProject,
    ) -> World {
        let (dir, mut workspace) = WorkspaceMother::empty();
        let project = make(&workspace.root().join("app"), &Agent::ALL, Share::Shared);
        workspace.link(&project).unwrap();
        let name = McpMother::name();
        let world = World {
            ledger: workspace.ledger().unwrap(),
            _dir: dir,
            workspace,
            project,
            name,
        };
        world.define(&text(&world.name));
        world
    }

    /// Where the library keeps the server's definition.
    fn definition(&self) -> PathBuf {
        self.workspace
            .own()
            .directory_for(Kind::Mcp)
            .unwrap()
            .join(format!("{}.toml", self.name))
    }

    /// Give the server `text` as its definition in the library.
    fn define(&self, text: &str) {
        let path = self.definition();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn root(&self) -> &Path {
        self.project.root()
    }

    /// Render the library's definition into every agent's file, as an
    /// install will, and record each entry with the fingerprint written:
    /// what #94's install leaves.
    fn installed(&self) {
        let definition =
            McpDefinition::parse(&fs::read_to_string(self.definition()).unwrap()).unwrap();
        let homes = AgentHomes::default();
        let claude = claude_mcp::render(self.root(), &definition).unwrap();
        let codex = codex_mcp::render(self.root(), &definition, &homes).unwrap();
        let opencode = opencode_mcp::render(self.root(), &definition, &homes).unwrap();
        let entry = |agent: Agent, fingerprint| EntryTarget {
            agent,
            file: agent.mcp_file(self.root()).unwrap().to_owned(),
            name: ArtifactName::from(self.name.as_str()),
            fingerprint,
        };
        let record = InstallRecord {
            kind: Kind::Mcp,
            name: ArtifactName::from(self.name.as_str()),
            source_id: None,
            origin: Some(Origin::from(install::WORKSPACE_ORIGIN)),
            content_hash: None,
            targets: Vec::new(),
            entries: vec![
                entry(Agent::Claude, claude.entry.fingerprint()),
                entry(Agent::Codex, codex.entry.fingerprint()),
                entry(Agent::OpenCode, opencode.entry.fingerprint()),
            ],
        };
        self.ledger.installed("app", &record).unwrap();
    }

    /// How the server stands for each agent.
    fn standings(&self) -> Vec<(Agent, Standing)> {
        install::entry_standings(&self.workspace, &self.ledger, &self.project, &self.name)
            .unwrap()
            .into_iter()
            .map(|(agent, standing)| (agent, standing.unwrap()))
            .collect()
    }

    /// How the installation stands as a whole, as an update would read it.
    fn installation(&self) -> Option<Standing> {
        install::standings(&self.workspace, &self.ledger, &self.project, Kind::Mcp)
            .unwrap()
            .into_iter()
            .find(|(name, _)| *name == self.name)
            .map(|(_, standing)| standing)
    }

    /// The report of the project, and the codes of what it could not read.
    fn status(&self) -> (Status, Vec<&'static str>) {
        let (status, failures) = Status::of_workspace(&self.workspace);
        (
            status,
            failures.iter().map(|failure| failure.code()).collect(),
        )
    }

    /// What the installed view lists of the project's entries.
    fn entries(&self) -> Vec<EntryStatus> {
        self.status().0.projects[0].entries.clone()
    }

    /// The server's own row of the report.
    fn row(&self) -> (State, Vec<Agent>) {
        let (status, _) = self.status();
        let row = status.projects[0]
            .find(Kind::Mcp, &self.name)
            .unwrap()
            .clone();
        (row.state, row.agents)
    }

    /// Change what the server's entry holds in `agent`'s file, as a person
    /// editing it by hand would.
    fn edit(&self, agent: Agent, key: &[&str], value: Value) {
        let route = agent.mcp_file(self.root()).unwrap();
        let format = match agent {
            Agent::Claude => Format::Json,
            Agent::Codex => Format::Toml,
            Agent::OpenCode => Format::Jsonc,
        };
        let file = ConfigFile::new(self.root(), route, format).unwrap();
        file.edit(key, &mindweaver_core::config_entry::Edit::Replace(value))
            .unwrap();
    }
}

fn every(standing: Standing) -> Vec<(Agent, Standing)> {
    Agent::ALL
        .into_iter()
        .map(|agent| (agent, standing))
        .collect()
}

/// Each agent's entry, as the installed view lists one it recorded.
fn listed(world: &World, state: State) -> Vec<EntryStatus> {
    Agent::ALL
        .into_iter()
        .map(|agent| EntryStatus {
            agent,
            file: agent.mcp_file(world.root()).unwrap().to_owned(),
            name: ArtifactName::from(world.name.as_str()),
            state,
            origin: Some(Origin::from(install::WORKSPACE_ORIGIN)),
        })
        .collect()
}

#[test]
fn a_server_nothing_records_and_no_file_holds_is_absent_for_every_agent() {
    let world = World::new(McpMother::plain_program);

    assert_eq!(world.standings(), every(Standing::Absent));
    assert_eq!(world.entries(), []);
    assert_eq!(world.installation(), None);
}

#[test]
fn an_entry_written_and_recorded_is_current_for_every_agent() {
    let world = World::new(McpMother::plain_program);

    world.installed();

    assert_eq!(world.standings(), every(Standing::Current));
    assert_eq!(world.installation(), Some(Standing::Current));
    assert_eq!(world.entries(), listed(&world, State::Current));
    assert_eq!(world.row(), (State::Current, Agent::ALL.to_vec()));
    assert_eq!(world.status().1, Vec::<&str>::new());
}

#[test]
fn an_entry_laid_out_again_by_somebody_is_still_current() {
    let world = World::new(McpMother::plain_program);
    world.installed();
    let path = world.root().join(claude_mcp::FILE);
    let held: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

    // Minified, as another tool might write it back.
    fs::write(&path, serde_json::to_string(&held).unwrap()).unwrap();

    assert_eq!(world.standings(), every(Standing::Current));
}

#[test]
fn a_definition_changed_since_makes_every_entry_it_changes_outdated() {
    let world = World::new(McpMother::plain_program);
    world.installed();

    world.define(&McpMother::plain_program(&world.name).replacen("\"-y\"", "\"--yes\"", 1));

    assert_eq!(world.standings(), every(Standing::Outdated));
    assert_eq!(world.installation(), Some(Standing::Outdated));
    assert_eq!(world.entries(), listed(&world, State::Outdated));
    assert_eq!(world.row(), (State::Outdated, Agent::ALL.to_vec()));
}

#[test]
fn a_change_one_agent_alone_is_given_leaves_the_others_current() {
    let world = World::new(McpMother::plain_program);
    world.installed();

    // Extras for Claude only: what Codex and OpenCode are given is the same.
    world.define(&McpMother::plain_program(&world.name).replacen(
        "timeout = 30000",
        "timeout = 60000",
        1,
    ));

    assert_eq!(
        world.standings(),
        [
            (Agent::Claude, Standing::Outdated),
            (Agent::Codex, Standing::Current),
            (Agent::OpenCode, Standing::Current),
        ]
    );
    assert_eq!(world.installation(), Some(Standing::Outdated));
}

#[test]
fn an_entry_edited_in_its_file_is_edited_for_that_agent_and_over_the_rest() {
    let world = World::new(McpMother::plain_program);
    world.installed();

    world.edit(
        Agent::Codex,
        &["mcp_servers", &world.name, "command"],
        Value::from("bunx"),
    );
    // And a definition changed beside it, which the edit is said over.
    world.define(&McpMother::plain_program(&world.name).replacen("\"-y\"", "\"--yes\"", 1));

    assert_eq!(
        world.standings(),
        [
            (Agent::Claude, Standing::Outdated),
            (Agent::Codex, Standing::Edited),
            (Agent::OpenCode, Standing::Outdated),
        ]
    );
    assert_eq!(world.installation(), Some(Standing::Edited));
    assert_eq!(world.row().0, State::Edited);
}

#[test]
fn an_entry_taken_out_of_its_file_or_with_it_is_missing() {
    let world = World::new(McpMother::plain_program);
    world.installed();

    opencode_mcp::remove(world.root(), &world.name, false).unwrap();
    fs::remove_file(world.root().join(claude_mcp::FILE)).unwrap();

    assert_eq!(
        world.standings(),
        [
            (Agent::Claude, Standing::Missing),
            (Agent::Codex, Standing::Current),
            (Agent::OpenCode, Standing::Missing),
        ]
    );
    assert_eq!(world.installation(), Some(Standing::Missing));
    // Missing, and reaching Codex alone.
    assert_eq!(world.row(), (State::Missing, vec![Agent::Codex]));
}

#[test]
fn an_entry_whose_definition_is_offered_no_more_is_orphaned_over_missing() {
    let world = World::new(McpMother::plain_program);
    world.installed();
    fs::remove_file(world.root().join(claude_mcp::FILE)).unwrap();

    fs::remove_file(world.definition()).unwrap();

    assert_eq!(world.standings(), every(Standing::Orphaned));
    assert_eq!(world.installation(), Some(Standing::Orphaned));
    // Reaching the agents whose entry is still there, Claude's gone.
    assert_eq!(
        world.row(),
        (State::Orphaned, vec![Agent::Codex, Agent::OpenCode])
    );
}

#[test]
fn an_entry_of_the_name_nothing_records_is_foreign_and_listed_as_foreign() {
    let world = World::new(McpMother::plain_program);
    let definition = McpDefinition::parse(&McpMother::plain_program(&world.name)).unwrap();
    claude_mcp::render(world.root(), &definition).unwrap();
    // And one of a name the library has no server of, beside it.
    let theirs = format!("{}x", world.name);
    codex_mcp::render(
        world.root(),
        &McpDefinition::parse(&McpMother::plain_program(&theirs)).unwrap(),
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(
        world.standings(),
        [
            (Agent::Claude, Standing::Foreign),
            (Agent::Codex, Standing::Absent),
            (Agent::OpenCode, Standing::Absent),
        ]
    );
    let foreign = |agent: Agent, name: &str| EntryStatus {
        agent,
        file: agent.mcp_file(world.root()).unwrap().to_owned(),
        name: ArtifactName::from(name),
        state: State::Foreign,
        origin: None,
    };
    assert_eq!(
        world.entries(),
        [
            foreign(Agent::Claude, &world.name),
            foreign(Agent::Codex, &theirs)
        ]
    );
    // Nothing is installed, so no row of the server's own.
    assert!(world.status().0.projects[0]
        .find(Kind::Mcp, &world.name)
        .is_none());
}

#[test]
fn an_entry_beside_somebody_elses_is_told_apart_from_theirs_by_its_name() {
    let world = World::new(McpMother::plain_program);
    let theirs = format!("{}x", world.name);
    let definition = McpDefinition::parse(&McpMother::plain_program(&theirs)).unwrap();
    opencode_mcp::render(world.root(), &definition, &AgentHomes::default()).unwrap();

    world.installed();

    let mut expected = listed(&world, State::Current);
    expected.push(EntryStatus {
        agent: Agent::OpenCode,
        file: String::from(opencode_mcp::FILE),
        name: ArtifactName::from(theirs.as_str()),
        state: State::Foreign,
        origin: None,
    });
    assert_eq!(world.entries(), expected);
}

#[test]
fn a_file_that_cannot_be_read_holds_no_entry_and_says_why() {
    let world = World::new(McpMother::plain_program);
    world.installed();
    let path = world.root().join(claude_mcp::FILE);
    let text = fs::read_to_string(&path).unwrap();

    // A comment, which Claude loads none of the file over.
    fs::write(&path, format!("// ours\n{text}")).unwrap();

    let told =
        install::entry_standings(&world.workspace, &world.ledger, &world.project, &world.name)
            .unwrap();
    assert!(
        matches!(&told[0], (Agent::Claude, Err(unread)) if unread.code() == "mcp_json_not_strict"),
        "{told:?}"
    );
    assert_eq!(world.installation(), Some(Standing::Missing));
    let (status, codes) = world.status();
    assert_eq!(codes, ["mcp_json_not_strict"]);
    assert_eq!(status.projects[0].entries[0].state, State::Missing);
}

#[cfg(unix)]
#[test]
fn a_linked_config_file_nothing_was_written_into_fails_nothing_and_lists_nothing() {
    let world = World::new(McpMother::plain_program);
    // As a dotfiles setup links them, which the agents follow and a
    // renderer, built to write, refuses.
    let dotfiles = world.workspace.root().join("dotfiles");
    fs::create_dir_all(dotfiles.join("codex")).unwrap();
    let theirs = McpDefinition::parse(&McpMother::plain_program("theirs")).unwrap();
    claude_mcp::render(&dotfiles, &theirs).unwrap();
    std::os::unix::fs::symlink(
        dotfiles.join(claude_mcp::FILE),
        world.root().join(claude_mcp::FILE),
    )
    .unwrap();
    std::os::unix::fs::symlink(dotfiles.join("codex"), world.root().join(".codex")).unwrap();

    let (status, codes) = world.status();

    assert_eq!(codes, Vec::<&str>::new());
    assert_eq!(status.projects[0].entries, []);
    let read = mindweaver_core::view::Project::read(&world.project, Some(&world.workspace));
    assert_eq!(read.health, mindweaver_core::view::Health::Ok);
    assert!(read.failures.is_empty(), "{:?}", read.failures);
}

#[test]
fn an_opencode_entry_kept_in_v1s_shape_is_held_to_what_that_shape_is_given() {
    let world = World::new(McpMother::plain_program);
    // A file that keeps its servers in V1's shape already, so the entry is
    // written in it.
    fs::write(
        world.root().join(opencode_mcp::FILE),
        "{\"mcp\": {\"theirs\": {\"type\": \"local\", \"command\": [\"x\"], \"enabled\": true}}}\n",
    )
    .unwrap();

    world.installed();

    assert_eq!(world.standings(), every(Standing::Current));
    let held = fs::read_to_string(world.root().join(opencode_mcp::FILE)).unwrap();
    assert!(!held.contains("\"servers\""), "{held}");
    let entries = world.entries();
    assert!(
        entries
            .iter()
            .any(|entry| entry.name == "theirs" && entry.state == State::Foreign),
        "{entries:?}"
    );
}

#[test]
fn the_snapshot_lists_a_projects_entries_as_status_does() {
    let world = World::new(McpMother::plain_program);
    world.installed();
    world.edit(
        Agent::Claude,
        &["mcpServers", &world.name, "command"],
        Value::from("bunx"),
    );

    let read = mindweaver_core::view::Project::read(&world.project, Some(&world.workspace));

    assert_eq!(read.entries, world.entries());
    assert_eq!(read.entries[0].state, State::Edited);
    let held = read
        .artifacts
        .iter()
        .find(|held| held.kind == Kind::Mcp)
        .unwrap();
    assert_eq!(held.state, Some(State::Edited));
    // Where it is held is each file it was written into, in path order.
    assert_eq!(
        held.targets,
        [codex_mcp::FILE, claude_mcp::FILE, opencode_mcp::FILE]
    );
}

#[test]
fn the_projects_own_server_of_the_name_added_renamed_and_removed_leaves_what_was_installed_recorded(
) {
    let world = World::authoring(McpMother::plain_program);
    world.installed();
    let recorded = || {
        world
            .ledger
            .installation("app", Kind::Mcp, &world.name)
            .unwrap()
    };
    let before = recorded();
    let ours = |name: &str| {
        lifecycle::find_to_change(
            Some(&world.workspace),
            &world.project,
            &Reference::parse(&format!("mcp:{name}")),
        )
        .unwrap()
    };

    // Each in the project's own `mcp`, which holds none of the entries the
    // install wrote into its agents' config files.
    let added = lifecycle::add(
        Some(&world.workspace),
        &world.project,
        Kind::Mcp,
        &world.name,
        &SkillMother::description(),
        Start::Server(&McpMother::given()),
    )
    .unwrap();
    let renamed = lifecycle::rename(
        Some(&world.workspace),
        &world.project,
        &ours(&world.name),
        &format!("{}x", world.name),
    )
    .unwrap();
    let removed =
        lifecycle::remove(Some(&world.workspace), &world.project, &renamed.target).unwrap();

    assert_eq!(added.stale.unwrap(), Record::Untouched);
    assert_eq!(renamed.copy.unwrap(), Record::Untouched);
    assert_eq!(removed.copy.unwrap(), Record::Untouched);
    assert_eq!(recorded(), before);
    assert_eq!(world.standings(), every(Standing::Current));
    assert_eq!(world.entries(), listed(&world, State::Current));
}
