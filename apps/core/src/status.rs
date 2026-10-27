//! What each project holds, where each thing in it came from, which agents
//! read it, and whether it is all there: what `flayer status` reports.
//!
//! Built from discovery and the ledger together, since neither answers
//! alone. Discovery says what sits in the folders a project's agents read and
//! the one it authors in, and which agents read each copy ([`Catalog`]); it
//! reads no ledger, so it cannot tell an install from somebody's own. The
//! ledger says which copies Mindflayer put where, and from where
//! ([`Ledger::records`]); it cannot see that one has gone since, nor a copy
//! it never wrote.
//!
//! [`Ledger::records`]: crate::ledger::Ledger::records
//!
//! Core builds and orders the report, so every front end shows the same
//! states in the same order: the CLI's table and its JSON, and the TUI's rows.
//! What it serializes to is a contract from the first report on, which is why
//! each name in it is spelled as a command line takes it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::agent::Agent;
use crate::agents_md;
use crate::artifact::{Artifact, Provenance};
use crate::catalog::{Catalog, DiscoveryFailure};
use crate::install;
use crate::kind::Kind;
use crate::ledger::{InstallRecord, Target};
use crate::paths;
use crate::workspace::{
    same_directory, FlayerWorkspace, MindProject, MisplacedTarget, WorkspaceError,
};

/// What every project a report covers holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    /// In the order the workspace's marker lists them, as every screen lists
    /// them, each directory once however many entries point at it.
    pub projects: Vec<ProjectStatus>,
}

/// One project, and everything it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectStatus {
    /// What its marker calls it; none for an entry that would not open, which
    /// has no marker to read.
    pub name: Option<String>,
    /// The workspace's entry for it, as the marker spells it: what `-p` and
    /// `flayer unlink` take. None for a project no workspace links.
    pub entry: Option<String>,
    /// Where it is.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The agents its settings name, in report order: none for a project a
    /// `mind.toml` marks, which names none.
    pub agents: Vec<Agent>,
    /// Why its entry would not open, when it would not: a stale entry is
    /// listed rather than left out, since it is the one somebody has to find
    /// to unlink it.
    pub stale: Option<String>,
    /// Each kind installed somewhere none of some of its agents reads, by
    /// kind in report order: what a project's `targets` can leave, since a
    /// folder of its own reaches an agent only once that agent is pointed at
    /// it. None under the folders each agent reads, which is every project
    /// whose settings name no `targets`.
    pub unreached: Vec<Unreached>,
    /// Each rule it received that some of its agents read nowhere, all of it
    /// there as it was installed, in the order of `artifacts`: one about
    /// some files only, which Claude alone reads, or one Codex and OpenCode
    /// read nowhere because no block could be written for them.
    ///
    /// Said of each rule rather than of its kind, as `unreached` says a
    /// skill's: a skill reaches the agents that read the folders its kind is
    /// installed in, the same for every skill, while a rule reaches the ones
    /// what it states and where its block may go let it. One partly gone is
    /// partial already, which says so.
    pub missed: Vec<Missed>,
    /// Everything it holds, grouped by kind and ordered by name.
    pub artifacts: Vec<ArtifactStatus>,
}

/// One artifact a project received that some of its agents read nowhere,
/// and those agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Missed {
    pub kind: Kind,
    pub name: String,
    /// The agents among the project's that read it nowhere, in report
    /// order. Never empty.
    pub agents: Vec<Agent>,
}

/// A kind a project installs only in folders some of its agents do not read,
/// and those agents.
///
/// Told by where each agent reads ([`Agent::reads`]), matched by spelling and
/// then on disk, so a `.claude/skills` linked to the project's `.skills` is
/// Claude reading it: pointing an agent at a folder is up to whoever keeps
/// the project, and a link is one way the report can see. An agent told
/// some other way, by a setting of its own, is beyond what docs/agents.md
/// says it reads, and is still said here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unreached {
    pub kind: Kind,
    /// Every folder the kind is installed in, as a route from the project,
    /// with `/` whatever the platform, in the order an install writes them.
    pub targets: Vec<String>,
    /// The agents among the project's that read none of them, in report
    /// order. Never empty.
    pub agents: Vec<Agent>,
}

