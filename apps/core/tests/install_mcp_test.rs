//! An MCP server installed into a project, an entry in the config file of
//! each agent it names (D-10), brought up to date and taken back out: what
//! each file holds afterwards, what the ledger records of it, what is
//! refused and what is left as it was, each file built here in a temporary
//! folder, none in a real agent's home.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::install::mcp::Variable;
use mindweaver_core::install::{
    self, Edits, InstallError, Installation, Installed, Removed, Standing,
};
use mindweaver_core::ledger::InstallRecord;
use mindweaver_core::lifecycle::{self, LifecycleError};
use mindweaver_core::mother::McpMother;
use mindweaver_core::update::{self, Step};
use mindweaver_core::{
    claude_mcp, codex_mcp, exclude, opencode_mcp, Agent, AgentHomes, Directories, Kind, Ledger,
    McpDefinition, MindProject, Reference, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A team's own `.mcp.json`, holding a server of theirs.
const TEAMS: &str = "{\n  \"mcpServers\": {\n    \"docs\": {\n      \"type\": \"stdio\",\n      \
                     \"command\": \"docs-mcp\"\n    }\n  }\n}\n";

/// A team's own `.codex/config.toml`: a setting, a comment and a server of
/// theirs.
const CODEX_TEAMS: &str = "# The team's Codex settings.\nmodel = \"o4\"\n\n\
                           [mcp_servers.docs]\ncommand = \"docs-mcp\" # theirs\n";

/// A team's own `opencode.json`, as OpenCode reads it, JSONC: a comment,
/// a trailing comma and a server of theirs.
const OPENCODE_TEAMS: &str = "// The team's OpenCode settings.\n{\n  \"theme\": \"dark\",\n  \
                              \"mcp\": {\n    \"servers\": {\n      \"docs\": {\n        \
                              \"type\": \"local\",\n        \"command\": [\"docs-mcp\"],\n      \
                              },\n    },\n  },\n}\n";

/// A workspace at the root of a git work tree, linking `app`, a project
/// below it whose settings name `agents` and share as `share`, with the
/// server `name` among the workspace's own.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    ledger: Ledger,
    name: String,
}

fn fixture(share: Share, agents: &[Agent], text: fn(&str) -> String) -> Fixture {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, agents, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    let fixture = Fixture {
        ledger: workspace.ledger().unwrap(),
        dir,
        workspace,
        project,
        name: McpMother::name(),
    };
    fixture.define(&text(&fixture.name));
    fixture
}

impl Fixture {
    fn root(&self) -> &Path {
        self.project.root()
    }

    /// Give the server `text` as its definition in the library.
    fn define(&self, text: &str) {
        let folder = self.workspace.own().directory_for(Kind::Mcp).unwrap();
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{}.toml", self.name)), text).unwrap();
    }

    /// The library's definition of the server.
    fn definition(&self) -> McpDefinition {
        let folder = self.workspace.own().directory_for(Kind::Mcp).unwrap();
        McpDefinition::parse(
            &fs::read_to_string(folder.join(format!("{}.toml", self.name))).unwrap(),
        )
        .unwrap()
    }

