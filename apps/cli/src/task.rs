//! Work a front end asks for, as the command lines it is.
//!
//! A [`Task`] is what a key in the TUI asks for, and what the panel will ask
//! for over the local server (D-18): not a description of the work, but the
//! work, since [`Task::commands`] is exactly the words the caller parses,
//! with the parsers `mind` and `flayer` use, and runs. Here rather than in
//! the TUI, where it was born, so that every front end that runs commands
//! builds its lines one way: a second one writing its own would be a second
//! place for a line to be spelled wrong, or for a value to be taken for a
//! flag.
//!
//! **No value a task holds is read as an option, or as a command.** A task
//! sent by a browser holds whatever the browser put in it, so a path, a name
//! or a description that starts with a dash has to reach the command as
//! that value: `--yes` as a description must not confirm anything. The words
//! of the command itself are fixed here, never taken from a field, so a
//! value cannot name another command, and each value goes one of three
//! ways: after the option that takes it, joined to it by `=` when it starts
//! with a dash (`option`); as a `-C` or `-p` that starts with one, behind a
//! `./` that names the same directory (`directory`); or as a positional,
//! behind a `--` whenever one of them starts with a dash (`words`). A test
//! feeds every variant values that start with a dash, and finds each line
//! read as the same line with a plain value is, each option and positional
//! holding the value as given wherever the plain one was, behind its `./`
//! under a `-C` or a `-p`.
//! Shown, a line is quoted so that, pasted into a shell, it runs as one
//! command of exactly these words ([`shell`]).

use clap::Parser as _;
use mindflayer_core::install::Change;
use mindflayer_core::url::redacted;
use mindflayer_core::workspace::SCRATCH_DIR;
use mindflayer_core::{
    codes, hash, Agent, Kind, Share, DEFAULT_SUBDIRECTORY, FLAYER_DIR, QUALIFIER,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Cli, Command, Editing, FlayerCli, FlayerCommand, GatherCommand, Given};

/// What an `agents` line says, as the toggles have it.
///
/// `{"type": "named", "agents": ["claude"]}` in JSON: tagged as a task is,
/// with the agents beside the tag, since a variant holding a list has no
/// field of its own for them to be named by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(
    tag = "type",
    content = "agents",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AgentsLine {
    /// `agents --yes <agent>...`: exactly these.
    Named(Vec<Agent>),
    /// `agents remove --yes <agent>...`: none of these, every one the
    /// project names, which is how a line asks for no agent at all, since
    /// naming none is not one. The command refuses it, with its own reason.
    Removed(Vec<Agent>),
}

/// What a project's agents and share toggles say: the agents ticked, its
/// share mode, and whether a file git tracks may be edited to reach an
/// agent. What [`Task::settings`] reads twice, as the project's settings
/// say them and as the toggles do.
///
/// `{"agents": ["claude"], "share": "local", "edit_tracked": false}` in
/// JSON: the names the snapshot gives the same three in a project's
/// settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Toggles {
    /// As its settings name them, in their order; ticked, in any.
    pub agents: Vec<Agent>,
    pub share: Share,
    pub edit_tracked: bool,
}

/// Which level a task runs at: what holds the artifacts it is about.
///
/// `{"type": "project", "entry": "alpha"}` in JSON, or `{"type":
/// "workspace"}`: the shape a task's own variants have, which a variant
/// holding one string cannot take inside the object the tag is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(
    tag = "type",
    content = "entry",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Holder {
    /// The workspace, for its own artifacts: `flayer <cmd>`.
    Workspace,
    /// A project, by its entry: `mind -C <entry> <cmd>`, or `mind <cmd>` for
    /// the one at the root the commands run from.
    Project(String),
    /// A project whose own workspace is not the one on screen, by its entry:
    /// `mind -W . -C <entry> <cmd>`, which names the one on screen, so a
    /// change drops that workspace's record and an add takes its templates.
    ///
    /// Still the project's command rather than `flayer <cmd> -p <entry>`,
    /// which would consult the same workspace: a project's row teaches the
    /// project's command, at every row alike, and `-W` only where leaving
    /// it out would consult another.
    Elsewhere(String),
}