/// One artifact a project holds, and how it stands.
///
/// One a name, as discovery lists one ([`Artifact::copies`]): every copy in
/// the project's agents' folders and the one it authors in, together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactStatus {
    pub kind: Kind,
    pub name: String,
    pub state: State,
    /// Every agent among the project's that reads one of its copies, in
    /// report order ([`Artifact::agents`]): what it reaches, which for a
    /// partial install is fewer than it was installed for.
    pub agents: Vec<Agent>,
    /// Where what was installed came from, as `--from` takes it: `workspace`,
    /// `load:<entry>`, or a shelf source's address. None when the ledger
    /// records no copy of it, and for a record from before origins were kept
    /// of something that came from no source.
    pub origin: Option<String>,
}

/// How an artifact stands in a project, as far as its copies and the ledger
/// say.
///
/// The outdated and edited states are #47's and #48's, once installs are
/// compared with what they came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum State {
    /// Every copy is one the ledger records, and every copy it records is
    /// found where it was put.
    Installed,
    /// The ledger records a copy that is not found where it was put any
    /// more: deleted since, by hand or by `git clean -X`, or left unreadable.
    /// The next install of it puts every copy back. Said over the project's
    /// own copy of the name too, which sits where the install wrote nothing.
    Partial,
    /// In an agent's folder outside any the project authors in, and recorded
    /// by nothing: somebody's, put there some other way, and never
    /// Mindflayer's to change.
    Foreign,
    /// In a folder the project authors in, and recorded by nothing: the
    /// project's own work, which a change made from it reaches.
    Authored,
}

impl State {
    /// Every state, in report order.
    pub const ALL: [State; 4] = [
        State::Installed,
        State::Partial,
        State::Foreign,
        State::Authored,
    ];

    /// How it is written in a report, in text and in JSON alike.
    pub const fn slug(self) -> &'static str {
        match self {
            State::Installed => "installed",
            State::Partial => "partial",
            State::Foreign => "foreign",
            State::Authored => "authored",
        }
    }
}