    fn try_install(&self, edits: Edits) -> Result<Installation, InstallError> {
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Mcp,
            &self.name,
            None,
        )?;
        install::install_with(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
            edits,
        )
    }

    fn install(&self) -> Installation {
        self.try_install(Edits::Kept).unwrap()
    }

    /// What installing the server would give each agent, nothing written.
    fn preview(&self) -> Installation {
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Mcp,
            &self.name,
            None,
        )
        .unwrap();
        install::mcp::preview(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
            Edits::Kept,
        )
        .unwrap()
    }

    fn try_uninstall(&self, edits: Edits) -> Result<Removed, InstallError> {
        install::uninstall_with(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Mcp,
            &self.name,
            &AgentHomes::default(),
            edits,
        )
    }

    fn uninstall(&self, edits: Edits) -> Removed {
        self.try_uninstall(edits).unwrap()
    }

    /// Update the project, the server's installation and nothing else of it,
    /// and say what was planned for it and what came of writing it.
    fn update(&self, edits: Edits) -> update::Update {
        let applied = update::apply(
            &self.workspace,
            std::slice::from_ref(&self.project),
            &[],
            edits,
            &AgentHomes::default(),
        )
        .unwrap();
        let mut planned = applied.plan.projects;
        assert_eq!(planned.len(), 1, "{planned:?}");
        let mut installations = planned.remove(0).installations;
        assert_eq!(installations.len(), 1, "{installations:?}");
        installations.remove(0)
    }

    /// How the installation stands as a whole, as an update reads it.
    fn installation(&self) -> Option<Standing> {
        install::standings(&self.workspace, &self.ledger, &self.project, Kind::Mcp)
            .unwrap()
            .into_iter()
            .find(|(name, _)| *name == self.name)
            .map(|(_, standing)| standing)
    }

    /// Every config file of the project's agents, by agent, and what each
    /// holds, if it is there.
    fn files(&self) -> Vec<(Agent, Option<String>)> {
        Agent::ALL
            .into_iter()
            .map(|agent| (agent, self.read(self.route(agent))))
            .collect()
    }

    fn record(&self) -> Option<InstallRecord> {
        self.ledger
            .installation("app", Kind::Mcp, &self.name)
            .unwrap()
    }

    /// The route of `agent`'s config file in the project.
    fn route(&self, agent: Agent) -> &'static str {
        agent.mcp_file(self.root()).unwrap()
    }

    fn file(&self, agent: Agent) -> PathBuf {
        self.root().join(self.route(agent))
    }

    /// What the file at `route` in the project holds, if it is there.
    fn read(&self, route: &str) -> Option<String> {
        fs::read_to_string(self.root().join(route)).ok()
    }

    fn write(&self, route: &str, text: &str) {
        let path = self.root().join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// How the server stands for each agent the project names.
    fn standings(&self) -> Vec<(Agent, Standing)> {
        install::entry_standings(&self.workspace, &self.ledger, &self.project, &self.name)
            .unwrap()
            .into_iter()
            .map(|(agent, standing)| (agent, standing.unwrap()))
            .collect()
    }

    /// The lines of the project's group of the exclude block.
    fn excluded(&self) -> Vec<String> {
        let id = self.project.settings().unwrap().id.to_string();
        exclude::listed(self.root(), &id)
            .unwrap()
            .map(|listing| listing.lines)
            .unwrap_or_default()
    }
}

/// The fingerprint `agent`'s renderer gives the entry of `definition`.
fn fingerprint(agent: Agent, definition: &McpDefinition) -> mindweaver_core::value::ContentHash {
    match agent {
        Agent::Claude => claude_mcp::entry(definition).unwrap().fingerprint(),
        Agent::Codex => codex_mcp::entry(definition).unwrap().fingerprint(),
        Agent::OpenCode => opencode_mcp::entry(definition, opencode_mcp::Shape::V2)
            .unwrap()
            .fingerprint(),
    }
}

/// The agents an installation names unreached, each by the code of why.
fn unreached(installation: &Installation) -> Vec<(Agent, &'static str)> {
    installation
        .entries
        .as_ref()
        .unwrap()
        .unreached
        .iter()
        .map(|unreached| (unreached.agent, unreached.why.code))
        .collect()
}