/// Work a front end asked for, as the command it is.
///
/// In JSON, what the panel sends to be run, each variant is an object
/// tagged by its name in snake_case, its fields beside the tag: `{"type":
/// "link", "path": "beta"}`. Tagged inside the object rather than around
/// it, so a task reads as one flat record, the shape TypeScript tells a
/// union apart by. A field a task with fields does not have is refused
/// rather than passed over, as one its holder or its agents line does not
/// have is: a task is a write, and one sent with a field misspelled would
/// run without it. Each name is a contract once a front end sends it, as a
/// report's is, so a test pins every one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Task {
    /// `flayer init`, where the TUI was opened.
    InitWorkspace,
    /// `mind init`, where the TUI was opened.
    InitProject,
    /// `flayer link <path>`.
    Link { path: String },
    /// `mind link`: the project the TUI is on, into the workspace above it.
    LinkHere,
    /// `mind -C <path> init`, then `flayer link <path>`.
    InitAndLink { path: String },
    /// `flayer unlink <entry>`.
    Unlink { entry: String },
    /// `flayer relink <old> <new>`: a stale entry pointed at where the scan
    /// found its project, moved.
    Relink { old: String, new: String },
    /// `flayer load <path>`.
    Load { path: String },
    /// `mind load <path>`: into the workspace above the project the TUI is on.
    LoadHere { path: String },
    /// `flayer unload <entry>`.
    Unload { entry: String },
    /// `mind unload <path>`: out of the workspace above the project the TUI
    /// is on, by its absolute path, which reads the same from anywhere.
    UnloadHere { path: String },
    /// `… add [--template <template>] <kind> <name> <description>`.
    Add {
        holder: Holder,
        kind: Kind,
        name: String,
        description: String,
        template: Option<String>,
    },
    /// `… show <kind:name>`.
    Show { holder: Holder, reference: String },
    /// `… edit <kind:name>`, which hands the terminal to an editor until it is
    /// closed.
    Edit {
        holder: Holder,
        kind: Kind,
        name: String,
    },
    /// `flayer edit [-p <project>] --contents <file> --if-unchanged <hash>
    /// <kind:name>`: what the artifact is read from, a skill's `SKILL.md`
    /// or a rule's file, replaced with `text` whole, only while it holds
    /// what `hash`, the SHA-256 the panel was given with what it read
    /// (`contents::Contents::hash`), is the hash of.
    ///
    /// The text, never a path: the file the line reads is the workspace's
    /// scratch file for it ([`scratch_route`]), named by the line, which
    /// the front end running it writes the text into before the line runs
    /// and takes away after. A front end that has not written it runs a
    /// line that reads nothing, which `edit` says. The workspace's command
    /// at either level, `-p` naming a project, rather than `mind -C`, so
    /// the route is read from the workspace's root, where the lines run and
    /// the scratch folder is.
    Save {
        holder: Holder,
        kind: Kind,
        name: String,
        text: String,
        hash: String,
    },
    /// `… rename <kind:name> <to>`.
    Rename {
        holder: Holder,
        kind: Kind,
        from: String,
        to: String,
    },
    /// `… remove --yes <kind:name>`: asked about before it gets here, which is
    /// what the `--yes` says. `flayer remove --everywhere --yes <kind:name>`
    /// when `everywhere`, one of the workspace's own taken out of every
    /// project it links first. Without `yes`, the dry run, which deletes
    /// nothing and says what would go, each project's copy with how it
    /// stands when `everywhere`: what the panel shows before it asks.
    Remove {
        holder: Holder,
        kind: Kind,
        name: String,
        everywhere: bool,
        yes: bool,
    },
    /// `flayer validate`, everything; `mind validate`, one project.
    Validate { holder: Holder },
    /// `mind -C <path> init` for each repository that is not a project yet,
    /// then one `flayer link` and one `flayer unlink`: what the minds screen's
    /// marks amount to, built from them by [`Task::minds`].
    Minds {
        init: Vec<String>,
        link: Vec<String>,
        unlink: Vec<String>,
    },
    /// What a front end's marks on directories ask for, each by the path a
    /// command is given: the batch [`Task::minds`] builds from them, whose
    /// lines these are, as the minds screen's are.
    ///
    /// How the panel links what a scan found: it sends its marks, never the
    /// batch, so that a directory made a project is then linked, with the
    /// rest, by the one function that says so, and the same marks make the
    /// same lines on either front end: the same words where both spell a
    /// folder alike, and the same batch folder for folder inside the
    /// workspace, where the panel names one by its path, all the path
    /// policy takes, and the minds screen by its route from the root.
    /// [`Minds`](Task::Minds) taken from a
    /// browser was the other way, which would hold the panel to spelling
    /// that rule again in TypeScript, and could leave a folder made a
    /// project and never linked.
    Marks { marks: Vec<Mark> },
    /// `flayer gather list`.
    Shelf,
    /// `flayer workspaces`: the workspaces this machine knows, the one in
    /// use marked, to read.
    Workspaces,
    /// `flayer gather git [--path <folder>] [--ref <ref>] <url>`.
    Gather {
        url: String,
        folder: String,
        reference: Option<String>,
    },
    /// `… targets set --yes <kind> <dir>...`, or `… targets reset --yes
    /// <kind>` when no folder is named: what the targets form amounts to.
    Targets {
        holder: Holder,
        kind: Kind,
        folders: Option<Vec<String>>,
    },
    /// `… agents --yes <agent>...`, then `… share --yes <mode>
    /// [--edit-tracked]`, each only when what it sets changes: what the
    /// agents and share toggles amount to.
    Settings {
        holder: Holder,
        /// The agents to name, when they change.
        agents: Option<AgentsLine>,
        /// The share mode, when it or `edit_tracked` changes.
        share: Option<Share>,
        edit_tracked: bool,
    },
    /// What a project's toggles amount to, turned from `was`, what its
    /// settings say, to `now`, what the toggles say: the
    /// [`Settings`](Task::Settings) task [`Task::settings`] builds, whose
    /// lines these are, as the TUI's toggles' are.
    ///
    /// How the panel changes a project's agents and share mode: it sends
    /// the toggles as they stand, never the lines' agents, so the order a
    /// project names them in, and what unticking every one asks, is said by
    /// the one function, never again in TypeScript. `was` is what the
    /// panel read in the snapshot, as the TUI's is what it read when the
    /// form opened: each line names the agents and the mode outright, so a
    /// `was` the disk has moved on from changes which lines run, never what
    /// a line sets.
    Toggled {
        holder: Holder,
        was: Toggles,
        now: Toggles,
    },
    /// `… update --yes [<kind:name>]`: what is installed in a project, or
    /// the one artifact named, brought up to date and put back where a copy
    /// has gone; `flayer update --all --yes [<kind:name>]` for the
    /// workspace's own row, in every project it links. With `force`, `--yes
    /// --force`, writing over a copy changed since it was installed too,
    /// which the line is then asked about for ([`Caution::Forces`]).
    Update {
        holder: Holder,
        reference: Option<String>,
        force: bool,
    },
    /// `flayer install -p <project>... [--from <origin>] <kind:name>`: one
    /// offer into each project named, by what picks it out
    /// ([`FlayerWorkspace::naming`]), from the place `from` names when it
    /// names one.
    ///
    /// Each of `projects` is what `naming` gives, an entry or a path from
    /// the root, never a project's name, though `-p` takes one: a value
    /// that starts with a dash reaches `-p` behind a `./` (`directory`),
    /// which names the same directory, and so picks the same project by its
    /// path, but is no longer a name `pick` would match. `-p=-odd`, which
    /// would keep it a name, was the alternative, and it would change the
    /// line the TUI has always run and shown for such a project.
    ///
    /// [`FlayerWorkspace::naming`]: mindflayer_core::FlayerWorkspace::naming
    Install {
        projects: Vec<String>,
        kind: Kind,
        name: String,
        from: Option<String>,
    },
    /// `flayer uninstall -p <project>... <kind:name>`: one artifact taken
    /// back out of each project named, whichever place its copy came from,
    /// each named as an install's are, by what `naming` gives. With
    /// `force`, `--force`, taking out a copy changed since it was installed
    /// too, which the line is then asked about for ([`Caution::Forces`]):
    /// never from the install screen, whose marks leave an edit to a command
    /// line, and from the panel only for the copy core says needs it.
    Uninstall {
        projects: Vec<String>,
        kind: Kind,
        name: String,
        force: bool,
    },
    /// `… uninstall --orphaned`: every installation in a project that
    /// nothing offers any more where it came from, taken back out of it;
    /// `flayer uninstall --all-projects --orphaned` for the workspace's own
    /// row, out of every project it links, as an update reaches them. Never
    /// forced: one changed since it was installed is left, every copy of it,
    /// for a command line that says `--force` once somebody has looked.
    UninstallOrphaned { holder: Holder },
    /// `flayer adopt -p <project> --yes <kind:name>`: somebody's skill in
    /// the project's agents' folders brought into the workspace's library,
    /// its copies recorded as installed from there: asked about before it
    /// gets here, which is what the `--yes` says. The project is named as an
    /// install's are, by what `naming` gives.
    Adopt {
        project: String,
        kind: Kind,
        name: String,
    },
    /// `flayer migrate -p <project> [--yes]`: one project the workspace
    /// links moved from an older marker, a `mind.toml` or a
    /// `settings.json`, to a `settings.toml`, what the workspace records of
    /// it carried along; without `yes`, the dry run, which writes nothing
    /// and says what would move. The project is named as an install's are,
    /// by what `naming` gives.
    ///
    /// The workspace's command, as an install's is, so what it carries is
    /// what the workspace on screen records: `mind migrate` in the project
    /// finds a workspace by walking up, which misses one linking the
    /// project from beside it. The agents and the share mode are left to
    /// the command, every agent and shared for a `mind.toml`, as a project
    /// committed by design was, and what a `settings.json` names for one,
    /// for `mind agents` and `mind share` to change once it is moved.
    Migrate { project: String, yes: bool },
    /// Open the install screen, which is what `flayer install` opens.
    OpenInstall,
    /// What the install screen marked, as the `Install` and `Uninstall`
    /// tasks it amounts to ([`Screen::task`]), each run whatever came
    /// of the ones before.
    ///
    /// [`Screen::task`]: crate::install::state::Screen::task
    ApplyInstall { changes: Vec<Task> },
    /// Read the disk again.
    Refresh,
}

