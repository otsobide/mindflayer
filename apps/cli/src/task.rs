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
use mindflayer_core::url::redacted;
use mindflayer_core::{codes, Agent, Kind, Share, DEFAULT_SUBDIRECTORY, QUALIFIER};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Cli, Command, FlayerCli, FlayerCommand, GatherCommand};

/// What an `agents` line says, as the toggles have it.
///
/// `{"type": "named", "agents": ["claude"]}` in JSON: tagged as a task is,
/// with the agents beside the tag, since a variant holding a list has no
/// field of its own for them to be named by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Which level a task runs at: what holds the artifacts it is about.
///
/// `{"type": "project", "entry": "alpha"}` in JSON, or `{"type":
/// "workspace"}`: the shape a task's own variants have, which a variant
/// holding one string cannot take inside the object the tag is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// project it links first.
    Remove {
        holder: Holder,
        kind: Kind,
        name: String,
        everywhere: bool,
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
    /// `… update --yes [<kind:name>]`: what is installed in a project, or
    /// the one artifact named, brought up to date and put back where a copy
    /// has gone; `flayer update --all --yes [<kind:name>]` for the
    /// workspace's own row, in every project it links.
    Update {
        holder: Holder,
        reference: Option<String>,
    },
    /// `flayer install -p <project>... [--from <origin>] <kind:name>`: one
    /// offer into each project named, by what picks it out
    /// ([`FlayerWorkspace::naming`]), from the place `from` names when it
    /// names one.
    ///
    /// [`FlayerWorkspace::naming`]: mindflayer_core::FlayerWorkspace::naming
    Install {
        projects: Vec<String>,
        kind: Kind,
        name: String,
        from: Option<String>,
    },
    /// `flayer uninstall -p <project>... <kind:name>`: one artifact taken
    /// back out of each project named, whichever place its copy came from.
    Uninstall {
        projects: Vec<String>,
        kind: Kind,
        name: String,
    },
    /// `flayer adopt -p <project> --yes <kind:name>`: somebody's skill in
    /// the project's agents' folders brought into the workspace's library,
    /// its copies recorded as installed from there: asked about before it
    /// gets here, which is what the `--yes` says.
    Adopt {
        project: String,
        kind: Kind,
        name: String,
    },
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

/// What one directory's mark on the minds screen asks of the workspace, when
/// it differs from how the directory started: what [`Task::minds`] is built
/// from. By its name in snake_case in JSON: `"init_and_link"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MindsChange {
    Link,
    /// `mind init` in it, then link it: a repository that is not a mind
    /// project yet.
    InitAndLink,
    Unlink,
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
            } => {
                let command: &[&str] = if *everywhere {
                    &["remove", "--everywhere", "--yes"]
                } else {
                    &["remove", "--yes"]
                };
                vec![at(holder, command, &[&qualified(*kind, name)])]
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
            // Never forced: an edit is somebody's work, and the screen
            // leaves it to a command line that says `--force`, as the install
            // screen does.
            Task::Update { holder, reference } => {
                let named: Vec<&str> = reference.iter().map(String::as_str).collect();
                match holder {
                    Holder::Workspace => {
                        vec![words(&["flayer", "update", "--all", "--yes"], &named)]
                    }
                    _ => vec![at(holder, &["update", "--yes"], &named)],
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
            } => {
                let head = picking(&["flayer", "uninstall"], projects);
                let head: Vec<&str> = head.iter().map(String::as_str).collect();
                vec![words(&head, &[&qualified(*kind, name)])]
            }
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
                | Task::Rename { .. }
                | Task::Remove { .. }
                | Task::Gather { .. }
                | Task::Minds { .. }
                | Task::Settings { .. }
                | Task::Targets { .. }
                | Task::Update { .. }
                | Task::Install { .. }
                | Task::Uninstall { .. }
                | Task::Adopt { .. }
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

    /// Why a front end with no terminal of its own does not run this task,
    /// when it does not: the refusal of the first of its lines that is
    /// refused ([`refused`]).
    ///
    /// Asked of the lines rather than of the variant, so the one list of
    /// commands decides, whatever task a line came from.
    pub fn refusal(&self) -> Option<Refusal> {
        self.commands().iter().find_map(|line| refused(line))
    }
}

/// Why a front end with no terminal of its own, the panel's server first
/// among them, does not run a command line: what [`Task::refusal`] and
/// [`refused`] answer, from one list every front end shares.
///
/// One list, of commands as the parsers read them rather than of tasks, so
/// a line refused for one front end is refused for every one, whichever
/// task it came from or none. The TUI has a terminal, and refuses only the
/// lines that would draw a screen inside its own ([`Refusal::Screen`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Refusal {
    /// The TUI (`mind` or `flayer` alone, or either's `tui`), the minds
    /// screen (`flayer minds`), or the install screen (`flayer install`
    /// with neither a name nor a project).
    #[error("opens a screen of its own, which needs a terminal to draw on")]
    Screen,
    /// `edit`, every form of it: it hands the terminal to an editor until
    /// the editor is closed. Until #81 gives it a form that takes the
    /// contents instead.
    #[error("opens an editor, which needs a terminal to draw on")]
    Editor,
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
}

codes!(Refusal {
    Self::Screen => "opens_screen",
    Self::Editor => "opens_editor",
    Self::Shell => "prints_shell_code",
    Self::Unwatched => "gather_unwatched",
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
            Some(Command::Edit { .. }) => Some(Refusal::Editor),
            Some(Command::Flayer { command }) => Refusal::of_flayer(command.as_ref()),
            Some(
                Command::Init { .. }
                | Command::Add { .. }
                | Command::Rename { .. }
                | Command::Remove { .. }
                | Command::Templates
                | Command::List { .. }
                | Command::Show { .. }
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
            Some(FlayerCommand::Edit { .. }) => Some(Refusal::Editor),
            Some(FlayerCommand::Activate { .. }) => Some(Refusal::Shell),
            Some(FlayerCommand::Gather(GatherCommand::Git { .. })) => Some(Refusal::Unwatched),
            Some(
                FlayerCommand::Init { .. }
                | FlayerCommand::Which
                | FlayerCommand::Workspaces { .. }
                | FlayerCommand::Add { .. }
                | FlayerCommand::Rename { .. }
                | FlayerCommand::Remove { .. }
                | FlayerCommand::Templates { .. }
                | FlayerCommand::List { .. }
                | FlayerCommand::Show { .. }
                | FlayerCommand::Validate { .. }
                | FlayerCommand::Status { .. }
                | FlayerCommand::Link { .. }
                | FlayerCommand::Unlink { .. }
                | FlayerCommand::Relink { .. }
                | FlayerCommand::Load { .. }
                | FlayerCommand::Unload { .. }
                | FlayerCommand::Scan { .. }
                | FlayerCommand::Gather(GatherCommand::List)
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