#[test]
fn a_server_is_given_to_every_agent_in_its_own_file_and_installing_it_again_writes_nothing() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    let definition = fixture.definition();

    let installed = fixture.install();

    let files: Vec<PathBuf> = Agent::ALL
        .into_iter()
        .map(|agent| fixture.file(agent))
        .collect();
    assert_eq!(
        installed.installed,
        Installed::Added {
            name: fixture.name.as_str().into(),
            paths: files.clone(),
            unhashed: None,
        }
    );
    assert_eq!(
        Agent::ALL.map(|agent| fixture.route(agent)),
        [".mcp.json", ".codex/config.toml", "opencode.json"]
    );
    for agent in Agent::ALL {
        let text = fixture.read(fixture.route(agent)).unwrap();
        assert!(text.contains(&fixture.name), "{agent:?}: {text}");
    }
    let record = fixture.record().unwrap();
    let recorded: Vec<(Agent, &str, _)> = record
        .entries
        .iter()
        .map(|entry| (entry.agent, entry.file.as_str(), entry.fingerprint.clone()))
        .collect();
    assert_eq!(
        recorded,
        Agent::ALL.map(|agent| (agent, fixture.route(agent), fingerprint(agent, &definition)))
    );
    assert!(record.targets.is_empty());
    assert_eq!(
        fixture.standings(),
        Agent::ALL.map(|agent| (agent, Standing::Current))
    );
    // Codex reads a project's file only in a project it trusts, and no
    // Codex home was given to say this one is: written, and counted as
    // reaching no Codex.
    assert_eq!(
        unreached(&installed),
        [(Agent::Codex, "codex_mcp_untrusted")]
    );

    let held: Vec<(String, std::time::SystemTime)> = files
        .iter()
        .map(|file| {
            (
                fs::read_to_string(file).unwrap(),
                fs::metadata(file).unwrap().modified().unwrap(),
            )
        })
        .collect();
    let again = fixture.install();

    assert_eq!(
        again.installed,
        Installed::Unchanged {
            name: fixture.name.as_str().into(),
            paths: files.clone(),
            unhashed: None,
        }
    );
    for (file, (text, modified)) in files.iter().zip(held) {
        assert_eq!(fs::read_to_string(file).unwrap(), text);
        assert_eq!(fs::metadata(file).unwrap().modified().unwrap(), modified);
    }
    assert_eq!(fixture.record().unwrap().entries, record.entries);
}

#[test]
fn somebodys_server_of_the_name_refuses_the_whole_install_with_nothing_written() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    let theirs = format!("[mcp_servers.{}]\ncommand = \"theirs\"\n", fixture.name);
    fixture.write(".codex/config.toml", &theirs);

    let installed = fixture.install();

    assert_eq!(
        installed.installed,
        Installed::Foreign {
            name: fixture.name.as_str().into(),
            path: fixture.file(Agent::Codex),
        }
    );
    assert_eq!(installed.installed.refused().unwrap().code(), "foreign");
    assert_eq!(fixture.read(".codex/config.toml").unwrap(), theirs);
    assert_eq!(fixture.read(".mcp.json"), None);
    assert_eq!(fixture.read("opencode.json"), None);
    assert_eq!(fixture.record(), None);
}

#[test]
fn an_agent_whose_renderer_refuses_the_server_is_not_reached_and_the_others_are_given_it() {
    // A secret passed on under a name other than its variable's, which
    // Codex, with no interpolation, cannot be given.
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::stdio);
    let refused = codex_mcp::entry(&fixture.definition()).unwrap_err();

    let installed = fixture.install();

    assert_eq!(
        installed.installed,
        Installed::Added {
            name: fixture.name.as_str().into(),
            paths: vec![fixture.file(Agent::Claude), fixture.file(Agent::OpenCode)],
            unhashed: None,
        }
    );
    assert_eq!(unreached(&installed), [(Agent::Codex, refused.code())]);
    let said = &installed.entries.as_ref().unwrap().unreached[0].why;
    assert_eq!(said.message.as_str(), refused.to_string());
    assert!(!fixture.root().join(".codex").exists());
    let agents: Vec<Agent> = fixture
        .record()
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.agent)
        .collect();
    assert_eq!(agents, [Agent::Claude, Agent::OpenCode]);
    // What Claude's is written without is said beside its entry.
    let given = &installed.entries.as_ref().unwrap().given;
    assert_eq!(given[0].agent, Agent::Claude);
    assert!(!given[0].warnings.is_empty(), "{given:?}");
}

#[test]
fn a_server_no_agent_the_project_names_is_given_is_refused_saying_why_of_each() {
    // An input read from no variable, which no renderer has a value for.
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::unread_input);

    let refused = fixture.try_install(Edits::Kept).unwrap_err();

    assert_eq!(refused.code(), "mcp_unreached");
    let message = refused.to_string();
    for code in [
        "claude: `",
        "codex: `",
        "opencode: `",
        "is read from no environment variable",
    ] {
        assert!(message.contains(code), "{message}");
    }
    for agent in Agent::ALL {
        assert_eq!(fixture.read(fixture.route(agent)), None, "{agent:?}");
    }
    assert_eq!(fixture.record(), None);
}