/// The task that runs one change core planned for a batch of boxes
/// ([`Plan`](mindflayer_core::install::Plan)): `flayer install` of the
/// offer from where it is, or `flayer uninstall` of the name, from each
/// project the change names, spelled as the boxes named them.
///
/// Here, beside the tasks it becomes, so the install screen and the panel
/// turn a plan into lines one way: core plans with no task of its own,
/// since a task is a front end's.
impl From<Change> for Task {
    fn from(change: Change) -> Task {
        match change {
            Change::Install {
                projects,
                kind,
                name,
                from,
            } => Task::Install {
                projects,
                kind,
                name,
                from: Some(from),
            },
            Change::Uninstall {
                projects,
                kind,
                name,
            } => Task::Uninstall {
                projects,
                kind,
                name,
                force: false,
            },
        }
    }
}

/// What one directory's mark on the minds screen asks of the workspace, when
/// it differs from how the directory started: what [`Task::minds`] is built
/// from. By its name in snake_case in JSON: `"init_and_link"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum MindsChange {
    Link,
    /// `mind init` in it, then link it: a repository that is not a mind
    /// project yet.
    InitAndLink,
    Unlink,
}

impl MindsChange {
    /// What marking a directory a scan found, and the workspace does not
    /// link, asks for: linking it as it is, a mind project, or making it
    /// one first, a git repository that is not one yet. One answer for the
    /// minds screen and for what the panel is told a directory offers.
    pub fn of_found(mind: bool) -> MindsChange {
        if mind {
            MindsChange::Link
        } else {
            MindsChange::InitAndLink
        }
    }
}

/// One directory a front end marked, by the path a command is given, with
/// what to do to it: what [`Task::Marks`] holds, and [`Task::minds`] builds
/// a batch from. `{"path": "/home/you/blog", "change": "link"}` in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub path: String,
    pub change: MindsChange,
}

impl Task {
    /// The task a pass over which minds a workspace manages amounts to, each
    /// directory by the path a command is given with what to do to it, or
    /// none when there is nothing to do.
    ///
    /// Here rather than on the minds screen, so a front end that marks
    /// directories some other way builds the same lines from its marks: that
    /// a directory made a project is then linked, in one `flayer link` with
    /// the rest, is the batch's rule, and a front end spelling it again could
    /// leave one made and never linked. Each list keeps the order the
    /// directories were given in.
    pub fn minds(changes: impl IntoIterator<Item = (String, MindsChange)>) -> Option<Task> {
        let (mut init, mut link, mut unlink) = (Vec::new(), Vec::new(), Vec::new());
        for (path, change) in changes {
            match change {
                MindsChange::Link => link.push(path),
                MindsChange::InitAndLink => {
                    init.push(path.clone());
                    link.push(path);
                }
                MindsChange::Unlink => unlink.push(path),
            }
        }
        (!link.is_empty() || !unlink.is_empty()).then_some(Task::Minds { init, link, unlink })
    }

    /// The task turning a project's toggles from `was`, what its settings
    /// say, to `now` amounts to, for `holder`: an `agents` line when the
    /// agents change, then a `share` line when the share mode or
    /// `edit_tracked` does; none while they say what the settings do.
    ///
    /// Here rather than on the TUI's toggles, where it was born, so every
    /// front end's toggles amount to the same lines: the agents the project
    /// names keep their order, and one ticked is named after them, in
    /// report order, so a toggle does not reorder the settings; and since a
    /// line cannot name no agent, unticking every one is `agents remove` of
    /// every one named, which the command refuses for its own reason, the
    /// rule that a project names an agent being core's.
    pub fn settings(holder: Holder, was: &Toggles, now: &Toggles) -> Option<Task> {
        let ticked = |agent: &Agent| now.agents.contains(agent);
        let mut agents: Vec<Agent> = was.agents.iter().copied().filter(ticked).collect();
        agents.extend(
            Agent::ALL
                .into_iter()
                .filter(|agent| ticked(agent) && !was.agents.contains(agent)),
        );
        agents.dedup();
        let mut named = was.agents.clone();
        named.dedup();
        let agents = match agents {
            _ if agents == named => None,
            none if none.is_empty() => Some(AgentsLine::Removed(named)),
            some => Some(AgentsLine::Named(some)),
        };
        let share =
            ((now.share, now.edit_tracked) != (was.share, was.edit_tracked)).then_some(now.share);
        if agents.is_none() && share.is_none() {
            return None;
        }
        Some(Task::Settings {
            holder,
            agents,
            share,
            edit_tracked: now.edit_tracked,
        })
    }

