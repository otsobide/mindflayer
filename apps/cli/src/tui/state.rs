//! What the TUI knows, and what a key does to it.
//!
//! Kept apart from the terminal and the drawing, as the install screen it
//! embeds is, so a test drives it the way a person does. Nothing here touches
//! a file. A key that asks for work returns a [`Task`], and a task is a
//! command line: [`Task::commands`] is exactly what the caller parses, with
//! the parsers `mind` and `flayer` use, and runs. There is nothing this screen
//! can do that a command cannot.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mindflayer_core::{paths, Kind, Layout, DEFAULT_SUBDIRECTORY, QUALIFIER};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::minds::{self, Minds};
use crate::install::state::{Screen as InstallScreen, Step as InstallStep};

/// How far a page key scrolls a page of output.
const PAGE: usize = 10;

// ---------------------------------------------------------------------------
// What is on screen
// ---------------------------------------------------------------------------

/// What the TUI was opened on.
///
/// It decides what the TUI offers to make when there is nothing yet, and at
/// which level its keys act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `flayer`: a workspace, its own artifacts, and the projects it manages.
    Workspace,
    /// `mind`, on a project no workspace manages: that project on its own.
    Project,
}

/// What the TUI is looking at.
#[derive(Debug, Clone)]
pub enum View {
    /// Nothing yet where the TUI was opened: no workspace for `flayer`, no
    /// project for `mind`.
    Nowhere,
    /// A workspace and everything it holds — or one project on its own.
    Workspace(Workspace),
}

/// A workspace as the screen shows it, or a project on its own.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub root: PathBuf,
    /// The workspace's own artifacts first, then every registered entry in
    /// the order the marker lists them. On its own, a project is the one.
    pub members: Vec<Member>,
    /// What sits under the root and could be linked, but is not.
    pub unlinked: Vec<Unlinked>,
    /// How many artifacts are on the shelf.
    pub shelf: usize,
    /// The workspaces it loads, in marker order.
    pub loaded: Vec<LoadedSource>,
    pub scope: Scope,
    /// For a project on its own: the workspace above it, which `mind link`
    /// would add it to.
    pub above: Option<PathBuf>,
    /// The default workspace in the home directory, opened because there was
    /// none at or above where the TUI was opened.
    pub default: bool,
}

impl Workspace {
    /// How many projects it manages, not counting its own artifacts.
    pub fn projects(&self) -> usize {
        self.members.iter().filter(|member| !member.own).count()
    }
}

/// One holder of artifacts: the workspace's own, or a registered entry.
#[derive(Debug, Clone)]
pub struct Member {
    /// The entry as the marker spells it, which is what a command is given.
    pub entry: String,
    /// Where it points.
    pub root: PathBuf,
    /// The project's name, or the entry when it would not open.
    pub name: String,
    /// The workspace's own artifacts rather than a project's.
    pub own: bool,
    /// What it holds, or why it could not be opened.
    pub holdings: Result<Holdings, String>,
}

impl Member {
    /// The level its commands run at.
    pub fn holder(&self) -> Holder {
        if self.own {
            Holder::Workspace
        } else {
            Holder::Project(self.entry.clone())
        }
    }

    /// Its artifacts; none when it could not be opened.
    pub fn items(&self) -> &[Item] {
        self.holdings
            .as_ref()
            .map_or(&[], |holdings| holdings.items.as_slice())
    }

    /// How many things about it want attention: artifacts `validate` would
    /// reject, and files that could not be read at all.
    pub fn problems(&self) -> usize {
        match &self.holdings {
            Ok(holdings) => {
                let invalid = holdings
                    .items
                    .iter()
                    .filter(|item| item.broken.is_some() || !item.issues.is_empty())
                    .count();
                invalid + holdings.warnings.len()
            }
            Err(_) => 1,
        }
    }
}

/// What an open project holds.
#[derive(Debug, Clone, Default)]
pub struct Holdings {
    pub items: Vec<Item>,
    /// Files that could not be read, one line each.
    pub warnings: Vec<String>,
    /// Where each kind lives, relative to the project, so an empty project
    /// can say where it looked.
    pub directories: Vec<(Kind, String)>,
    /// What `mind add --template` can start from in this project.
    pub templates: Vec<Choice>,
}

/// A template the add form can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub kind: Kind,
}

/// One artifact.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub summary: String,
    /// What `validate` says about it; empty when it is well formed.
    pub issues: Vec<String>,
    /// Why it cannot be read, when it cannot. Listed anyway, by where it sits,
    /// because a file that does not read is the one most in need of opening.
    pub broken: Option<String>,
}

impl Item {
    /// `skill:deploy`: qualified, so a command names this artifact and not a
    /// rule that happens to share its name.
    pub fn reference(&self) -> String {
        qualified(self.kind, &self.name)
    }
}

/// A workspace this one loads, as the screen shows it.
#[derive(Debug, Clone)]
pub struct LoadedSource {
    /// The entry as the marker spells it: what `flayer unload` is given.
    pub entry: String,
    /// Where it points: what `mind unload` is given, from a project.
    pub root: PathBuf,
    /// The loaded workspace's name, or the entry when it cannot be opened.
    pub name: String,
    /// How many of its own it offers to install, or why it cannot be read.
    pub offers: Result<usize, String>,
}

/// A directory under the workspace that is not linked yet.
#[derive(Debug, Clone)]
pub struct Unlinked {
    /// The route from the workspace root, which is what `flayer link` is given.
    pub route: String,
    /// A mind project already, rather than a repository that needs
    /// `mind init` before it can be linked.
    pub mind: bool,
}

// ---------------------------------------------------------------------------
// What a key can ask for
// ---------------------------------------------------------------------------

/// Which level a task runs at: what holds the artifacts it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    /// The workspace, for its own artifacts: `flayer <cmd>`.
    Workspace,
    /// A project, by its entry: `mind -C <entry> <cmd>`, or `mind <cmd>` for
    /// the one at the root the commands run from.
    Project(String),
}

/// Work a key asked for, as the command it is.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// what the `--yes` says.
    Remove {
        holder: Holder,
        kind: Kind,
        name: String,
    },
    /// `flayer validate`, everything; `mind validate`, one project.
    Validate { holder: Holder },
    /// `mind -C <path> init` for each repository that is not a project yet,
    /// then one `flayer link` and one `flayer unlink`: what the minds screen's
    /// marks amount to.
    Minds {
        init: Vec<String>,
        link: Vec<String>,
        unlink: Vec<String>,
    },
    /// `flayer gather list`.
    Shelf,
    /// `flayer gather git [--path <folder>] [--ref <ref>] <url>`.
    Gather {
        url: String,
        folder: String,
        reference: Option<String>,
    },
    /// Open the install screen, which is what `flayer install` opens.
    OpenInstall,
    /// Carry out what the install screen marked.
    ApplyInstall,
    /// Read the disk again.
    Refresh,
}