#[test]
fn a_project_that_names_no_agent_is_given_no_server() {
    let fixture = fixture(Share::Shared, &[], McpMother::plain_program);

    let refused = fixture.try_install(Edits::Kept).unwrap_err();

    assert_eq!(refused.code(), "no_target");
    assert_eq!(fixture.record(), None);
}

#[test]
fn a_file_git_tracks_in_a_local_project_is_left_as_it_is_and_its_agent_not_reached() {
    for share in [Share::Local, Share::Shared] {
        let fixture = fixture(share, &Agent::ALL, McpMother::plain_program);
        fixture.write(".mcp.json", TEAMS);
        common::indexed(fixture.dir.path());

        let installed = fixture.install();

        let agents: Vec<Agent> = fixture
            .record()
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.agent)
            .collect();
        match share {
            Share::Local => {
                assert_eq!(fixture.read(".mcp.json").unwrap(), TEAMS);
                assert_eq!(
                    unreached(&installed),
                    [
                        (Agent::Claude, "withheld_tracked"),
                        (Agent::Codex, "codex_mcp_untrusted"),
                    ]
                );
                assert_eq!(agents, [Agent::Codex, Agent::OpenCode]);
            }
            // The team's, to commit as a team: written.
            Share::Shared => {
                let text = fixture.read(".mcp.json").unwrap();
                assert!(text.contains(&fixture.name) && text.contains("docs-mcp"));
                assert_eq!(agents, Agent::ALL);
            }
        }
    }
}

#[test]
fn a_local_project_keeps_out_of_git_the_config_files_an_install_makes_and_no_other() {
    let fixture = fixture(Share::Local, &Agent::ALL, McpMother::plain_program);
    // Somebody's, which git does not track yet: theirs to commit.
    fixture.write(".mcp.json", TEAMS);

    fixture.install();

    assert_eq!(
        fixture.excluded(),
        ["/app/.codex/config.toml", "/app/opencode.json"]
    );
    for (route, made) in [
        (".mcp.json", false),
        (".codex/config.toml", true),
        ("opencode.json", true),
    ] {
        assert_eq!(
            fixture.ledger.created("app", route).unwrap(),
            made,
            "{route}"
        );
    }
}

/// Takes write permission away from a folder, and gives it back when
/// dropped, so that a failed assertion does not leave a temporary folder
/// that cannot be cleaned up.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    /// `None` where taking it away keeps nothing out, as for root.
    fn new(folder: &Path) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o555)).unwrap();
        let read_only = ReadOnly(folder.to_path_buf());
        let probe = folder.join(".probe");
        if fs::write(&probe, "").is_ok() {
            let _ = fs::remove_file(probe);
            eprintln!("skipped: {} can be written all the same", folder.display());
            return None;
        }
        Some(read_only)
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

// Unix only: a folder's write permission is what is taken away, and
// Windows keeps none on a folder.
#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_written_puts_back_every_file_written_before_it() {
    let fixture = fixture(Share::Local, &Agent::ALL, McpMother::plain_program);
    fixture.write(".mcp.json", TEAMS);
    // OpenCode's file, the last written, in a folder nothing can be
    // written in: Claude's and Codex's go first.
    fixture.write(".opencode/opencode.json", "{}\n");
    assert_eq!(fixture.route(Agent::OpenCode), ".opencode/opencode.json");
    let Some(_read_only) = ReadOnly::new(&fixture.root().join(".opencode")) else {
        return;
    };

    let failed = fixture.try_install(Edits::Kept).unwrap_err();

    assert_eq!(failed.code(), "config_write_failed", "{failed}");
    assert_eq!(fixture.read(".mcp.json").unwrap(), TEAMS);
    assert!(!fixture.root().join(".codex").exists());
    assert_eq!(fixture.read(".opencode/opencode.json").unwrap(), "{}\n");
    assert_eq!(fixture.record(), None);
    assert!(fixture.excluded().is_empty(), "{:?}", fixture.excluded());
    assert!(!fixture.ledger.created("app", ".codex/config.toml").unwrap());
}