    /// The command lines this task is, word by word, as a person would type
    /// them from where the commands run: the workspace root, or the project
    /// the TUI was opened on.
    ///
    /// Not a description of the task: the task. The caller parses these with
    /// the real parsers and runs what comes out, so a line here that does not
    /// parse is a task that cannot happen. Opening the install screen is the
    /// exception, a read that builds the screen `flayer install` opens rather
    /// than a line run, and a refresh changes nothing.
    pub fn commands(&self) -> Vec<Vec<String>> {
        match self {
            Task::InitWorkspace => vec![words(&["flayer", "init"], &[])],
            Task::InitProject => vec![words(&["mind", "init"], &[])],
            Task::Link { path } => vec![words(&["flayer", "link"], &[path])],
            Task::LinkHere => vec![words(&["mind", "link"], &[])],
            Task::InitAndLink { path } => vec![
                words(&["mind", "-C", &directory(path), "init"], &[]),
                words(&["flayer", "link"], &[path]),
            ],
            Task::Unlink { entry } => vec![words(&["flayer", "unlink"], &[entry])],
            Task::Relink { old, new } => vec![words(&["flayer", "relink"], &[old, new])],
            Task::Load { path } => vec![words(&["flayer", "load"], &[path])],
            Task::LoadHere { path } => vec![words(&["mind", "load"], &[path])],
            Task::Unload { entry } => vec![words(&["flayer", "unload"], &[entry])],
            Task::UnloadHere { path } => vec![words(&["mind", "unload"], &[path])],
            Task::Add {
                holder,
                kind,
                name,
                description,
                template,
            } => {
                let mut command = vec!["add"];
                let from = template
                    .as_deref()
                    .map(|template| option("--template", template));
                if let Some(from) = &from {
                    command.extend(from.iter().map(String::as_str));
                }
                vec![at(holder, &command, &[kind.slug(), name, description])]
            }
            Task::Show { holder, reference } => vec![at(holder, &["show"], &[reference])],
            Task::Edit { holder, kind, name } => {
                vec![at(holder, &["edit"], &[&qualified(*kind, name)])]
            }
            Task::Save {
                holder,
                kind,
                name,
                text,
                hash,
            } => {
                let mut head = match holder {
                    Holder::Workspace => owned(&["flayer", "edit"]),
                    Holder::Project(entry) | Holder::Elsewhere(entry) => {
                        picking(&["flayer", "edit"], std::slice::from_ref(entry))
                    }
                };
                head.extend(option("--contents", &scratch_route(text)));
                head.extend(option("--if-unchanged", hash));
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[&qualified(*kind, name)])]
            }
            Task::Rename {
                holder,
                kind,
                from,
                to,
            } => vec![at(holder, &["rename"], &[&qualified(*kind, from), to])],
            Task::Remove {
                holder,
                kind,
                name,
                everywhere,
                yes,
            } => {
                let mut command = vec!["remove"];
                if *everywhere {
                    command.push("--everywhere");
                }
                if *yes {
                    command.push("--yes");
                }
                vec![at(holder, &command, &[&qualified(*kind, name)])]
            }
            Task::Validate { holder } => vec![at(holder, &["validate"], &[])],
            Task::Minds { init, link, unlink } => {
                let mut lines: Vec<Vec<String>> = init
                    .iter()
                    .map(|path| words(&["mind", "-C", &directory(path), "init"], &[]))
                    .collect();
                for (command, paths) in [("link", link), ("unlink", unlink)] {
                    if !paths.is_empty() {
                        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
                        lines.push(words(&["flayer", command], &paths));
                    }
                }
                lines
            }
            Task::Marks { marks } => {
                Task::minds(marks.iter().map(|mark| (mark.path.clone(), mark.change)))
                    .map_or_else(Vec::new, |batch| batch.commands())
            }
            Task::Shelf => vec![words(&["flayer", "gather", "list"], &[])],
            Task::Workspaces => vec![words(&["flayer", "workspaces"], &[])],
            Task::Gather {
                url,
                folder,
                reference,
            } => {
                let mut head = vec!["flayer", "gather", "git"];
                let path = option("--path", folder);
                let at = reference
                    .as_deref()
                    .map(|reference| option("--ref", reference));
                // The default folder is left unsaid, so the line reads the
                // way somebody would type it.
                if folder != DEFAULT_SUBDIRECTORY {
                    head.extend(path.iter().map(String::as_str));
                }
                if let Some(at) = &at {
                    head.extend(at.iter().map(String::as_str));
                }
                vec![words(&head, &[url])]
            }
            // The agents first, so a project turning local finds the copies
            // they bring already kept out of git, and one turning shared
            // finds them in the group it lets go of; either order ends the
            // same.
            Task::Settings {
                holder,
                agents,
                share,
                edit_tracked,
            } => {
                let mut lines = Vec::new();
                let words = |agents: &[Agent]| -> Vec<&'static str> {
                    agents.iter().map(|agent| agent.name()).collect()
                };
                match agents {
                    Some(AgentsLine::Named(agents)) => {
                        lines.push(at(holder, &["agents", "--yes"], &words(agents)));
                    }
                    Some(AgentsLine::Removed(agents)) => {
                        lines.push(at(holder, &["agents", "remove", "--yes"], &words(agents)));
                    }
                    None => {}
                }
                if let Some(share) = share {
                    let mut command = vec!["share", "--yes"];
                    if *edit_tracked {
                        command.push("--edit-tracked");
                    }
                    lines.push(at(holder, &command, &[share.name()]));
                }
                lines
            }
            Task::Toggled { holder, was, now } => Task::settings(holder.clone(), was, now)
                .map_or_else(Vec::new, |settings| settings.commands()),
            Task::Targets {
                holder,
                kind,
                folders,
            } => match folders {
                Some(folders) => {
                    let mut positionals = vec![kind.folder()];
                    positionals.extend(folders.iter().map(String::as_str));
                    vec![at(holder, &["targets", "set", "--yes"], &positionals)]
                }
                None => vec![at(holder, &["targets", "reset", "--yes"], &[kind.folder()])],
            },
            // Forced only when asked: an edit is somebody's work, so the
            // TUI never forces, leaving it to a command line that says
            // `--force` as the install screen does, and the panel asks first,
            // since the line says what it writes over.
            Task::Update {
                holder,
                reference,
                force,
            } => {
                let named: Vec<&str> = reference.iter().map(String::as_str).collect();
                let forced: &[&str] = if *force { &["--force"] } else { &[] };
                match holder {
                    Holder::Workspace => {
                        let head = [&["flayer", "update", "--all", "--yes"], forced].concat();
                        vec![words(&head, &named)]
                    }
                    _ => vec![at(holder, &[&["update", "--yes"], forced].concat(), &named)],
                }
            }
            Task::Install {
                projects,
                kind,
                name,
                from,
            } => {
                let mut head = picking(&["flayer", "install"], projects);
                if let Some(from) = from {
                    head.extend(option("--from", from));
                }
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[&qualified(*kind, name)])]
            }
            Task::Uninstall {
                projects,
                kind,
                name,
                force,
            } => {
                let mut head = picking(&["flayer", "uninstall"], projects);
                if *force {
                    head.push(String::from("--force"));
                }
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[&qualified(*kind, name)])]
            }
            Task::UninstallOrphaned { holder } => match holder {
                Holder::Workspace => vec![words(
                    &["flayer", "uninstall", "--all-projects", "--orphaned"],
                    &[],
                )],
                _ => vec![at(holder, &["uninstall", "--orphaned"], &[])],
            },
            // `--yes` before the name, as `remove` has it, so a name a
            // `--` has to keep from being read as a flag leaves it a flag.
            Task::Adopt {
                project,
                kind,
                name,
            } => {
                let mut head = picking(&["flayer", "adopt"], std::slice::from_ref(project));
                head.push(String::from("--yes"));
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[&qualified(*kind, name)])]
            }
            Task::Migrate { project, yes } => {
                let mut head = picking(&["flayer", "migrate"], std::slice::from_ref(project));
                if *yes {
                    head.push(String::from("--yes"));
                }
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[])]
            }
            Task::ApplyInstall { changes } => changes.iter().flat_map(Task::commands).collect(),
            Task::OpenInstall => vec![words(&["flayer", "install"], &[])],
            Task::Refresh => Vec::new(),
        }
    }

    /// Whether this changes something on disk, and so belongs in the
    /// transcript printed when the TUI closes.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Task::InitWorkspace
                | Task::InitProject
                | Task::Link { .. }
                | Task::LinkHere
                | Task::InitAndLink { .. }
                | Task::Unlink { .. }
                | Task::Relink { .. }
                | Task::Load { .. }
                | Task::LoadHere { .. }
                | Task::Unload { .. }
                | Task::UnloadHere { .. }
                | Task::Add { .. }
                | Task::Edit { .. }
                | Task::Save { .. }
                | Task::Rename { .. }
                | Task::Remove { yes: true, .. }
                | Task::Gather { .. }
                | Task::Minds { .. }
                | Task::Marks { .. }
                | Task::Settings { .. }
                | Task::Toggled { .. }
                | Task::Targets { .. }
                | Task::Update { .. }
                | Task::Install { .. }
                | Task::Uninstall { .. }
                | Task::UninstallOrphaned { .. }
                | Task::Adopt { .. }
                | Task::Migrate { yes: true, .. }
                | Task::ApplyInstall { .. }
        )
    }

    /// Whether what is on screen has to be read again afterwards.
    pub fn reloads(&self) -> bool {
        self.writes() || *self == Task::Refresh
    }

    /// Whether it needs the terminal to itself while it runs: an editor draws
    /// its own screen, and this one has to step aside for it.
    pub fn takes_terminal(&self) -> bool {
        matches!(self, Task::Edit { .. })
    }

    /// Whether each of its lines leans on the one before, so that one failing
    /// is reason not to run the rest: `flayer link` after a failed `mind
    /// init` would only fail again, less clearly. True of every task but the
    /// install screen's batch, whose lines are boxes marked one by one, each
    /// carried out or said whatever came of the others, as the screen did
    /// before they were lines.
    pub fn chained(&self) -> bool {
        !matches!(self, Task::ApplyInstall { .. })
    }

    /// Whether it runs on a thread of its own while the screen goes on
    /// drawing what it reports: a gather, which is the network, and can take
    /// long enough to want watching. Everything else is the local disk, over
    /// before anybody could read a count.
    pub fn runs_apart(&self) -> bool {
        matches!(self, Task::Gather { .. })
    }

    /// What to say while it runs.
    pub fn describe(&self) -> String {
        match self {
            Task::Refresh => String::from("reading the disk again"),
            Task::OpenInstall => String::from("opening the install screen"),
            Task::ApplyInstall { .. } => String::from("installing and removing what was marked"),
            _ => self
                .commands()
                .iter()
                .map(|line| shell(line))
                .collect::<Vec<_>>()
                .join(" && "),
        }
    }

    /// Each of its lines a front end asks somebody to confirm before it
    /// runs them, by its place among [`commands`](Self::commands), with what
    /// it takes away ([`cautioned`]).
    ///
    /// Asked of the lines, as [`refusal`](Self::refusal) is, so the one list
    /// decides whatever task a line came from: the minds screen's batch is
    /// asked about for its `flayer unlink` line and for nothing else.
    pub fn cautions(&self) -> Vec<(usize, Caution)> {
        cautioned(&self.commands())
    }

    /// Why a front end with no terminal of its own does not run this task,
    /// when it does not: the refusal of the first of its lines that is
    /// refused ([`refused`]).
    ///
    /// Asked of the lines rather than of the variant, so the one list of
    /// commands decides, whatever task a line came from.
    pub fn refusal(&self) -> Option<Refusal> {
        self.refused_line().map(|(_, refusal)| refusal)
    }

    /// [`refusal`](Self::refusal), with the line it is the refusal of: for
    /// a front end that says which command it will not run.
    pub fn refused_line(&self) -> Option<(Vec<String>, Refusal)> {
        self.commands()
            .into_iter()
            .find_map(|line| refused(&line).map(|refusal| (line, refusal)))
    }
}