impl Task {
    /// The command lines this task is, word by word, as a person would type
    /// them from where the commands run: the workspace root, or the project
    /// the TUI was opened on.
    ///
    /// Not a description of the task: the task. The caller parses these with
    /// the real parsers and runs what comes out, so a line here that does not
    /// parse is a task that cannot happen. The two install tasks are the
    /// exception — the screen does that work, and `flayer install` is the
    /// command that opens the same screen — and a refresh changes nothing.
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
            Task::Remove { holder, kind, name } => {
                vec![at(holder, &["remove", "--yes"], &[&qualified(*kind, name)])]
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
            Task::OpenInstall | Task::ApplyInstall => vec![words(&["flayer", "install"], &[])],
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
                | Task::ApplyInstall
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

    /// What to say while it runs.
    pub fn describe(&self) -> String {
        match self {
            Task::Refresh => String::from("reading the disk again"),
            Task::OpenInstall => String::from("opening the install screen"),
            Task::ApplyInstall => String::from("installing and removing what was marked"),
            _ => self
                .commands()
                .iter()
                .map(|line| shell(line))
                .collect::<Vec<_>>()
                .join(" && "),
        }
    }
}

/// One command line, at the level a holder is at: `flayer <command>` for the
/// workspace's own, `mind -C <entry> <command>` for a project — and plain
/// `mind <command>` for the one at the root, where `-C .` says nothing.
fn at(holder: &Holder, command: &[&str], positionals: &[&str]) -> Vec<String> {
    let start: Vec<String> = match holder {
        Holder::Workspace => vec![String::from("flayer")],
        Holder::Project(entry) if entry == "." => vec![String::from("mind")],
        Holder::Project(entry) => vec![String::from("mind"), String::from("-C"), directory(entry)],
    };
    let mut head: Vec<&str> = start.iter().map(String::as_str).collect();
    head.extend_from_slice(command);
    words(&head, positionals)
}

/// `skill:deploy`: a name that says which kind it names.
fn qualified(kind: Kind, name: &str) -> String {
    format!("{}{QUALIFIER}{name}", kind.slug())
}

/// A command line's words: `head`, then the positionals, behind a `--` when
/// one of them would otherwise be read as a flag — a description is free
/// text, and `- the fast way` is a fine one.
fn words(head: &[&str], positionals: &[&str]) -> Vec<String> {
    let mut line: Vec<String> = head.iter().map(|word| (*word).to_owned()).collect();
    if positionals.iter().any(|word| word.starts_with('-')) {
        line.push(String::from("--"));
    }
    line.extend(positionals.iter().map(|word| (*word).to_owned()));
    line
}

/// A flag and its value, joined when the value would be read as a flag.
fn option(flag: &str, value: &str) -> Vec<String> {
    if value.starts_with('-') {
        vec![format!("{flag}={value}")]
    } else {
        vec![flag.to_owned(), value.to_owned()]
    }
}

/// A `-C` value, kept from being read as a flag.
fn directory(path: &str) -> String {
    if path.starts_with('-') {
        format!("./{path}")
    } else {
        path.to_owned()
    }
}

/// A command line as it would be typed into a shell.
pub fn shell(words: &[String]) -> String {
    words
        .iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ")
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

/// What a key press asks the caller to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Redraw and keep going.
    Stay,
    /// Close the TUI.
    Quit,
    /// Do this, then show what came of it.
    Run(Task),
}

// ---------------------------------------------------------------------------
// What came of it
// ---------------------------------------------------------------------------

/// One command the TUI ran, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The command line, as it would be typed.
    pub command: String,
    pub stdout: String,
    pub stderr: Vec<String>,
    /// Why it could not run, when it could not.
    pub error: Option<String>,
    /// Whether it ran and found everything in order: what its exit code
    /// would have said.
    pub ok: bool,
}

impl Done {
    /// A command that did not get as far as saying anything.
    pub fn failed(command: String, error: impl Into<String>) -> Self {
        Self {
            command,
            stdout: String::new(),
            stderr: Vec::new(),
            error: Some(error.into()),
            ok: false,
        }
    }
}

/// What carrying out a task produced.
#[derive(Debug)]
pub enum Performed {
    /// Commands ran; what each said, in order.
    Ran(Vec<Done>),
    /// The install screen, built and ready to be shown.
    Install(InstallScreen),
    /// A directory that exists but is not a mind project. Linking it needs
    /// `mind init` first, and whether to run that is asked, not assumed.
    NeedsInit { path: String },
}

// ---------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------

/// Which column the keyboard is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Items,
}

/// How a line of text should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Info,
    Good,
    Warn,
    Bad,
}

/// One line about the last thing that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub tone: Tone,
}

impl Message {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Info,
        }
    }

    fn good(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Good,
        }
    }

    fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Warn,
        }
    }

    fn bad(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Bad,
        }
    }
}

/// What the keys are doing right now.
#[derive(Debug)]
pub enum Mode {
    /// Moving around the projects and what they hold.
    Browse,
    Adding(AddForm),
    Renaming(RenameForm),
    Linking(LinkForm),
    Loading(LoadForm),
    Gathering(GatherForm),
    /// A yes or no before something that writes.
    Confirming(Confirm),
    /// A page of output: validate, show, the shelf, a report.
    Reading(Reader),
    /// The install screen, embedded as it is.
    Installing(InstallScreen),
    /// Which minds the workspace manages, ticked, to change in one pass.
    Minds(Minds),
}

/// The whole TUI.
#[derive(Debug)]
pub struct App {
    /// Where it was opened, which is where `flayer init` makes a workspace,
    /// or `mind init` a project.
    pub directory: PathBuf,
    /// What it was opened on: `flayer`'s workspace, or `mind`'s project.
    pub opened: Scope,
    pub view: View,
    pub focus: Focus,
    /// Which project the cursor is on.
    pub cursor: usize,
    /// Which artifact of that project.
    pub item: usize,
    pub mode: Mode,
    pub message: Option<Message>,
    /// Every command that changed something, in the order it ran.
    pub done: Vec<Done>,
    /// What is running, drawn over everything until it finishes.
    pub busy: Option<String>,
    /// Where the default workspace lives, for when there is none above: what
    /// every command this TUI runs is told, so none of them guesses.
    pub home: Option<PathBuf>,
}

impl App {
    /// The TUI `flayer` opens: on a workspace.
    pub fn new(directory: PathBuf, view: View) -> Self {
        Self::opened_on(Scope::Workspace, directory, view)
    }