#[test]
fn uninstalling_takes_out_each_entry_recorded_and_leaves_every_other_server_byte_for_byte() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.write(".mcp.json", TEAMS);
    fixture.write(".codex/config.toml", CODEX_TEAMS);
    fixture.write("opencode.json", OPENCODE_TEAMS);
    fixture.install();

    let removed = fixture.uninstall(Edits::Kept);

    assert_eq!(
        removed,
        Removed::Removed {
            name: fixture.name.as_str().into(),
            paths: Agent::ALL.map(|agent| fixture.file(agent)).to_vec(),
            still_excluded: None,
            rendered: None,
        }
    );
    assert_eq!(
        fixture.files(),
        [
            (Agent::Claude, Some(TEAMS.to_owned())),
            (Agent::Codex, Some(CODEX_TEAMS.to_owned())),
            (Agent::OpenCode, Some(OPENCODE_TEAMS.to_owned())),
        ]
    );
    assert_eq!(fixture.record(), None);
    assert_eq!(
        fixture.uninstall(Edits::Kept),
        Removed::Absent {
            name: fixture.name.as_str().into(),
        }
    );
}

#[test]
fn a_file_an_install_made_goes_with_its_last_server_and_its_exclude_line_with_it() {
    let fixture = fixture(Share::Local, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    assert_eq!(fixture.excluded().len(), 3, "{:?}", fixture.excluded());

    fixture.uninstall(Edits::Kept);

    assert_eq!(fixture.files(), Agent::ALL.map(|agent| (agent, None)));
    // Codex's own folder, which no record says Mindweaver made.
    assert!(fixture.root().join(".codex").is_dir());
    assert!(fixture.excluded().is_empty(), "{:?}", fixture.excluded());
    for agent in Agent::ALL {
        let route = fixture.route(agent);
        assert!(!fixture.ledger.created("app", route).unwrap(), "{route}");
    }
}

#[test]
fn a_file_an_install_made_and_somebody_took_away_is_forgotten_by_the_uninstall() {
    let fixture = fixture(Share::Local, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    fs::remove_file(fixture.file(Agent::Claude)).unwrap();

    fixture.uninstall(Edits::Kept);

    assert!(fixture.excluded().is_empty(), "{:?}", fixture.excluded());
    assert!(!fixture.ledger.created("app", ".mcp.json").unwrap());

    // So the one written there later is theirs: in git's sight, and kept
    // when the next server in it goes.
    let theirs = "{\n  \"mcpServers\": {}\n}\n";
    fixture.write(".mcp.json", theirs);
    fixture.install();
    assert!(!fixture.excluded().contains(&"/app/.mcp.json".to_owned()));

    fixture.uninstall(Edits::Kept);

    assert_eq!(fixture.read(".mcp.json").unwrap(), theirs);
}

#[test]
fn an_entry_edited_by_hand_refuses_the_uninstall_until_it_is_forced() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    let text = fixture.read(".mcp.json").unwrap();
    let edited = text.replacen("\"npx\"", "\"bunx\"", 1);
    assert_ne!(edited, text);
    fixture.write(".mcp.json", &edited);
    let before = fixture.files();

    let kept = fixture.uninstall(Edits::Kept);

    assert_eq!(
        kept,
        Removed::Edited {
            name: fixture.name.as_str().into(),
            paths: vec![fixture.file(Agent::Claude)],
        }
    );
    assert_eq!(kept.refused().unwrap().code(), "edited");
    assert_eq!(fixture.files(), before);
    assert!(fixture.record().is_some());

    let forced = fixture.uninstall(Edits::Forced);

    assert!(matches!(forced, Removed::Removed { .. }), "{forced:?}");
    for (agent, text) in fixture.files() {
        assert!(
            !text.unwrap_or_default().contains(&fixture.name),
            "{agent:?}"
        );
    }
    assert_eq!(fixture.record(), None);
}

#[test]
fn a_server_nothing_records_is_somebodys_and_left_where_it_is() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    let theirs = format!("[mcp_servers.{}]\ncommand = \"theirs\"\n", fixture.name);
    fixture.write(".codex/config.toml", &theirs);

    let removed = fixture.uninstall(Edits::Forced);

    assert_eq!(
        removed,
        Removed::Foreign {
            name: fixture.name.as_str().into(),
            path: fixture.file(Agent::Codex),
        }
    );
    assert_eq!(fixture.read(".codex/config.toml").unwrap(), theirs);
}