/// Why a front end with no terminal of its own, the panel's server first
/// among them, does not run a command line: what [`Task::refusal`] and
/// [`refused`] answer, from one list every front end shares.
///
/// One list, of commands as the parsers read them rather than of tasks, so
/// a line refused for one front end is refused for every one, whichever
/// task it came from or none. The TUI has a terminal, and refuses only the
/// lines that would draw a screen inside its own ([`Refusal::Screen`]),
/// hold it until a Ctrl-C it keeps for itself ([`Refusal::Panel`]), or read
/// the keys it reads ([`Refusal::Input`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Refusal {
    /// The TUI (`mind` or `flayer` alone, or either's `tui`), the minds
    /// screen (`flayer minds`), or the install screen (`flayer install`
    /// with neither a name nor a project).
    #[error("opens a screen of its own, which needs a terminal to draw on")]
    Screen,
    /// `edit` in an editor: it hands the terminal to one until it is
    /// closed. `edit --contents` with a file opens none, and runs.
    #[error("opens an editor, which needs a terminal to draw on")]
    Editor,
    /// `edit --contents -`, which reads what to write from standard input:
    /// the terminal the front end was started from, or nothing, where a
    /// line it runs would wait for an end nobody types. A front end hands
    /// it a file instead.
    #[error("reads standard input, which only a shell can hand it")]
    Input,
    /// `flayer activate`, which prints code for a shell to run, and means
    /// nothing anywhere else.
    #[error("prints code for a shell to run, which only a shell can")]
    Shell,
    /// `flayer gather git`: the network, for as long as it takes, which only
    /// a screen that shows how far it got and stops it on a key can watch,
    /// until #112 brings both to the server.
    #[error(
        "fetches over the network for as long as that takes, which only the TUI or a shell shows, and stops"
    )]
    Unwatched,
    /// `show`, which prints an artifact whole, read through every link on
    /// the way (a rule shared by a link is still a rule): at a terminal, to
    /// the person whose disk it is, but anywhere else to somebody who may
    /// not be. A front end without one reads one file at a time instead,
    /// held to its holder and a cap (`contents::Contents`), so that whether
    /// a file may leave is decided in one place.
    #[error(
        "prints an artifact whole, read through any link on the way, where a front end without a terminal reads one file at a time, held to its holder"
    )]
    Unheld,
    /// `ui`, from either binary: it serves the panel until Ctrl-C stops it,
    /// which would hold a front end that ran it as one of its lines for good,
    /// the panel's own writer or the TUI, whose screen keeps Ctrl-C for
    /// itself.
    #[error("serves the panel until Ctrl-C stops it, from the terminal it was started in")]
    Panel,
}