/// By its slug, for the reason a kind is: one spelling for the table and the
/// JSON, which a renamed variant cannot change.
impl Serialize for State {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

/// What a report could not read on its way, collected rather than raised: one
/// stale entry or unreadable skill must not hide the rest.
#[derive(Debug, Error)]
pub enum StatusFailure {
    /// A registry entry that would not open, which the report lists as stale.
    #[error(transparent)]
    Stale(WorkspaceError),
    /// What discovery could not read in a project, or the ledger.
    #[error(transparent)]
    Discovery(#[from] DiscoveryFailure),
    /// A folder a project's settings name in `targets` that no install uses,
    /// said wherever the project is reported, since the report reads its
    /// installs as going everywhere else they name.
    #[error(transparent)]
    Target(#[from] MisplacedTarget),
}

impl Status {
    /// Every project `workspace` links, against what its ledger records of
    /// them.
    ///
    /// Only the linked ones, as `flayer list` reads them: a workspace reports
    /// on what it was told to manage, not on whatever sits below it. Its own
    /// artifacts are no project's, and nothing is installed into them.
    ///
    /// A ledger that cannot be read fails every project's read the same way,
    /// about its one file, so it is said once, with the first.
    pub fn of_workspace(workspace: &FlayerWorkspace) -> (Status, Vec<StatusFailure>) {
        let mut status = Status::default();
        let mut failures = Vec::new();
        let mut ledger_said = false;
        let mut seen: Vec<PathBuf> = Vec::new();
        for member in workspace.members() {
            // Two spellings of one directory are one project, reported under
            // the first, rather than everything in it twice.
            if seen.iter().any(|root| same_directory(root, &member.root)) {
                continue;
            }
            seen.push(member.root.clone());
            match member.project {
                Ok(project) => {
                    let (held, catalog) =
                        ProjectStatus::read(&project, &Kind::ALL, Some(workspace));
                    let (_, found) = catalog.into_parts();
                    for failure in found {
                        let ledger = matches!(failure, DiscoveryFailure::Ledger(_));
                        if !(ledger && ledger_said) {
                            failures.push(StatusFailure::from(failure));
                        }
                        ledger_said |= ledger;
                    }
                    failures.extend(
                        project
                            .misplaced_targets()
                            .into_iter()
                            .map(StatusFailure::from),
                    );
                    status.projects.push(held);
                }
                Err(error) => {
                    status.projects.push(ProjectStatus {
                        name: None,
                        entry: Some(spelled(&member.entry)),
                        path: member.root,
                        agents: Vec::new(),
                        stale: Some(error.to_string()),
                        unreached: Vec::new(),
                        missed: Vec::new(),
                        artifacts: Vec::new(),
                    });
                    failures.push(StatusFailure::Stale(error));
                }
            }
        }
        (status, failures)
    }

    /// One project, against the ledger of `workspace` when that one links
    /// it.
    ///
    /// A project no workspace manages is still reported: what is in it is
    /// still its own or somebody's, and only that nothing records an install
    /// in it is missing, which reads as nothing installed.
    pub fn of_project(
        project: &MindProject,
        workspace: Option<&FlayerWorkspace>,
    ) -> (Status, Vec<StatusFailure>) {
        let (held, catalog) = ProjectStatus::read(project, &Kind::ALL, workspace);
        let (_, failures) = catalog.into_parts();
        let misplaced = project.misplaced_targets().into_iter();
        (
            Status {
                projects: vec![held],
            },
            failures
                .into_iter()
                .map(StatusFailure::from)
                .chain(misplaced.map(StatusFailure::from))
                .collect(),
        )
    }
}

impl ProjectStatus {
    /// Read `project`'s artifacts of `kinds` and say how each stands, against
    /// what `workspace`'s ledger says was installed, when that workspace
    /// links the project: what both reports and the TUI's rows build each
    /// project with.
    ///
    /// The ledger is opened here, for reading
    /// ([`FlayerWorkspace::ledger_for_reading`]), rather than handed in, so
    /// no front end can refuse a report over one that will not open while
    /// another reports without it. One that cannot be opened or read leaves
    /// every copy unrecorded, as `with_records` does: foreign or authored,
    /// which nothing changes, rather than installed, which an uninstall may
    /// delete.
    ///
    /// The catalog read comes back too, every copy the ledger records marked
    /// installed ([`Catalog::with_records`]) and a ledger that could not be
    /// read among its failures, for a front end that shows more of each
    /// artifact than how it stands, a screen's summaries and problems, without
    /// reading the project twice.
    pub fn read(
        project: &MindProject,
        kinds: &[Kind],
        workspace: Option<&FlayerWorkspace>,
    ) -> (ProjectStatus, Catalog) {
        let root = project.root();
        let mut catalog = Catalog::discover_kinds(std::slice::from_ref(project), kinds);
        let mut entry = None;
        let mut installed: Vec<InstallRecord> = Vec::new();
        // Records are a workspace's own, so a project it does not link has
        // none there, as `with_records` reads it, and its ledger is not
        // opened for it.
        if let Some((workspace, linked)) =
            workspace.and_then(|workspace| Some((workspace, workspace.linked_as(root)?)))
        {
            entry = Some(spelled(linked));
            let named = install::entry_for_root(workspace, root);
            let read = workspace.ledger_for_reading().and_then(|ledger| {
                let records = kinds
                    .iter()
                    .map(|kind| ledger.records(&named, *kind))
                    .collect::<Result<Vec<Vec<InstallRecord>>, _>>()?;
                Ok((ledger, records))
            });
            // Said once, rather than again by `with_records` failing on the
            // same ledger too.
            catalog = match read {
                Ok((ledger, read)) => {
                    installed = read.into_iter().flatten().collect();
                    catalog.with_records(workspace, &ledger)
                }
                Err(error) => catalog.failing(DiscoveryFailure::Ledger(error)),
            };
        }
        let mut agents = project
            .settings()
            .map(|settings| settings.agents.clone())
            .unwrap_or_default();
        agents.sort();
        agents.dedup();
        // What a project marked by its settings received as rules is no
        // copy discovery finds, but sections of a block and Claude's files:
        // read from the ledger and that block (D-23).
        let (rules, installed): (Vec<InstallRecord>, Vec<InstallRecord>) = installed
            .into_iter()
            .partition(|record| record.kind == Kind::Rule && project.settings().is_some());
        let received: Vec<String> = rules.iter().map(|record| record.name.clone()).collect();
        let rendered = rendered(root, &agents, rules);
        let artifacts = standing(root, catalog.artifacts(), installed, rendered);
        let status = ProjectStatus {
            name: Some(project.name().to_owned()),
            entry,
            path: root.to_path_buf(),
            stale: None,
            unreached: unreached(project, kinds, &agents),
            missed: missed(&agents, &artifacts, &received),
            agents,
            artifacts,
        };
        (status, catalog)
    }

    /// How the artifact of `kind` named `name` stands, if the project holds
    /// one by that name.
    pub fn find(&self, kind: Kind, name: &str) -> Option<&ArtifactStatus> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.kind == kind && artifact.name == name)
    }
}

/// How each of `artifacts`, all found in the project at `root`, stands
/// against `records`, every installation into it, with an entry of its own
/// for each installation nothing of which is found.
///
/// An artifact's record is the one that wrote one of its copies, matched by
/// path as discovery marks a copy installed, not by name, so an install of
/// one name never claims what somebody keeps under it elsewhere. Its state is
/// what discovery says of its copies ([`Artifact::provenance`]): installed
/// only when the ledger records every one, and partial when the record names
/// a copy discovery did not find where it was put, whether the rest are all
/// recorded or the project authors a copy of its own beside them.
///
/// A record no copy is found of is an install every copy of which is gone,
/// listed as partial with no agent, so that what was installed is said even
/// once nothing of it is left; unless the project holds something else by
/// that name, which is then that name's one entry, said as what is there:
/// a row is one name, and that one is somebody's.
///
/// `rendered` are the rules a project marked by its settings received, as
/// [`rendered`] reads them, each listed under the same rule: after what
/// discovery found, and not under a name the project holds something else
/// by.
fn standing(
    root: &Path,
    artifacts: &[Artifact],
    records: Vec<InstallRecord>,
    rendered: Vec<ArtifactStatus>,
) -> Vec<ArtifactStatus> {
    let mut unclaimed = records;
    let mut held: Vec<ArtifactStatus> = Vec::new();
    for artifact in artifacts {
        let routes: Vec<String> = artifact
            .copies()
            .iter()
            .map(|copy| install::route(root, copy.path()))
            .collect();
        let record = unclaimed
            .iter()
            .position(|record| {
                record.kind == artifact.kind()
                    && record
                        .targets
                        .iter()
                        .any(|target| routes.contains(&target.path))
            })
            .map(|index| unclaimed.remove(index));
        let missing = record.as_ref().is_some_and(|record| {
            record
                .targets
                .iter()
                .any(|target| !routes.contains(&target.path))
        });
        let state = match artifact.provenance() {
            // Somebody's copy in an agent's folder is said first, even beside
            // an install with a copy gone: the next install of the name is
            // refused over it, so partial would promise what it cannot keep.
            Provenance::Foreign => State::Foreign,
            // A project's own copy of the name, beside an install of it, does
            // not hide a copy of that install gone. It sits where the install
            // wrote nothing, or it would be the install's, so the next one
            // puts the copy back all the same; and an agent left reading none
            // would otherwise go unsaid.
            Provenance::Installed | Provenance::Authored if missing => State::Partial,
            Provenance::Installed => State::Installed,
            Provenance::Authored => State::Authored,
        };
        held.push(ArtifactStatus {
            kind: artifact.kind(),
            name: artifact.name().to_owned(),
            state,
            agents: artifact.agents(),
            origin: record.and_then(|record| record.origin),
        });
    }
    for rule in rendered {
        if !held
            .iter()
            .any(|other| other.kind == rule.kind && other.name == rule.name)
        {
            held.push(rule);
        }
    }
    for record in unclaimed {
        if held
            .iter()
            .any(|other| other.kind == record.kind && other.name == record.name)
        {
            continue;
        }
        held.push(ArtifactStatus {
            kind: record.kind,
            name: record.name,
            state: State::Partial,
            agents: Vec::new(),
            origin: record.origin,
        });
    }
    held.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    held
}

/// How each rule recorded as installed into the project at `root`, which its
/// settings mark, stands, from the ledger and the block of its `AGENTS.md`
/// (D-23): catalog discovery reads no block, since a section is never
/// authored in place but rendered from a rule kept elsewhere.
///
/// Installed while every target recorded is found, its section in the block
/// and its file for Claude where it was written, and partial while one is
/// not, as for a skill's copies; a rule recorded with none, which reaches no
/// agent the project names, has nothing missing. It reaches Codex and
/// OpenCode through the block, and Claude through its own file, or through
/// the block when it was given none: a render gives Claude every rule as a
/// file while Claude reads no block, so a rule in the block with no file of
/// its own is one Claude reads there. Only the agents among `agents`, the
/// project's own.
///
/// A block that cannot be read holds none of them, and every rule in it
/// reads as partial: the next render refuses it, saying why.
fn rendered(root: &Path, agents: &[Agent], records: Vec<InstallRecord>) -> Vec<ArtifactStatus> {
    if records.is_empty() {
        return Vec::new();
    }
    let block = install::rules::block_routes(root);
    records
        .into_iter()
        .map(|record| {
            let file = install::rules::claude_file(&record.name);
            let found = |target: &Target| {
                if target.path == agents_md::FILE {
                    block.contains(&record.name)
                } else {
                    fs::symlink_metadata(root.join(&target.path)).is_ok()
                }
            };
            let missing = record.targets.iter().any(|target| !found(target));
            let in_block = record
                .targets
                .iter()
                .any(|target| target.path == agents_md::FILE && found(target));
            let given_file = record.targets.iter().find(|target| target.path == file);
            let in_file = given_file.is_some_and(found);
            let reached = agents
                .iter()
                .copied()
                .filter(|agent| match agent {
                    Agent::Claude => in_file || (in_block && given_file.is_none()),
                    Agent::Codex | Agent::OpenCode => in_block,
                })
                .collect();
            ArtifactStatus {
                kind: Kind::Rule,
                name: record.name,
                state: if missing {
                    State::Partial
                } else {
                    State::Installed
                },
                agents: reached,
                origin: record.origin,
            }
        })
        .collect()
}

/// Each of `artifacts` that is one of the rules a project marked by its
/// settings `received`, installed whole, with the agents among `agents`,
/// its own, that read it nowhere ([`ProjectStatus::missed`]).
///
/// From the rows as the report lists them, so what is said of a rule is
/// what its row says it reaches.
fn missed(agents: &[Agent], artifacts: &[ArtifactStatus], received: &[String]) -> Vec<Missed> {
    artifacts
        .iter()
        .filter(|held| {
            held.kind == Kind::Rule
                && held.state == State::Installed
                && received.contains(&held.name)
        })
        .filter_map(|held| {
            let agents: Vec<Agent> = agents
                .iter()
                .copied()
                .filter(|agent| !held.agents.contains(agent))
                .collect();
            (!agents.is_empty()).then(|| Missed {
                kind: held.kind,
                name: held.name.clone(),
                agents,
            })
        })
        .collect()
}

/// Each of `kinds` that `project` installs only in folders some of `agents`,
/// its own, do not read, with those agents.
///
/// From where an install of each would put its copies
/// ([`install::targets`]), so two spellings of one folder are one, and a kind
/// installed in no folder is passed over: a rule, which reaches a project
/// marked by its settings through the block of its `AGENTS.md`, says which
/// agents it reaches as each rule's own entry does ([`rendered`]).
pub(crate) fn unreached(project: &MindProject, kinds: &[Kind], agents: &[Agent]) -> Vec<Unreached> {
    let root = project.root();
    kinds
        .iter()
        .filter_map(|&kind| {
            let targets = install::targets(project, kind).ok()?;
            let missed: Vec<Agent> = agents
                .iter()
                .copied()
                .filter(|agent| {
                    !targets.iter().any(|target| {
                        agent
                            .reads(kind)
                            .iter()
                            .any(|read| same_directory(&root.join(read), &target.folder))
                    })
                })
                .collect();
            (!missed.is_empty()).then(|| Unreached {
                kind,
                targets: targets
                    .iter()
                    .map(|target| {
                        let route = paths::relative_to(&target.folder, root)
                            .unwrap_or_else(|| target.folder.clone());
                        spelled(&route)
                    })
                    .collect(),
                agents: missed,
            })
        })
        .collect()
}

/// A registry entry as the marker spells it, with `/` whatever the platform.
fn spelled(entry: &Path) -> String {
    paths::to_config_string(entry).unwrap_or_else(|| entry.display().to_string())
}

#[cfg(test)]
#[path = "status_test.rs"]
mod tests;