#[test]
fn an_entry_in_a_file_git_tracks_in_a_local_project_refuses_the_uninstall() {
    let mut fixture = fixture(Share::Local, &Agent::ALL, McpMother::plain_program);
    fixture
        .project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap();
    fixture.write(".mcp.json", TEAMS);
    common::indexed(fixture.dir.path());
    fixture.install();
    fixture
        .project
        .change_settings(|settings| settings.edit_tracked = false)
        .unwrap();
    let before = fixture.files();

    let refused = fixture.try_uninstall(Edits::Forced).unwrap_err();

    assert_eq!(refused.code(), "mcp_entry_unedited", "{refused}");
    assert_eq!(fixture.files(), before);
    assert!(fixture.record().is_some());
}

#[test]
fn an_entry_edited_by_hand_reads_as_edited_and_an_update_leaves_it_until_forced() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    let text = fixture.read(".mcp.json").unwrap();
    fixture.write(".mcp.json", &text.replacen("\"npx\"", "\"bunx\"", 1));
    let before = fixture.files();

    assert_eq!(
        fixture.standings(),
        [
            (Agent::Claude, Standing::Edited),
            (Agent::Codex, Standing::Current),
            (Agent::OpenCode, Standing::Current),
        ]
    );
    assert_eq!(fixture.installation(), Some(Standing::Edited));

    let left = fixture.update(Edits::Kept);

    assert_eq!(left.step, Step::Leave(vec![fixture.file(Agent::Claude)]));
    assert!(left.done.is_none());
    assert_eq!(fixture.files(), before);
    assert_eq!(fixture.installation(), Some(Standing::Edited));

    let forced = fixture.update(Edits::Forced);

    assert_eq!(
        forced.step,
        Step::WriteOver(vec![fixture.file(Agent::Claude)])
    );
    assert!(forced.done.unwrap().is_ok());
    assert_eq!(fixture.read(".mcp.json").unwrap(), text);
    assert_eq!(fixture.installation(), Some(Standing::Current));
}

#[test]
fn an_outdated_entry_is_written_again_and_a_missing_one_put_back() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    let before = fixture.read(".mcp.json").unwrap();
    // The library's definition starts its program otherwise now.
    fixture.define(&McpMother::plain_program(&fixture.name).replacen("\"npx\"", "\"bunx\"", 1));
    assert_eq!(fixture.installation(), Some(Standing::Outdated));

    let refreshed = fixture.update(Edits::Kept);

    assert_eq!(refreshed.step, Step::Refresh);
    assert!(matches!(
        refreshed.done.unwrap().unwrap().installed,
        Installed::Updated { .. }
    ));
    let after = fixture.read(".mcp.json").unwrap();
    assert_eq!(after, before.replacen("\"npx\"", "\"bunx\"", 1));
    assert_eq!(
        fixture.standings(),
        Agent::ALL.map(|agent| (agent, Standing::Current))
    );

    fs::remove_file(fixture.file(Agent::OpenCode)).unwrap();
    assert_eq!(fixture.installation(), Some(Standing::Missing));

    let put_back = fixture.update(Edits::Kept);

    assert_eq!(
        put_back.step,
        Step::PutBack(vec![fixture.file(Agent::OpenCode)])
    );
    assert!(put_back.done.unwrap().is_ok());
    assert!(fixture
        .read("opencode.json")
        .unwrap()
        .contains(&fixture.name));
    assert_eq!(fixture.read(".mcp.json").unwrap(), after);
    assert_eq!(fixture.installation(), Some(Standing::Current));
}