codes!(Refusal {
    Self::Screen => "opens_screen",
    Self::Editor => "opens_editor",
    Self::Input => "reads_stdin",
    Self::Shell => "prints_shell_code",
    Self::Unwatched => "gather_unwatched",
    Self::Unheld => "show_unheld",
    Self::Panel => "serves_panel",
});

impl Refusal {
    /// Why `mind <command>` is not run by a front end with no terminal, when
    /// it is not.
    ///
    /// One arm per command with no wildcard, as `of_flayer`'s, so a new
    /// command does not compile until it says whether a front end without a
    /// terminal runs it.
    pub fn of_mind(command: Option<&Command>) -> Option<Refusal> {
        match command {
            None | Some(Command::Tui) => Some(Refusal::Screen),
            Some(Command::Edit { editing, .. }) => Refusal::of_edit(editing),
            Some(Command::Show { .. }) => Some(Refusal::Unheld),
            #[cfg(feature = "ui")]
            Some(Command::Ui(_)) => Some(Refusal::Panel),
            Some(Command::Flayer { command }) => Refusal::of_flayer(command.as_ref()),
            Some(
                Command::Init { .. }
                | Command::Add { .. }
                | Command::Rename { .. }
                | Command::Remove { .. }
                | Command::Templates { .. }
                | Command::List { .. }
                | Command::Validate { .. }
                | Command::Status { .. }
                | Command::Link
                | Command::Unlink
                | Command::Load { .. }
                | Command::Unload { .. }
                | Command::Install { .. }
                | Command::Uninstall { .. }
                | Command::Update { .. }
                | Command::Agents { .. }
                | Command::Share { .. }
                | Command::Targets { .. }
                | Command::Migrate { .. },
            ) => None,
        }
    }

    /// Why `flayer <command>` is not run by a front end with no terminal,
    /// when it is not.
    pub fn of_flayer(command: Option<&FlayerCommand>) -> Option<Refusal> {
        match command {
            None | Some(FlayerCommand::Tui | FlayerCommand::Minds { .. }) => Some(Refusal::Screen),
            // The screen only when it is what the line opens, as the command
            // decides: a project with nothing named is half an install,
            // which the command refuses with a reason and a code of its own
            // (`install_nothing_named`), and a screen it would not open is
            // no reason to give in their place.
            Some(FlayerCommand::Install {
                name: None,
                projects,
                all_projects: false,
                ..
            }) if projects.is_empty() => Some(Refusal::Screen),
            Some(FlayerCommand::Edit { editing, .. }) => Refusal::of_edit(editing),
            Some(FlayerCommand::Activate { .. }) => Some(Refusal::Shell),
            Some(FlayerCommand::Show { .. }) => Some(Refusal::Unheld),
            Some(FlayerCommand::Gather(GatherCommand::Git { .. })) => Some(Refusal::Unwatched),
            #[cfg(feature = "ui")]
            Some(FlayerCommand::Ui(_)) => Some(Refusal::Panel),
            Some(
                FlayerCommand::Init { .. }
                | FlayerCommand::Which
                | FlayerCommand::Workspaces { .. }
                | FlayerCommand::Add { .. }
                | FlayerCommand::Rename { .. }
                | FlayerCommand::Remove { .. }
                | FlayerCommand::Templates { .. }
                | FlayerCommand::List { .. }
                | FlayerCommand::Validate { .. }
                | FlayerCommand::Status { .. }
                | FlayerCommand::Link { .. }
                | FlayerCommand::Unlink { .. }
                | FlayerCommand::Relink { .. }
                | FlayerCommand::Load { .. }
                | FlayerCommand::Unload { .. }
                | FlayerCommand::Scan { .. }
                | FlayerCommand::Gather(GatherCommand::List { .. })
                | FlayerCommand::Install { .. }
                | FlayerCommand::Uninstall { .. }
                | FlayerCommand::Update { .. }
                | FlayerCommand::Adopt { .. }
                | FlayerCommand::Agents { .. }
                | FlayerCommand::Share { .. }
                | FlayerCommand::Targets { .. }
                | FlayerCommand::Migrate { .. },
            ) => None,
        }
    }