    /// The TUI opened on `scope`.
    pub fn opened_on(scope: Scope, directory: PathBuf, view: View) -> Self {
        let mut app = Self {
            directory,
            opened: scope,
            view,
            focus: Focus::Projects,
            cursor: 0,
            item: 0,
            mode: Mode::Browse,
            message: None,
            done: Vec::new(),
            busy: None,
            home: None,
        };
        app.settle_in();
        app
    }

    /// Where the cursor starts, and what is worth saying first.
    ///
    /// On the first project rather than on the workspace's own row above
    /// them: the projects are what is usually worked on, and their own row
    /// is one key up.
    fn settle_in(&mut self) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let first = workspace.members.iter().position(|member| !member.own);
        let lonely = workspace.scope == Scope::Workspace && workspace.projects() == 0;
        let message = match (&workspace.above, workspace.scope) {
            _ if workspace.default => Some(format!(
                "no workspace here or above — this is your default one, in {}; `flayer init` makes one here",
                workspace.root.display()
            )),
            _ if lonely => Some(format!(
                "{} manages no projects yet — l links one",
                workspace.name
            )),
            (Some(above), Scope::Project) => Some(format!(
                "no workspace manages this project; the one at {} would — l links it",
                above.display()
            )),
            _ => None,
        };
        self.cursor = first.unwrap_or(0);
        self.message = message.map(Message::info);
    }

    /// Tell every command this TUI runs where the default workspace lives.
    pub fn at_home(mut self, home: Option<&Path>) -> Self {
        self.home = home.map(Path::to_path_buf);
        self
    }

    /// Put the cursor on the project `path` is in, in its artifacts if it
    /// has any: where `mind` opens, when the project it was run in is managed.
    ///
    /// The deepest project holding the path, since `mind` can be run from
    /// anywhere inside one.
    pub fn focus_on(&mut self, path: &Path) {
        let at = self.workspace().and_then(|workspace| {
            workspace
                .members
                .iter()
                .enumerate()
                .filter(|(_, member)| !member.own && path.starts_with(&member.root))
                .max_by_key(|(_, member)| member.root.components().count())
                .map(|(at, _)| at)
        });
        if let Some(at) = at {
            self.cursor = at;
            self.item = 0;
            let items = self
                .member()
                .is_some_and(|member| !member.items().is_empty());
            self.focus = if items { Focus::Items } else { Focus::Projects };
        }
    }

    /// The level the keys act at: a project on its own, or a workspace.
    pub fn scope(&self) -> Scope {
        self.workspace()
            .map_or(self.opened, |workspace| workspace.scope)
    }

    /// The workspace, when there is one.
    pub fn workspace(&self) -> Option<&Workspace> {
        match &self.view {
            View::Workspace(workspace) => Some(workspace),
            View::Nowhere => None,
        }
    }

    /// The project under the cursor.
    pub fn member(&self) -> Option<&Member> {
        self.workspace()?.members.get(self.cursor)
    }

    /// The artifact under the cursor, in that project.
    pub fn current_item(&self) -> Option<&Item> {
        self.member()?.items().get(self.item)
    }

    /// Where commands run from: the workspace root, or where the TUI was
    /// opened when there is no workspace yet.
    pub fn base(&self) -> &Path {
        self.workspace()
            .map_or(self.directory.as_path(), |workspace| &workspace.root)
    }

    /// Every command that changed something, as a shell session would show
    /// it. Printed when the TUI closes, so what it did outlives the screen and
    /// can be done again without it.
    pub fn transcript(&self) -> String {
        let mut text = String::new();
        for done in &self.done {
            let _ = writeln!(text, "$ {}", done.command);
            text.push_str(&done.stdout);
            for line in &done.stderr {
                let _ = writeln!(text, "{line}");
            }
            if let Some(error) = &done.error {
                let _ = writeln!(text, "error: {error}");
            }
        }
        text
    }

    // -----------------------------------------------------------------------
    // Keys
    // -----------------------------------------------------------------------

    /// Feed it a key.
    pub fn press(&mut self, key: KeyEvent) -> Step {
        // From anywhere, typing included: raw mode swallows the signal, and a
        // screen that cannot be left that way feels broken.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Step::Quit;
        }
        // A message explains the key before this one, so it goes as soon as
        // another is pressed rather than sitting there being about nothing.
        self.message = None;

        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        let (mode, step) = match mode {
            Mode::Browse => self.browse(key),
            Mode::Adding(form) => self.adding(form, key),
            Mode::Renaming(form) => self.renaming(form, key),
            Mode::Linking(form) => self.linking(form, key),
            Mode::Loading(form) => self.loading(form, key),
            Mode::Gathering(form) => gathering(form, key),
            Mode::Confirming(confirm) => confirming(confirm, key),
            Mode::Reading(reader) => reading(reader, key),
            Mode::Installing(screen) => self.installing(screen, key),
            Mode::Minds(screen) => self.in_minds(screen, key),
        };
        self.mode = mode;
        step
    }

    fn browse(&mut self, key: KeyEvent) -> (Mode, Step) {
        if self.workspace().is_none() {
            // With nothing here there is one thing to do, or leave: make what
            // the TUI was opened for.
            let make = match self.opened {
                Scope::Workspace => Task::InitWorkspace,
                Scope::Project => Task::InitProject,
            };
            let step = match key.code {
                KeyCode::Enter | KeyCode::Char('i' | 'y') => Step::Run(make),
                KeyCode::Char('q') | KeyCode::Esc => Step::Quit,
                _ => Step::Stay,
            };
            return (Mode::Browse, step);
        }

        match key.code {
            KeyCode::Char('q') => return (Mode::Browse, Step::Quit),
            KeyCode::Char('?') => return (Mode::Reading(Reader::help()), Step::Stay),
            KeyCode::Char('R') => return (Mode::Browse, Step::Run(Task::Refresh)),
            KeyCode::Char(key @ ('e' | 'r' | 'd')) => return self.on_item(key),
            KeyCode::Char('a') => return self.start_adding(),
            KeyCode::Char('v') => {
                let holder = match self.scope() {
                    Scope::Workspace => Holder::Workspace,
                    Scope::Project => Holder::Project(String::from(".")),
                };
                return (Mode::Browse, Step::Run(Task::Validate { holder }));
            }
            _ => {}
        }
        // What only a workspace has: its registry, its shelf, its installs.
        if let KeyCode::Char(key @ ('l' | 'u' | 'g' | 's' | 'i' | 'm' | 'L')) = key.code {
            return match self.scope() {
                Scope::Workspace => self.in_workspace(key),
                Scope::Project => self.on_its_own(key),
            };
        }
        match self.focus {
            Focus::Projects => self.in_projects(key),
            Focus::Items => self.in_items(key),
        }
    }

    fn in_workspace(&mut self, key: char) -> (Mode, Step) {
        match key {
            'l' => (Mode::Linking(LinkForm::default()), Step::Stay),
            'L' => (Mode::Loading(LoadForm::default()), Step::Stay),
            'u' => self.start_unlinking(),
            'g' => (Mode::Gathering(GatherForm::default()), Step::Stay),
            's' => (Mode::Browse, Step::Run(Task::Shelf)),
            'm' => match self.workspace() {
                Some(workspace) => (Mode::Minds(Minds::of(workspace)), Step::Stay),
                None => (Mode::Browse, Step::Stay),
            },
            _ => (Mode::Browse, Step::Run(Task::OpenInstall)),
        }
    }

    /// A project no workspace manages has no registry, shelf or installs of
    /// its own; the one thing to do about that is link it to a workspace.
    fn on_its_own(&mut self, key: char) -> (Mode, Step) {
        let above = self
            .workspace()
            .and_then(|workspace| workspace.above.clone());
        match (key, above) {
            ('l', Some(_)) => (Mode::Browse, Step::Run(Task::LinkHere)),
            // Loading is into the workspace above, which does not have to
            // manage this project yet: `mind load`, as `mind link` is.
            ('L', Some(_)) => (Mode::Loading(LoadForm::default()), Step::Stay),
            ('l' | 'L', None) => {
                self.message = Some(Message::info(
                    "no workspace above this project — `flayer init` in the folder that holds your projects makes one",
                ));
                (Mode::Browse, Step::Stay)
            }
            _ => {
                self.message = Some(Message::info(
                    "that is a workspace's, and no workspace manages this project — l links it to one",
                ));
                (Mode::Browse, Step::Stay)
            }
        }
    }

    fn in_projects(&mut self, key: KeyEvent) -> (Mode, Step) {
        let count = self
            .workspace()
            .map_or(0, |workspace| workspace.members.len());
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if self.cursor > 0 => {
                self.cursor -= 1;
                self.item = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if self.cursor + 1 < count => {
                self.cursor += 1;
                self.item = 0;
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Tab => {
                let empty = self
                    .member()
                    .map(|member| (member.items().is_empty(), member.name.clone()));
                match empty {
                    Some((false, _)) => self.focus = Focus::Items,
                    Some((true, name)) => {
                        self.message = Some(Message::info(format!(
                            "nothing in {name} yet — a adds a skill or a rule"
                        )));
                    }
                    None => {}
                }
            }
            _ => {}
        }
        (Mode::Browse, Step::Stay)
    }

    fn in_items(&mut self, key: KeyEvent) -> (Mode, Step) {
        let count = self.member().map_or(0, |member| member.items().len());
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.item = self.item.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if self.item + 1 < count => self.item += 1,
            KeyCode::Left | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
                self.focus = Focus::Projects;
            }
            KeyCode::Enter => {
                if let (Some(member), Some(item)) = (self.member(), self.current_item()) {
                    let task = Task::Show {
                        holder: member.holder(),
                        reference: item.reference(),
                    };
                    return (Mode::Browse, Step::Run(task));
                }
            }
            _ => {}
        }
        (Mode::Browse, Step::Stay)
    }

    /// `e`, `r` and `d`: edit, rename or delete the artifact under the cursor.
    ///
    /// Only from the artifacts column. From the projects column the cursor in
    /// the other one is dimmed, and a key that deletes what a dimmed cursor
    /// happens to rest on is a key that deletes the wrong thing.
    fn on_item(&mut self, key: char) -> (Mode, Step) {
        let picked = match (self.focus, self.member(), self.current_item()) {
            (Focus::Items, Some(member), Some(item)) => {
                Some((member.holder(), item.kind, item.name.clone()))
            }
            _ => None,
        };
        let Some((holder, kind, name)) = picked else {
            self.message = Some(Message::info(
                "pick an artifact first — → moves over to them",
            ));
            return (Mode::Browse, Step::Stay);
        };

        match key {
            'e' => (Mode::Browse, Step::Run(Task::Edit { holder, kind, name })),
            'r' => (
                Mode::Renaming(RenameForm::new(self.cursor, kind, name)),
                Step::Stay,
            ),
            _ => {
                let what = match kind.layout() {
                    Layout::Directory { .. } => "Its folder goes, and everything in it.",
                    Layout::Files { .. } => "The file goes.",
                };
                let confirm = Confirm {
                    question: vec![
                        format!("Delete {kind} {name}?"),
                        String::new(),
                        what.to_owned(),
                    ],
                    task: Task::Remove { holder, kind, name },
                    back: Box::new(Mode::Browse),
                };
                (Mode::Confirming(confirm), Step::Stay)
            }
        }
    }

    fn renaming(&mut self, mut form: RenameForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        match key.code {
            KeyCode::Esc => (Mode::Browse, Step::Stay),
            KeyCode::Enter => match self.rename_task(&form) {
                Some(task) => (Mode::Renaming(form), Step::Run(task)),
                None => (Mode::Browse, Step::Stay),
            },
            _ => {
                form.name.press(key);
                (Mode::Renaming(form), Step::Stay)
            }
        }
    }

    /// What the rename form would run, if it were submitted now.
    pub fn rename_task(&self, form: &RenameForm) -> Option<Task> {
        let member = self.workspace()?.members.get(form.member)?;
        Some(Task::Rename {
            holder: member.holder(),
            kind: form.kind,
            from: form.from.clone(),
            to: form.name.text().trim().to_owned(),
        })
    }

    /// The templates the add form can offer for `kind`, in the order it
    /// offers them.
    pub fn templates_for(&self, member: usize, kind: Kind) -> Vec<&Choice> {
        self.workspace()
            .and_then(|workspace| workspace.members.get(member))
            .and_then(|member| member.holdings.as_ref().ok())
            .map(|holdings| {
                holdings
                    .templates
                    .iter()
                    .filter(|choice| choice.kind == kind)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn start_adding(&mut self) -> (Mode, Step) {
        let refusal = match self.member() {
            Some(member) if member.holdings.is_ok() => None,
            Some(member) => Some(format!(
                "{} cannot be opened, so nothing can be added to it",
                member.name
            )),
            None => Some(String::from("link a project first — l")),
        };
        match refusal {
            None => (Mode::Adding(AddForm::new(self.cursor)), Step::Stay),
            Some(refusal) => {
                self.message = Some(Message::info(refusal));
                (Mode::Browse, Step::Stay)
            }
        }
    }

    fn start_unlinking(&mut self) -> (Mode, Step) {
        let Some(member) = self.member() else {
            self.message = Some(Message::info("nothing to unlink"));
            return (Mode::Browse, Step::Stay);
        };
        if member.own {
            self.message = Some(Message::info(
                "these are the workspace's own, not a project it links — there is nothing to unlink",
            ));
            return (Mode::Browse, Step::Stay);
        }
        let confirm = Confirm {
            question: vec![
                format!("Unlink {}?", member.name),
                String::new(),
                String::from("The workspace forgets it; nothing on disk is deleted."),
            ],
            task: Task::Unlink {
                entry: member.entry.clone(),
            },
            back: Box::new(Mode::Browse),
        };
        (Mode::Confirming(confirm), Step::Stay)
    }

    fn adding(&mut self, mut form: AddForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Enter => {
                return match self.add_task(&form) {
                    Some(task) => (Mode::Adding(form), Step::Run(task)),
                    // The project it was for has gone from under it.
                    None => (Mode::Browse, Step::Stay),
                };
            }
            KeyCode::Tab | KeyCode::Down => form.field = form.field.next(),
            KeyCode::BackTab | KeyCode::Up => form.field = form.field.previous(),
            _ => match form.field {
                AddField::Kind => {
                    if matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                    ) {
                        form.kind = other_kind(form.kind);
                        // Templates belong to a kind, so the pick does not
                        // carry across.
                        form.template = None;
                    }
                }
                AddField::Template => {
                    let offered = self.templates_for(form.member, form.kind).len();
                    form.template = match key.code {
                        KeyCode::Right | KeyCode::Char(' ') => {
                            step_through(form.template, offered, true)
                        }
                        KeyCode::Left => step_through(form.template, offered, false),
                        _ => form.template,
                    };
                }
                AddField::Name => {
                    form.name.press(key);
                }
                AddField::Description => {
                    form.description.press(key);
                }
            },
        }
        (Mode::Adding(form), Step::Stay)
    }

    /// What the add form would run, if it were submitted now.
    pub fn add_task(&self, form: &AddForm) -> Option<Task> {
        let member = self.workspace()?.members.get(form.member)?;
        let template = form.template.and_then(|at| {
            self.templates_for(form.member, form.kind)
                .get(at)
                .map(|choice| choice.name.clone())
        });
        Some(Task::Add {
            holder: member.holder(),
            kind: form.kind,
            name: form.name.text().trim().to_owned(),
            description: form.description.text().trim().to_owned(),
            template,
        })
    }

    fn linking(&mut self, mut form: LinkForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        let choices = self
            .workspace()
            .map_or(0, |workspace| workspace.unlinked.len());
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Up => form.pick = form.pick.saturating_sub(1),
            KeyCode::Down if form.pick + 1 < choices => form.pick += 1,
            KeyCode::Enter => {
                return match self.link_task(&form) {
                    // Making a repository a mind project writes into it, so
                    // it is asked about first.
                    Some(Task::InitAndLink { path }) => {
                        let confirm = Confirm::init_and_link(path, Mode::Linking(form));
                        (Mode::Confirming(confirm), Step::Stay)
                    }
                    Some(task) => (Mode::Linking(form), Step::Run(task)),
                    None => {
                        form.error = Some(String::from(
                            "type the path of a project, relative to the workspace",
                        ));
                        (Mode::Linking(form), Step::Stay)
                    }
                };
            }
            _ => {
                form.path.press(key);
            }
        }
        (Mode::Linking(form), Step::Stay)
    }

    /// What the link form would run, if it were submitted now: the path typed,
    /// or else the directory picked from what the workspace found.
    pub fn link_task(&self, form: &LinkForm) -> Option<Task> {
        let typed = form.path.text().trim();
        if !typed.is_empty() {
            return Some(Task::Link {
                path: typed.to_owned(),
            });
        }
        let picked = self.workspace()?.unlinked.get(form.pick)?;
        let path = picked.route.clone();
        Some(if picked.mind {
            Task::Link { path }
        } else {
            Task::InitAndLink { path }
        })
    }

    fn loading(&mut self, mut form: LoadForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        let choices = self
            .workspace()
            .map_or(0, |workspace| workspace.loaded.len());
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Up => form.pick = form.pick.saturating_sub(1),
            KeyCode::Down if form.pick + 1 < choices => form.pick += 1,
            KeyCode::Enter => {
                if let Some(task) = self.load_task(&form) {
                    // The workspace a load goes into skips the one being
                    // loaded, so loading the one on screen would quietly go
                    // somewhere else — into the one above, or your home's.
                    if self.loads_itself(&form) {
                        form.error = Some(String::from(
                            "that is the workspace this would load into — a workspace cannot load itself",
                        ));
                        return (Mode::Loading(form), Step::Stay);
                    }
                    return (Mode::Loading(form), Step::Run(task));
                }
                // Nothing typed: enter on a loaded one offers to unload it,
                // asked first as unlinking is.
                let scope = self.scope();
                let picked = self
                    .workspace()
                    .and_then(|workspace| workspace.loaded.get(form.pick))
                    .cloned();
                return match picked {
                    Some(source) => {
                        let task = match scope {
                            Scope::Workspace => Task::Unload {
                                entry: source.entry,
                            },
                            Scope::Project => Task::UnloadHere {
                                path: source.root.to_string_lossy().into_owned(),
                            },
                        };
                        let confirm = Confirm {
                            question: vec![
                                format!("Unload {}?", source.name),
                                String::new(),
                                String::from(
                                    "Its skills stop being offered here; what was installed from it stays, and nothing on disk is deleted.",
                                ),
                            ],
                            task,
                            back: Box::new(Mode::Loading(form)),
                        };
                        (Mode::Confirming(confirm), Step::Stay)
                    }
                    None => {
                        form.error =
                            Some(String::from("type the path of a flayer workspace to load"));
                        (Mode::Loading(form), Step::Stay)
                    }
                };
            }
            _ => {
                form.path.press(key);
            }
        }
        (Mode::Loading(form), Step::Stay)
    }

    /// Whether the path typed into the load form is the workspace it would
    /// load into: the one on screen, or for a project the one above it.
    fn loads_itself(&self, form: &LoadForm) -> bool {
        let Some(workspace) = self.workspace() else {
            return false;
        };
        let into = match workspace.scope {
            Scope::Workspace => Some(workspace.root.as_path()),
            Scope::Project => workspace.above.as_deref(),
        };
        let typed = self.base().join(form.path.text().trim());
        into.is_some_and(|into| {
            let (a, b) = (paths::normalize(&typed), paths::normalize(into));
            a == b
                || matches!(
                    (std::fs::canonicalize(&a), std::fs::canonicalize(&b)),
                    (Ok(a), Ok(b)) if a == b
                )
        })
    }

    /// What the load form would run, if it were submitted now: `flayer load`
    /// on a workspace, `mind load` on a project on its own. Nothing without a
    /// path — picking a loaded one is for unloading it.
    pub fn load_task(&self, form: &LoadForm) -> Option<Task> {
        let path = form.path.text().trim();
        if path.is_empty() {
            return None;
        }
        let path = path.to_owned();
        Some(match self.scope() {
            Scope::Workspace => Task::Load { path },
            Scope::Project => Task::LoadHere { path },
        })
    }

    fn in_minds(&mut self, mut screen: Minds, key: KeyEvent) -> (Mode, Step) {
        match screen.press(key.code) {
            minds::Key::Stay => (Mode::Minds(screen), Step::Stay),
            minds::Key::Leave => {
                if !screen.idle() {
                    self.message = Some(Message::info("left the minds screen; nothing applied"));
                }
                (Mode::Browse, Step::Stay)
            }
            minds::Key::Apply => match screen.task() {
                // Asked first, like every other change that writes: `mind
                // init` writes into a repository, and unlinking forgets.
                Some(task) => {
                    let confirm = Confirm {
                        question: screen.question(),
                        task,
                        back: Box::new(Mode::Minds(screen)),
                    };
                    (Mode::Confirming(confirm), Step::Stay)
                }
                None => {
                    self.message = Some(Message::info("nothing marked"));
                    (Mode::Minds(screen), Step::Stay)
                }
            },
        }
    }

    fn installing(&mut self, mut screen: InstallScreen, key: KeyEvent) -> (Mode, Step) {
        match screen.press(key.code) {
            InstallStep::Stay => (Mode::Installing(screen), Step::Stay),
            // The install screen's own way out leaves it, not the TUI.
            InstallStep::Quit => {
                if !screen.idle() {
                    self.message = Some(Message::info("left the install screen; nothing applied"));
                }
                (Mode::Browse, Step::Stay)
            }
            // Kept open while it applies: the marks are what gets applied.
            InstallStep::Apply => (Mode::Installing(screen), Step::Run(Task::ApplyInstall)),
        }
    }

    // -----------------------------------------------------------------------
    // What came back
    // -----------------------------------------------------------------------

    /// Show what came of a task, with the disk read again when it asked for
    /// that.
    pub fn finish(
        &mut self,
        task: &Task,
        performed: Performed,
        view: Option<Result<View, String>>,
    ) {
        if let Some(view) = view {
            match view {
                Ok(view) => self.reload(view),
                Err(error) => self.message = Some(Message::bad(error)),
            }
        }

        let dones = match performed {
            Performed::Install(screen) => {
                if screen.targets.is_empty() {
                    self.message = Some(Message::info(
                        "no projects to install into yet — l links one",
                    ));
                } else {
                    self.mode = Mode::Installing(screen);
                }
                return;
            }
            Performed::NeedsInit { path } => {
                let back = std::mem::replace(&mut self.mode, Mode::Browse);
                self.mode = Mode::Confirming(Confirm::init_and_link(path, back));
                return;
            }
            Performed::Ran(dones) => dones,
        };

        if task.writes() {
            self.done.extend(dones.iter().cloned());
        }
        let failure = dones.iter().find_map(|done| done.error.clone());

        match (task, failure) {
            (Task::Refresh, _) => self.message = Some(Message::info("read the disk again")),
            // Output worth reading in full, whether or not it went well.
            (
                Task::Show { .. }
                | Task::Validate { .. }
                | Task::Shelf
                | Task::ApplyInstall
                | Task::Minds { .. },
                _,
            )
            | (Task::Gather { .. }, None) => {
                self.mode = Mode::Reading(Reader::of(&dones));
            }
            (_, Some(error)) => {
                // A form stays open with the reason in it, so a typo is fixed
                // where it was made rather than typed out again.
                if !self.form_error(&error) {
                    self.message = Some(Message::bad(error));
                }
            }
            (_, None) => self.succeeded(task, &dones),
        }
    }

    fn succeeded(&mut self, task: &Task, dones: &[Done]) {
        let said = dones
            .last()
            .and_then(|done| done.stdout.lines().next())
            .unwrap_or("done");
        // Ran, but found something wrong on the way: an edit that left the
        // skill invalid is not news to give in green.
        let clean = dones.iter().all(|done| done.ok);
        self.message = Some(if clean {
            Message::good(said)
        } else {
            Message::warn(said)
        });

        match task {
            Task::InitWorkspace | Task::Link { .. } | Task::InitAndLink { .. } => {
                if let Task::Link { path } | Task::InitAndLink { path } = task {
                    self.select_member(path);
                }
                // Straight back to linking while there is something left to
                // link: a workspace is usually set up several projects at once.
                let more = self
                    .workspace()
                    .is_some_and(|workspace| !workspace.unlinked.is_empty());
                self.mode = if more {
                    Mode::Linking(LinkForm::default())
                } else {
                    Mode::Browse
                };
            }
            Task::Add { kind, name, .. } | Task::Rename { kind, to: name, .. } => {
                self.mode = Mode::Browse;
                self.select_item(*kind, name);
            }
            // The project the TUI was opened on, now a project — or now one a
            // workspace manages, which is what the screen shows from here on.
            Task::InitProject | Task::LinkHere => {
                self.mode = Mode::Browse;
                let here = self.directory.clone();
                self.cursor = 0;
                self.settle_in();
                self.focus_on(&here);
                self.message = Some(Message::good(said));
            }
            _ => self.mode = Mode::Browse,
        }
    }

    /// Put an error into the form that is open, if one is.
    fn form_error(&mut self, error: &str) -> bool {
        let slot = match &mut self.mode {
            Mode::Adding(form) => &mut form.error,
            Mode::Renaming(form) => &mut form.error,
            Mode::Linking(form) => &mut form.error,
            Mode::Loading(form) => &mut form.error,
            Mode::Gathering(form) => &mut form.error,
            _ => return false,
        };
        *slot = Some(error.to_owned());
        true
    }

    /// Take a fresh reading of the disk, keeping the cursor on what it was on
    /// wherever that has moved to.
    fn reload(&mut self, view: View) {
        let entry = self.member().map(|member| member.entry.clone());
        let item = self
            .current_item()
            .map(|item| (item.kind, item.name.clone()));
        self.view = view;

        let members = self.workspace().map(|workspace| {
            let at = entry.and_then(|entry| {
                workspace
                    .members
                    .iter()
                    .position(|member| member.entry == entry)
            });
            (at, workspace.members.len())
        });
        let Some((at, count)) = members else {
            self.cursor = 0;
            self.item = 0;
            self.focus = Focus::Projects;
            return;
        };
        self.cursor = at.unwrap_or(self.cursor).min(count.saturating_sub(1));

        let items = self.member().map(|member| {
            let at = item.and_then(|(kind, name)| {
                member
                    .items()
                    .iter()
                    .position(|item| item.kind == kind && item.name == name)
            });
            (at, member.items().len())
        });
        let (at, count) = items.unwrap_or((None, 0));
        self.item = at.unwrap_or(self.item).min(count.saturating_sub(1));
        if count == 0 {
            self.focus = Focus::Projects;
        }
    }

    /// Move the cursor to the project a path names.
    fn select_member(&mut self, path: &str) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let target = paths::normalize(&workspace.root.join(path));
        // Not the workspace's own row, which shares the root with a project
        // linked as `.`.
        if let Some(at) = workspace
            .members
            .iter()
            .position(|member| !member.own && member.root == target)
        {
            self.cursor = at;
            self.item = 0;
            self.focus = Focus::Projects;
        }
    }

    /// Move the cursor to an artifact of the current project.
    fn select_item(&mut self, kind: Kind, name: &str) {
        let at = self.member().and_then(|member| {
            member
                .items()
                .iter()
                .position(|item| item.kind == kind && item.name == name)
        });
        if let Some(at) = at {
            self.item = at;
            self.focus = Focus::Items;
        }
    }
}

