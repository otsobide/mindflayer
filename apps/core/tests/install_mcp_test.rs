//! An MCP server installed into a project, an entry in the config file of
//! each agent it names (D-10): what each file holds afterwards, what the
//! ledger records of it, what is refused and what is left as it was, each
//! file built here in a temporary folder, none in a real agent's home.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::install::{self, Edits, InstallError, Installation, Installed, Standing};
use mindweaver_core::ledger::InstallRecord;
use mindweaver_core::mother::McpMother;
use mindweaver_core::{
    claude_mcp, codex_mcp, exclude, opencode_mcp, Agent, AgentHomes, Directories, Kind, Ledger,
    McpDefinition, MindProject, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A team's own `.mcp.json`, holding a server of theirs.
const TEAMS: &str = "{\n  \"mcpServers\": {\n    \"docs\": {\n      \"type\": \"stdio\",\n      \
                     \"command\": \"docs-mcp\"\n    }\n  }\n}\n";

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