    /// Why an `edit` at either level is not run by a front end with no
    /// terminal, by how it takes what the artifact is to hold: an editor
    /// or standard input, each the terminal's, are refused, and a file is
    /// run, its contents held to core's rules as any edit's are.
    fn of_edit(editing: &Editing) -> Option<Refusal> {
        match editing.given() {
            None => Some(Refusal::Editor),
            Some(Given::Input) => Some(Refusal::Input),
            Some(Given::File(_)) => None,
        }
    }
}

/// Why a front end with no terminal of its own does not run `words`, a
/// command line, when it does not.
///
/// Read with the parsers that run it, so the list is of commands as they
/// run, not as they are spelled: `mind flayer minds` is `flayer minds`, and
/// a flag in front of either changes nothing. A line neither takes is no
/// refusal of this list's: running it says why it cannot run, in the
/// parser's words.
pub fn refused(words: &[String]) -> Option<Refusal> {
    match words.first().map(String::as_str) {
        Some("mind") => Cli::try_parse_from(words)
            .ok()
            .and_then(|cli| Refusal::of_mind(cli.command.as_ref())),
        Some("flayer") => FlayerCli::try_parse_from(words)
            .ok()
            .and_then(|cli| Refusal::of_flayer(cli.command.as_ref())),
        _ => None,
    }
}

/// What a command line takes away, which a front end asks somebody to
/// confirm before it runs the line: what [`cautions`] reads off the parsed
/// command, as [`refused`] reads a refusal, so a line is asked about the
/// same way whatever task it came from, and by every front end.
///
/// One list, here, rather than one per front end: the panel holds no rules
/// (D-18), so which of its lines need a yes first is said with each preview
/// it is answered with, in these words, and a list in TypeScript would be a
/// second place for a command to be forgotten. The TUI asks its own
/// questions, worded for the row they are about, before it builds a task.
///
/// Only what takes something away, or writes over somebody's work: a line
/// that only adds, or that changes what is installed where its agents now
/// read, is shown before it runs and runs once asked. And never a dry run,
/// which a line of a command that takes `--yes` is without it: it takes
/// nothing away, and says what it would.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Caution {
    /// `remove --yes`, at either level: the artifact deleted.
    #[error("deletes an artifact: its folder and everything in it, or its file")]
    Removes,
    /// `uninstall`, every form of it, and `flayer remove --everywhere
    /// --yes`, which uninstalls from every project first.
    #[error("takes copies Mindflayer installed out of projects")]
    Uninstalls,
    /// `unlink`, at either level.
    #[error("makes the workspace forget a project, though nothing on disk is deleted")]
    Unlinks,
    /// `unload`, at either level: `unlink`'s twin, for a loaded workspace,
    /// whose copies in the projects are left orphaned.
    #[error(
        "makes the workspace stop offering a loaded workspace's own, though nothing on disk is deleted"
    )]
    Unloads,
    /// `--force`: on `install` and `uninstall`, and on `update` that is
    /// told `--yes`.
    #[error("writes over, or takes out, a copy changed since it was installed")]
    Forces,
}

impl Caution {
    /// What `mind <command>` takes away, in the order of [`Caution`]'s
    /// variants; nothing for a line that takes nothing.
    ///
    /// One arm per command with no wildcard, as [`Refusal::of_mind`]'s, so
    /// a new command does not compile until it says what it takes away.
    pub fn of_mind(command: Option<&Command>) -> Vec<Caution> {
        match command {
            Some(Command::Remove { yes, .. }) => when(*yes, &[Caution::Removes]),
            Some(Command::Install { force, .. }) => when(*force, &[Caution::Forces]),
            Some(Command::Uninstall { force, .. }) => uninstalling(*force),
            Some(Command::Update { force, yes, .. }) => when(*force && *yes, &[Caution::Forces]),
            Some(Command::Unlink) => vec![Caution::Unlinks],
            Some(Command::Unload { .. }) => vec![Caution::Unloads],
            Some(Command::Flayer { command }) => Caution::of_flayer(command.as_ref()),
            #[cfg(feature = "ui")]
            Some(Command::Ui(_)) => Vec::new(),
            None
            | Some(
                Command::Tui
                | Command::Init { .. }
                | Command::Add { .. }
                | Command::Edit { .. }
                | Command::Rename { .. }
                | Command::Templates { .. }
                | Command::List { .. }
                | Command::Show { .. }
                | Command::Validate { .. }
                | Command::Status { .. }
                | Command::Link
                | Command::Load { .. }
                | Command::Agents { .. }
                | Command::Share { .. }
                | Command::Targets { .. }
                | Command::Migrate { .. },
            ) => Vec::new(),
        }
    }

    /// What `flayer <command>` takes away.
    pub fn of_flayer(command: Option<&FlayerCommand>) -> Vec<Caution> {
        match command {
            Some(FlayerCommand::Remove {
                yes, everywhere, ..
            }) => match (*yes, *everywhere) {
                (false, _) => Vec::new(),
                (true, false) => vec![Caution::Removes],
                (true, true) => vec![Caution::Removes, Caution::Uninstalls],
            },
            Some(FlayerCommand::Install { force, .. }) => when(*force, &[Caution::Forces]),
            Some(FlayerCommand::Uninstall { force, .. }) => uninstalling(*force),
            Some(FlayerCommand::Update { force, yes, .. }) => {
                when(*force && *yes, &[Caution::Forces])
            }
            Some(FlayerCommand::Unlink { .. }) => vec![Caution::Unlinks],
            Some(FlayerCommand::Unload { .. }) => vec![Caution::Unloads],
            #[cfg(feature = "ui")]
            Some(FlayerCommand::Ui(_)) => Vec::new(),
            None
            | Some(
                FlayerCommand::Tui
                | FlayerCommand::Init { .. }
                | FlayerCommand::Activate { .. }
                | FlayerCommand::Which
                | FlayerCommand::Workspaces { .. }
                | FlayerCommand::Add { .. }
                | FlayerCommand::Edit { .. }
                | FlayerCommand::Rename { .. }
                | FlayerCommand::Templates { .. }
                | FlayerCommand::List { .. }
                | FlayerCommand::Show { .. }
                | FlayerCommand::Validate { .. }
                | FlayerCommand::Status { .. }
                | FlayerCommand::Link { .. }
                | FlayerCommand::Relink { .. }
                | FlayerCommand::Load { .. }
                | FlayerCommand::Minds { .. }
                | FlayerCommand::Scan { .. }
                | FlayerCommand::Gather(_)
                | FlayerCommand::Adopt { .. }
                | FlayerCommand::Agents { .. }
                | FlayerCommand::Share { .. }
                | FlayerCommand::Targets { .. }
                | FlayerCommand::Migrate { .. },
            ) => Vec::new(),
        }
    }
}