#[test]
fn an_update_takes_out_the_entry_of_an_agent_the_definition_no_longer_reaches() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    // Passed a secret under a name other than its variable's now, which
    // Codex cannot be given.
    let name = fixture.name.clone();
    fixture.define(&McpMother::stdio(&name));
    let refusal = codex_mcp::entry(&fixture.definition()).unwrap_err();
    assert_eq!(fixture.installation(), Some(Standing::Outdated));

    let refreshed = fixture.update(Edits::Kept);

    assert_eq!(refreshed.step, Step::Refresh);
    let done = refreshed.done.unwrap().unwrap();
    assert!(
        matches!(done.installed, Installed::Updated { .. }),
        "{:?}",
        done.installed
    );
    assert_eq!(unreached(&done), [(Agent::Codex, refusal.code())]);
    // The file the install made goes with it, as an uninstall takes it.
    assert_eq!(fixture.read(".codex/config.toml"), None);
    let agents: Vec<Agent> = fixture
        .record()
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.agent)
        .collect();
    assert_eq!(agents, [Agent::Claude, Agent::OpenCode]);
    assert!(!fixture.ledger.created("app", ".codex/config.toml").unwrap());
    // So an update and what is said of the installation agree.
    assert_eq!(fixture.installation(), Some(Standing::Current));
    assert_eq!(fixture.update(Edits::Kept).step, Step::Keep);
}

#[test]
fn a_server_nothing_offers_any_more_is_said_by_an_update_and_left() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    let before = fixture.files();
    let folder = fixture.workspace.own().directory_for(Kind::Mcp).unwrap();
    fs::remove_file(folder.join(format!("{}.toml", fixture.name))).unwrap();

    let orphaned = fixture.update(Edits::Forced);

    assert_eq!(orphaned.step, Step::Orphaned);
    assert!(orphaned.done.is_none());
    assert_eq!(fixture.files(), before);
    assert!(fixture.record().is_some());
}

#[test]
fn a_change_aimed_at_a_server_the_project_received_is_refused_as_received() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();
    let find = |workspace: Option<&WeaverWorkspace>, typed: &str| {
        lifecycle::find_to_change(workspace, &fixture.project, &Reference::parse(typed))
            .unwrap_err()
    };
    let qualified = format!("mcp:{}", fixture.name);

    let received = find(Some(&fixture.workspace), &qualified);

    assert!(
        matches!(&received, LifecycleError::ReceivedServer { typed, project, .. }
            if *typed == qualified && project == "app"),
        "{received:?}"
    );
    assert_eq!(received.code(), "change_received_mcp");
    // Named bare however it was typed, the ledger recording the name as no
    // other kind: the form an uninstall finds it by.
    assert!(received.to_string().ends_with(&format!(
        "uninstall it as `{}`, or change its definition in the library it came from and \
         update",
        fixture.name
    )));
    assert!(matches!(
        find(Some(&fixture.workspace), &fixture.name),
        LifecycleError::ReceivedServer { typed, named, .. }
            if *typed == fixture.name && *named == fixture.name
    ));
    assert!(matches!(
        find(Some(&fixture.workspace), &format!("skill:{}", fixture.name)),
        LifecycleError::NotFound { .. }
    ));
    // Without the workspace whose ledger says so, nothing says it was
    // received.
    assert!(matches!(
        find(None, &qualified),
        LifecycleError::NotFound { .. }
    ));
}

/// What each agent of `installation` is given, as its line says it.
fn runs(installation: &Installation) -> Vec<(Agent, String)> {
    installation
        .entries
        .as_ref()
        .unwrap()
        .given
        .iter()
        .map(|given| (given.agent, given.runs.to_string()))
        .collect()
}