/// The kind after this one, round and round.
fn other_kind(kind: Kind) -> Kind {
    let at = Kind::ALL.iter().position(|each| *each == kind).unwrap_or(0);
    Kind::ALL[(at + 1) % Kind::ALL.len()]
}

/// The next pick among `count` choices, where no pick at all is a choice too
/// and comes first: none, the first, …, the last, none again.
fn step_through(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    match (current, forward) {
        (_, _) if count == 0 => None,
        (None, true) => Some(0),
        (None, false) => Some(count - 1),
        (Some(at), true) => (at + 1 < count).then_some(at + 1),
        (Some(at), false) => at.checked_sub(1),
    }
}

fn gathering(mut form: GatherForm, key: KeyEvent) -> (Mode, Step) {
    form.error = None;
    match key.code {
        KeyCode::Esc => return (Mode::Browse, Step::Stay),
        KeyCode::Enter => {
            return match form.task() {
                Some(task) => (Mode::Gathering(form), Step::Run(task)),
                None => {
                    form.error = Some(String::from("a repository URL is needed"));
                    (Mode::Gathering(form), Step::Stay)
                }
            };
        }
        KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % GATHER_FIELDS.len(),
        KeyCode::BackTab | KeyCode::Up => {
            form.field = (form.field + GATHER_FIELDS.len() - 1) % GATHER_FIELDS.len();
        }
        _ => {
            form.input_mut().press(key);
        }
    }
    (Mode::Gathering(form), Step::Stay)
}