/// `cautions` when `asked`, and none otherwise.
fn when(asked: bool, cautions: &[Caution]) -> Vec<Caution> {
    if asked {
        cautions.to_vec()
    } else {
        Vec::new()
    }
}

/// What an uninstall takes away: copies, and with `--force` an edited one
/// among them, which it otherwise leaves with every other copy of it.
fn uninstalling(force: bool) -> Vec<Caution> {
    let mut cautions = vec![Caution::Uninstalls];
    if force {
        cautions.push(Caution::Forces);
    }
    cautions
}

/// What `words`, a command line, takes away, which a front end asks about
/// before it runs it; nothing for a line that takes nothing away.
///
/// Read with the parsers that run it, as [`refused`] is: `mind flayer
/// unlink alpha` is `flayer unlink alpha`, and a flag in front of either
/// changes nothing. A line neither takes would not run, and takes nothing.
pub fn cautions(words: &[String]) -> Vec<Caution> {
    match words.first().map(String::as_str) {
        Some("mind") => Cli::try_parse_from(words)
            .map(|cli| Caution::of_mind(cli.command.as_ref()))
            .unwrap_or_default(),
        Some("flayer") => FlayerCli::try_parse_from(words)
            .map(|cli| Caution::of_flayer(cli.command.as_ref()))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// What each of `lines` takes away, each line by its place among them,
/// from 0: what a front end asks somebody to confirm before it runs them.
pub fn cautioned(lines: &[Vec<String>]) -> Vec<(usize, Caution)> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(at, line)| cautions(line).into_iter().map(move |caution| (at, caution)))
        .collect()
}

/// One command line, at the level a holder is at: `flayer <command>` for the
/// workspace's own, `mind -C <entry> <command>` for a project, and plain
/// `mind <command>` for the one at the root, where `-C .` says nothing.
///
/// A `-W .` is typed from the root, as the `-C` is, so it names the
/// workspace on screen.
fn at(holder: &Holder, command: &[&str], positionals: &[&str]) -> Vec<String> {
    let start: Vec<String> = match holder {
        Holder::Workspace => vec![String::from("flayer")],
        Holder::Project(entry) if entry == "." => vec![String::from("mind")],
        Holder::Project(entry) => vec![String::from("mind"), String::from("-C"), directory(entry)],
        Holder::Elsewhere(entry) => ["mind", "-W", ".", "-C"]
            .into_iter()
            .map(String::from)
            .chain([directory(entry)])
            .collect(),
    };
    let mut head: Vec<&str> = start.iter().map(String::as_str).collect();
    head.extend_from_slice(command);
    words(&head, positionals)
}

/// Where a save's `text` is put for its line to read, as a route from the
/// workspace's root, `/` separated: a file in the workspace's scratch
/// folder (`FlayerWorkspace::scratch_dir`) named by the first sixteen hex
/// digits of the SHA-256 of what it holds.
///
/// By what it holds rather than by a name drawn for the run, so the line a
/// preview shows is the line that runs, a preview and a run being two
/// requests; and no two saves of different text are named alike, while two
/// of the same text hold the same bytes. A file an interrupted run left
/// under the name is the same text, which a front end may take as it is.
/// In `.mindflayer`, where the line runs from and no agent looks, rather
/// than in the system's temporary folder, where another user could read
/// what is about to be written.
pub fn scratch_route(text: &str) -> String {
    let named = &hash::bytes(text.as_bytes())[..16];
    format!("{FLAYER_DIR}/{SCRATCH_DIR}/edit-{named}.md")
}

/// `words` as owned words.
fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// `head`, then a `-p` for each of `projects`, each kept from being read as a
/// flag as a `-C` is.
fn picking(head: &[&str], projects: &[String]) -> Vec<String> {
    let mut line: Vec<String> = head.iter().map(|word| (*word).to_owned()).collect();
    for project in projects {
        line.push(String::from("-p"));
        line.push(directory(project));
    }
    line
}

/// `skill:deploy`: a name that says which kind it names.
pub(crate) fn qualified(kind: Kind, name: &str) -> String {
    format!("{}{QUALIFIER}{name}", kind.slug())
}

/// A command line's words: `head`, then the positionals, behind a `--` when
/// one of them would otherwise be read as a flag; a description is free
/// text, and `- the fast way` is a fine one.
///
/// Every positional goes through here, which is what keeps one from being
/// read as an option: `head` holds the command's own words and options
/// already kept apart, by [`option`] or [`directory`].
fn words(head: &[&str], positionals: &[&str]) -> Vec<String> {
    let mut line: Vec<String> = head.iter().map(|word| (*word).to_owned()).collect();
    if positionals.iter().any(|word| word.starts_with('-')) {
        line.push(String::from("--"));
    }
    line.extend(positionals.iter().map(|word| (*word).to_owned()));
    line
}

/// A flag and its value, joined by `=` when the value starts with a dash,
/// so the parser takes it as this flag's value rather than as a flag of its
/// own.
fn option(flag: &str, value: &str) -> Vec<String> {
    if value.starts_with('-') {
        vec![format!("{flag}={value}")]
    } else {
        vec![flag.to_owned(), value.to_owned()]
    }
}

/// A `-C` or `-p` value, kept from being read as a flag: behind a `./` when
/// it starts with a dash, which names the same directory from where the
/// lines run, and a project by that path.
fn directory(path: &str) -> String {
    if path.starts_with('-') {
        format!("./{path}")
    } else {
        path.to_owned()
    }
}

/// A command line as it would be typed into a shell, for showing.
///
/// Without the credentials of any URL in it, here rather than where it is
/// shown, because it is shown in more than one place: the box that says what
/// is running, and the transcript printed when the TUI closes. The words run
/// as typed, so a URL with a token in it is refused rather than fetched
/// without one; only the line that names them leaves it out.
pub fn shell(words: &[String]) -> String {
    let line = words
        .iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    redacted(&line).into_owned()
}

/// A word, quoted only when a shell would otherwise split or expand it.
fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
#[path = "task_test.rs"]
mod tests;