#[test]
fn an_install_hands_back_what_each_agent_runs_and_every_variable_its_entries_read() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::stdio);

    let installed = fixture.install();

    // Each input as the agent reads it, and no Codex, which cannot be
    // given a secret under a name other than its variable's.
    assert_eq!(
        runs(&installed),
        [
            (
                Agent::Claude,
                String::from("npx -y @example/server --workspace ${EXAMPLE_WORKSPACE}")
            ),
            (
                Agent::OpenCode,
                String::from("npx -y @example/server --workspace {env:EXAMPLE_WORKSPACE}")
            ),
        ]
    );
    let entries = installed.entries.as_ref().unwrap();
    assert!(entries
        .given
        .iter()
        .all(|given| given.written == Some(mindweaver_core::config_entry::Written::Created)));
    assert_eq!(
        entries.variables,
        [
            Variable {
                name: "EXAMPLE_API_KEY".into(),
                secret: true,
            },
            Variable {
                name: "EXAMPLE_WORKSPACE".into(),
                secret: false,
            },
        ]
    );
}

#[test]
fn a_preview_says_what_each_agent_would_run_and_why_one_is_not_reached_writing_nothing() {
    let fixture = fixture(Share::Local, &Agent::ALL, McpMother::stdio);
    let refused = codex_mcp::entry(&fixture.definition()).unwrap_err();

    let previewed = fixture.preview();

    assert_eq!(
        previewed.installed,
        Installed::Unconfirmed {
            name: fixture.name.as_str().into(),
            paths: vec![fixture.file(Agent::Claude), fixture.file(Agent::OpenCode)],
            recorded: false,
        }
    );
    assert!(previewed
        .entries
        .as_ref()
        .unwrap()
        .given
        .iter()
        .all(|given| given.written.is_none()));
    assert_eq!(unreached(&previewed), [(Agent::Codex, refused.code())]);
    assert!(fixture.files().iter().all(|(_, text)| text.is_none()));
    assert!(fixture.record().is_none());
    assert!(fixture.excluded().is_empty());
    // What it says each agent would run is what the install then writes.
    let installed = fixture.install();
    assert_eq!(runs(&installed), runs(&previewed));
    assert_eq!(
        installed.entries.unwrap().variables,
        previewed.entries.unwrap().variables
    );
}

#[test]
fn a_preview_meets_somebodys_server_and_codex_s_trust_as_the_install_would() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);

    let untrusted = fixture.preview();
    fixture.write(
        ".mcp.json",
        &TEAMS.replace("\"docs\"", &format!("\"{}\"", fixture.name)),
    );
    let foreign = fixture.preview();

    assert_eq!(
        unreached(&untrusted),
        [(Agent::Codex, "codex_mcp_untrusted")]
    );
    assert_eq!(
        runs(&untrusted),
        Agent::ALL.map(|agent| (agent, String::from("npx -y @example/server --workspace .")))
    );
    assert_eq!(
        foreign.installed,
        Installed::Foreign {
            name: fixture.name.as_str().into(),
            path: fixture.file(Agent::Claude),
        }
    );
    assert!(fixture.record().is_none());
}

/// A preview comes to what the install would: nothing to change once each
/// agent holds what it would be given, as recorded, and an update of what
/// the ledger records once the definition changes.
#[test]
fn a_preview_tells_a_server_already_current_and_one_it_would_update() {
    let fixture = fixture(Share::Shared, &Agent::ALL, McpMother::plain_program);
    fixture.install();

    let current = fixture.preview();
    fixture.define(&McpMother::plain_program(&fixture.name).replacen(
        "--workspace",
        "--verbose",
        1,
    ));
    let changed = fixture.preview();

    let paths: Vec<PathBuf> = Agent::ALL.map(|agent| fixture.file(agent)).to_vec();
    assert_eq!(
        current.installed,
        Installed::Unchanged {
            name: fixture.name.as_str().into(),
            paths: paths.clone(),
            unhashed: None,
        }
    );
    assert_eq!(
        changed.installed,
        Installed::Unconfirmed {
            name: fixture.name.as_str().into(),
            paths,
            recorded: true,
        }
    );
    // What it says is what the install then comes to.
    let updated = fixture.install();
    assert!(
        matches!(updated.installed, Installed::Updated { .. }),
        "{:?}",
        updated.installed
    );
    assert!(matches!(
        fixture.preview().installed,
        Installed::Unchanged { .. }
    ));
}