fn confirming(confirm: Confirm, key: KeyEvent) -> (Mode, Step) {
    match key.code {
        // Back to where it was asked from while it runs, so a failure lands in
        // the form that led here.
        KeyCode::Char('y') | KeyCode::Enter => (*confirm.back, Step::Run(confirm.task)),
        KeyCode::Char('n') | KeyCode::Esc => (*confirm.back, Step::Stay),
        _ => (Mode::Confirming(confirm), Step::Stay),
    }
}

fn reading(mut reader: Reader, key: KeyEvent) -> (Mode, Step) {
    let last = reader.lines.len().saturating_sub(1);
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return (Mode::Browse, Step::Stay),
        KeyCode::Up | KeyCode::Char('k') => reader.scroll = reader.scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => reader.scroll = (reader.scroll + 1).min(last),
        KeyCode::PageUp => reader.scroll = reader.scroll.saturating_sub(PAGE),
        KeyCode::PageDown | KeyCode::Char(' ') => {
            reader.scroll = (reader.scroll + PAGE).min(last);
        }
        KeyCode::Home | KeyCode::Char('g') => reader.scroll = 0,
        KeyCode::End | KeyCode::Char('G') => reader.scroll = last,
        _ => {}
    }
    (Mode::Reading(reader), Step::Stay)
}

// ---------------------------------------------------------------------------
// Forms, questions and pages
// ---------------------------------------------------------------------------

/// One line of text being typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// In characters, not bytes, so a key never lands inside one.
    cursor: usize,
}

impl Input {
    /// An input that starts out holding `text`, with the cursor at its end.
    pub fn with(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Apply an editing key, and say whether it was one.
    pub fn press(&mut self, key: KeyEvent) -> bool {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Ctrl-U clears the line, as it does at a shell prompt.
            KeyCode::Char('u') if control => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) if !control && !key.modifiers.contains(KeyModifiers::ALT) => {
                let at = self.byte(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Delete if self.cursor < self.len() => {
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => return false,
        }
        true
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// The byte offset of a character position.
    fn byte(&self, position: usize) -> usize {
        self.text
            .char_indices()
            .nth(position)
            .map_or(self.text.len(), |(at, _)| at)
    }
}

/// Which line of the add form the keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Kind,
    Template,
    Name,
    Description,
}

impl AddField {
    const ORDER: [AddField; 4] = [
        AddField::Kind,
        AddField::Template,
        AddField::Name,
        AddField::Description,
    ];

    fn next(self) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|each| *each == self)
            .unwrap_or(0);
        Self::ORDER[(at + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|each| *each == self)
            .unwrap_or(0);
        Self::ORDER[(at + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// A skill or a rule about to be created.
#[derive(Debug, Clone)]
pub struct AddForm {
    /// The project it goes into.
    pub member: usize,
    pub kind: Kind,
    /// Which of that kind's templates, if any.
    pub template: Option<usize>,
    pub name: Input,
    pub description: Input,
    pub field: AddField,
    /// Why the last attempt did not work.
    pub error: Option<String>,
}

impl AddForm {
    /// Opens on the name, because that is what is typed first; the kind and
    /// the template are the lines above it.
    pub fn new(member: usize) -> Self {
        Self {
            member,
            kind: Kind::Skill,
            template: None,
            name: Input::default(),
            description: Input::default(),
            field: AddField::Name,
            error: None,
        }
    }
}

/// An artifact about to get a new name.
#[derive(Debug, Clone)]
pub struct RenameForm {
    /// The project it is in.
    pub member: usize,
    pub kind: Kind,
    /// The name it has now.
    pub from: String,
    /// The name it will have, starting out as the one it has: most renames
    /// change a word, not the whole thing.
    pub name: Input,
    pub error: Option<String>,
}

impl RenameForm {
    pub fn new(member: usize, kind: Kind, from: String) -> Self {
        Self {
            member,
            kind,
            name: Input::with(&from),
            from,
            error: None,
        }
    }
}

/// A project about to be linked: typed, or picked from what the workspace
/// found under itself.
#[derive(Debug, Clone, Default)]
pub struct LinkForm {
    pub path: Input,
    /// Which of the unlinked directories is picked.
    pub pick: usize,
    pub error: Option<String>,
}

/// A flayer workspace about to be loaded, typed; or one already loaded,
/// picked to unload.
#[derive(Debug, Clone, Default)]
pub struct LoadForm {
    pub path: Input,
    /// Which of the loaded workspaces is picked.
    pub pick: usize,
    pub error: Option<String>,
}

/// The lines of the gather form, in order.
pub const GATHER_FIELDS: [&str; 3] = ["url", "folder", "ref"];

/// A repository about to be gathered from.
#[derive(Debug, Clone)]
pub struct GatherForm {
    pub url: Input,
    /// The folder inside it to take skills from.
    pub folder: Input,
    /// A branch or tag; empty for the repository's default.
    pub reference: Input,
    /// Which of [`GATHER_FIELDS`] the keyboard is on.
    pub field: usize,
    pub error: Option<String>,
}

impl Default for GatherForm {
    fn default() -> Self {
        Self {
            url: Input::default(),
            folder: Input::with(DEFAULT_SUBDIRECTORY),
            reference: Input::default(),
            field: 0,
            error: None,
        }
    }
}

impl GatherForm {
    /// The input the keyboard is on.
    pub fn input(&self, field: usize) -> &Input {
        match field {
            0 => &self.url,
            1 => &self.folder,
            _ => &self.reference,
        }
    }

    fn input_mut(&mut self) -> &mut Input {
        match self.field {
            0 => &mut self.url,
            1 => &mut self.folder,
            _ => &mut self.reference,
        }
    }

    /// What it would run, if it were submitted now. Nothing without a URL.
    pub fn task(&self) -> Option<Task> {
        let url = self.url.text().trim();
        if url.is_empty() {
            return None;
        }
        let folder = match self.folder.text().trim() {
            "" => DEFAULT_SUBDIRECTORY,
            folder => folder,
        };
        let reference = Some(self.reference.text().trim())
            .filter(|reference| !reference.is_empty())
            .map(str::to_owned);
        Some(Task::Gather {
            url: url.to_owned(),
            folder: folder.to_owned(),
            reference,
        })
    }
}

/// A yes or no before something that writes.
#[derive(Debug)]
pub struct Confirm {
    pub question: Vec<String>,
    /// What yes runs.
    pub task: Task,
    /// Where either answer goes back to.
    pub back: Box<Mode>,
}

impl Confirm {
    /// Whether to make a directory a mind project so it can be linked.
    fn init_and_link(path: String, back: Mode) -> Self {
        Self {
            question: vec![
                format!("{path} is not a mind project yet."),
                String::new(),
                String::from(
                    "Make it one — a .mind marker and empty skills/ and rules/ folders — and link it?",
                ),
            ],
            task: Task::InitAndLink { path },
            back: Box::new(back),
        }
    }
}

/// A page of output to read.
#[derive(Debug, Clone)]
pub struct Reader {
    pub title: String,
    pub lines: Vec<(Tone, String)>,
    /// The first line shown.
    pub scroll: usize,
}

impl Reader {
    /// What some commands said: their output, their warnings, and why they
    /// failed if they did, in that order.
    pub fn of(dones: &[Done]) -> Self {
        let title = dones
            .iter()
            .map(|done| done.command.as_str())
            .collect::<Vec<_>>()
            .join(" && ");
        let mut lines = Vec::new();
        for done in dones {
            lines.extend(
                done.stdout
                    .lines()
                    .map(|line| (Tone::Plain, line.to_owned())),
            );
            lines.extend(done.stderr.iter().map(|line| (Tone::Warn, line.clone())));
            if let Some(error) = &done.error {
                lines.push((Tone::Bad, format!("error: {error}")));
            }
        }
        if lines.is_empty() {
            lines.push((Tone::Info, String::from("nothing to show")));
        }
        Self {
            title,
            lines,
            scroll: 0,
        }
    }

    /// Every key, and the command it runs.
    pub fn help() -> Self {
        let rows: [(&str, &str, &[&str]); 15] = [
            (
                "a",
                "add a skill or a rule",
                &["mind -C <project> add <kind> <name> <description>"],
            ),
            (
                "enter",
                "show an artifact",
                &["mind -C <project> show <kind:name>"],
            ),
            (
                "e",
                "edit it, in $VISUAL or $EDITOR",
                &["mind -C <project> edit <kind:name>"],
            ),
            (
                "r",
                "rename it",
                &["mind -C <project> rename <kind:name> <new-name>"],
            ),
            (
                "d",
                "delete it, once you say yes",
                &["mind -C <project> remove --yes <kind:name>"],
            ),
            (
                "l",
                "link a project",
                &[
                    "flayer link <path>",
                    "mind -C <path> init, first, for a repository that is not one yet",
                ],
            ),
            ("u", "unlink the project", &["flayer unlink <path>"]),
            (
                "L",
                "load another workspace's own skills, read live from there",
                &[
                    "flayer load <path>",
                    "flayer unload <path>, on one already loaded",
                    "mind load / mind unload, on a project on its own",
                ],
            ),
            ("v", "validate everything", &["flayer validate"]),
            (
                "g",
                "gather from a git repository",
                &["flayer gather git <url>"],
            ),
            ("s", "what is on the shelf", &["flayer gather list"]),
            ("i", "install from the shelf", &["flayer install"]),
            (
                "m",
                "which minds the workspace manages, ticked",
                &["flayer link <path>...", "flayer unlink <path>..."],
            ),
            ("R", "read the disk again", &[]),
            ("q", "quit, printing what was run", &[]),
        ];
        let mut lines = vec![
            (
                Tone::Plain,
                String::from("Every key here runs a command you could type instead."),
            ),
            (Tone::Plain, String::new()),
        ];
        // Each command on a line of its own under what the key does: side by
        // side, the longest wraps into the next key's row on any terminal
        // narrower than a hundred columns.
        for (key, what, commands) in rows {
            lines.push((Tone::Plain, format!("  {key:<6} {what}")));
            for command in commands {
                lines.push((Tone::Info, format!("         {command}")));
            }
        }
        lines.push((Tone::Plain, String::new()));
        lines.push((
            Tone::Info,
            String::from(
                "On the workspace's own row the same keys run `flayer <cmd>` instead of `mind -C <project> <cmd>`.",
            ),
        ));
        lines.push((
            Tone::Info,
            String::from("What the link form offers is what `flayer scan` lists."),
        ));
        Self {
            title: String::from("keys"),
            lines,
            scroll: 0,
        }
    }
}
