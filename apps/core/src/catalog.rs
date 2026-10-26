//! Every artifact held by a set of mind projects.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::Agent;
use crate::artifact::{Artifact, ArtifactError};
use crate::hash;
use crate::install;
use crate::kind::{Kind, Layout};
use crate::ledger::{Ledger, LedgerError};
use crate::paths;
use crate::workspace::{same_directory, FlayerWorkspace, MindProject};

/// The artifacts discovered across some mind projects, and what went wrong.
///
/// Failures are collected rather than returned, because one unreadable file
/// must not hide the forty next to it: a front end lists what it found and
/// reports the rest.
#[derive(Debug, Default)]
pub struct Catalog {
    artifacts: Vec<Artifact>,
    failures: Vec<DiscoveryFailure>,
}

impl Catalog {
    /// Load every kind held by every given project.
    pub fn discover(projects: &[MindProject]) -> Self {
        Self::discover_kinds(projects, &Kind::ALL)
    }

    /// The same, restricted to some kinds.
    ///
    /// Restricting here rather than filtering the result is what keeps a
    /// listing from reporting failures in a kind nobody asked about.
    ///
    /// A project marked by its settings is read where its agents read: the
    /// folders its agents are sent copies in ([`MindProject::targets`]),
    /// beside the folder it authors in, each folder once
    /// ([`folders`](Self::folders)). What its agents load is what sits in
    /// their folders, whoever put it there, so that is what is listed: an
    /// install's copies, and a skill somebody keeps there by hand, which
    /// nothing records and so is [foreign](crate::Provenance::Foreign). Every
    /// copy of one name in the agents' folders is one artifact, with each
    /// agent that reads a copy ([`Artifact::agents`]), and copies that do not
    /// hold the same are a failure ([`DiscoveryFailure::Diverged`]); one the
    /// project authors where none of them reads is an artifact of its own.
    /// Every other project is read from the one folder its marker names, as
    /// before.
    pub fn discover_kinds(projects: &[MindProject], kinds: &[Kind]) -> Self {
        Self::discover_in(projects, kinds, Reach::Agents)
    }

    /// The same, only in the folders the projects author in.
    ///
    /// What a change to a project's own artifacts looks in
    /// ([`crate::lifecycle::find`]): never a copy in an agent's folder the
    /// project does not author in, which is foreign or an install's, and no
    /// edit, rename or removal of the project's own is Mindflayer's to make
    /// there. Each copy is its own artifact, as in every project before
    /// settings, so two of one name are two things to choose between. The
    /// same as [`discover_kinds`](Self::discover_kinds) for a project a
    /// `mind.toml` marks.
    pub fn discover_authored(projects: &[MindProject], kinds: &[Kind]) -> Self {
        Self::discover_in(projects, kinds, Reach::Authored)
    }

    /// The two above.
    fn discover_in(projects: &[MindProject], kinds: &[Kind], reach: Reach) -> Self {
        let mut catalog = Catalog::default();

        for project in projects {
            for kind in kinds {
                let (mut found, mut shared) = (Vec::new(), Vec::new());
                // A kind the project keeps no folder for holds nothing: an
                // absence, like a folder nobody has made yet.
                for folder in read_from(project, *kind) {
                    if reach == Reach::Authored && !folder.authored {
                        continue;
                    }
                    for mut artifact in catalog.load(&folder.path, *kind, project.root()) {
                        artifact.placed(folder.agents.clone(), folder.authored);
                        if folder.shared && reach == Reach::Agents {
                            shared.push(artifact);
                        } else {
                            found.push(artifact);
                        }
                    }
                }
                found.extend(catalog.gather_copies(shared));
                // By path, as one folder is read, so two of one name in one
                // project come in the same order wherever they were found.
                found.sort_by(|a, b| a.path().cmp(b.path()));
                catalog.artifacts.extend(found);
            }
        }

        catalog.settle();
        catalog
    }

    /// Every folder discovery reads `kind` from in `project`, in the order
    /// it reads them, whether or not each is there.
    ///
    /// For a project marked by its settings, the folder each of its agents
    /// is sent a copy in, then the one it authors in, unless that is one of
    /// them: one folder named twice is read once, so a skill in it is listed
    /// once, whatever the ledger says of it. For any other, the one folder its
    /// marker names. None for a kind it keeps nowhere. What a front end names
    /// to say where it looked.
    pub fn folders(project: &MindProject, kind: Kind) -> Vec<PathBuf> {
        read_from(project, kind)
            .into_iter()
            .map(|folder| folder.path)
            .collect()
    }

    /// Every artifact of `kind` in the folder `root`, loaded, with what could
    /// not be collected as a failure.
    fn load(&mut self, root: &Path, kind: Kind, project: &Path) -> Vec<Artifact> {
        match kind.layout() {
            Layout::Directory { manifest } => {
                // Skill is the only directory-shaped kind, so this arm names
                // its constructor directly. A second one would add a match
                // here, which is the point of a closed enum: the compiler
                // asks.
                match kind {
                    Kind::Skill => self.load_directories(root, manifest, project),
                    Kind::Rule => unreachable!("a rule is not a directory"),
                }
            }
            Layout::Files { extension } => self.load_files(root, extension, project),
        }
    }

    /// One artifact for each name among `copies`, all of one kind in one
    /// project, read from folders its agents read: its first copy by path,
    /// with every other taken in, and a failure when they do not all hold
    /// the same.
    fn gather_copies(&mut self, copies: Vec<Artifact>) -> Vec<Artifact> {
        let mut gathered = Vec::new();
        let mut named: BTreeMap<String, Vec<Artifact>> = BTreeMap::new();
        for copy in copies {
            named.entry(copy.name().to_owned()).or_default().push(copy);
        }
        for (name, mut copies) in named {
            copies.sort_by(|a, b| a.path().cmp(b.path()));
            let mut copies = copies.into_iter();
            let Some(mut first) = copies.next() else {
                continue;
            };
            let mut hashed = None;
            let mut paths = vec![first.path().to_path_buf()];
            let mut differ = false;
            for other in copies {
                paths.push(other.path().to_path_buf());
                let differs = self.differs(&name, first.path(), &mut hashed, other.path());
                differ |= differs;
                first.take_copy(other, differs);
            }
            if differ {
                self.failures.push(DiscoveryFailure::Diverged {
                    kind: first.kind(),
                    name,
                    paths,
                });
            }
            gathered.push(first);
        }
        gathered
    }

    /// Whether the copy at `other` holds other names or bytes than the one at
    /// `first`, whose hash is `hashed` once worked out.
    ///
    /// By [`hash::tree`], what an install records of what it wrote, rather
    /// than by following both trees side by side: it refuses a link rather
    /// than reading wherever one points, and a copy here is anybody's. One
    /// folder reached twice, through a link to it or by a second spelling, is
    /// the same copy without reading it. A copy that cannot be hashed is said,
    /// and is not taken to differ: nothing is known either way.
    ///
    /// Every copy compared is a skill's folder: no agent is said to read a
    /// folder of rules ([`Agent::reads`]), so no rule is ever gathered here,
    /// and how two rule files would compare is M2's to say when one is.
    fn differs(
        &mut self,
        name: &str,
        first: &Path,
        hashed: &mut Option<Option<String>>,
        other: &Path,
    ) -> bool {
        if same_directory(first, other) {
            return false;
        }
        let first_hash = hashed
            .get_or_insert_with(|| match hash::tree(first) {
                Ok(hash) => Some(hash),
                Err(source) => {
                    self.failures.push(DiscoveryFailure::Uncompared {
                        name: name.to_owned(),
                        path: first.to_path_buf(),
                        source,
                    });
                    None
                }
            })
            .clone();
        let Some(first_hash) = first_hash else {
            return false;
        };
        match hash::tree(other) {
            Ok(hash) => hash != first_hash,
            Err(source) => {
                self.failures.push(DiscoveryFailure::Uncompared {
                    name: name.to_owned(),
                    path: other.to_path_buf(),
                    source,
                });
                false
            }
        }
    }

    /// What a workspace has gathered onto its shelf, of some kinds.
    ///
    /// Read from the shelf's folders rather than from the ledger, the way a
    /// project is read from its own: what is checked is what is there, so a
    /// file edited or broken since it was gathered is found as it is now. Each
    /// source has a folder of its own under the kind's, holding what it
    /// yielded the way a project's folder holds its skills.
    pub fn discover_gathered(workspace: &FlayerWorkspace, kinds: &[Kind]) -> Self {
        let mut catalog = Catalog::default();

        for kind in kinds {
            // Matched on the kind, like the refusal in `gather`: a third kind
            // has to say whether it is ever on the shelf.
            match kind {
                Kind::Skill => {}
                Kind::Rule => continue,
            }
            let Layout::Directory { manifest } = kind.layout() else {
                continue;
            };
            let Some(mut sources) = catalog.read_dir(&workspace.gathered_dir(*kind)) else {
                continue;
            };
            sources.sort();
            for source in sources {
                // Every folder, a dotted one included: the shelf is written
                // only by gathering, and a source at `./skills` is filed as
                // `.-skills`.
                if !source.is_dir() {
                    continue;
                }
                let found = catalog.load_directories(&source, manifest, workspace.root());
                catalog.artifacts.extend(found);
            }
        }

        catalog.settle();
        catalog
    }

    /// The same catalog, with every copy `ledger` records Mindflayer putting
    /// into a project `workspace` links marked as installed
    /// ([`Provenance::Installed`](crate::Provenance::Installed)).
    ///
    /// What makes a skill in a folder a project authors in its own or an
    /// install's, and one in an agent's folder an install's or somebody
    /// else's: the ledger, by the path of each copy it records
    /// ([`Ledger::copies`]), not by name, so an install of one name into one
    /// folder does not claim what somebody keeps under that name in another.
    /// Apart from discovery, which reads files and nothing else: a listing
    /// that says nothing of who put what where opens no ledger, and one that
    /// does is handed the one its workspace keeps, whose records are that
    /// workspace's alone. A project it does not link has none there.
    ///
    /// A ledger that cannot be read is a failure, said once for each project
    /// and kind it was asked about, and leaves their copies unrecorded:
    /// foreign or authored, which nothing changes, rather than installed,
    /// which an uninstall may delete.
    pub fn with_records(mut self, workspace: &FlayerWorkspace, ledger: &Ledger) -> Self {
        let mut recorded: BTreeMap<(PathBuf, Kind), BTreeSet<String>> = BTreeMap::new();
        let Catalog {
            artifacts,
            failures,
        } = &mut self;
        for artifact in artifacts.iter_mut() {
            let (root, kind) = (artifact.project().to_path_buf(), artifact.kind());
            if !workspace.is_linked(&root) {
                continue;
            }
            let copies = recorded.entry((root.clone(), kind)).or_insert_with(|| {
                let entry = install::entry_for_root(workspace, &root);
                match ledger.copies(&entry, kind) {
                    Ok(copies) => copies.into_iter().collect(),
                    Err(error) => {
                        failures.push(DiscoveryFailure::Ledger(error));
                        BTreeSet::new()
                    }
                }
            });
            artifact.record(|path| copies.contains(&install::route(&root, path)));
        }
        self.settle();
        self
    }

    /// Two catalogs as one, in the same order and with each file once.
    ///
    /// For a report over holders that are found differently (a workspace's
    /// own artifacts beside its projects'), which still has to read as one
    /// listing.
    pub fn merge(mut self, other: Catalog) -> Catalog {
        self.artifacts.extend(other.artifacts);
        self.failures.extend(other.failures);
        self.settle();
        self
    }

    /// Put everything in report order, each file once.
    fn settle(&mut self) {
        // Grouped by kind, then by name. Kind first because a mixed listing
        // reads as blocks of like things rather than an alphabetical shuffle
        // of two different sorts of thing; name second because that is what
        // the reader is scanning within a block.
        self.artifacts.sort_by(|a, b| {
            a.kind()
                .cmp(&b.kind())
                .then_with(|| a.name().cmp(b.name()))
                .then_with(|| a.project().cmp(b.project()))
        });
        // One file is one artifact, however many holders can see it: a
        // workspace whose root is also a project it manages would otherwise
        // list everything there twice.
        self.artifacts.dedup_by(|a, b| a.path() == b.path());
        self.failures.sort_by(|a, b| a.path().cmp(b.path()));
        self.failures.dedup_by(|a, b| a.path() == b.path());
    }

    /// Where every artifact of `kind` in the folder `root` is, without reading
    /// any of them, and why a folder in it could not be listed.
    ///
    /// What discovery reads, found by the walk discovery uses, for a caller
    /// that acts on the files rather than on what they declare: one moving
    /// them has to move a skill whose front matter is broken too, since it is
    /// no less the holder's for that. A folder that is not there holds none,
    /// and is no failure.
    pub fn locate(root: &Path, kind: Kind) -> (Vec<PathBuf>, Vec<DiscoveryFailure>) {
        let mut catalog = Catalog::default();
        let found = match kind.layout() {
            Layout::Directory { manifest } => catalog.directories_in(root, manifest),
            Layout::Files { extension } => catalog.files_in(root, extension),
        };
        (found, catalog.failures)
    }

    /// One artifact per immediate subdirectory holding `manifest`.
    fn load_directories(&mut self, root: &Path, manifest: &str, project: &Path) -> Vec<Artifact> {
        let mut found = Vec::new();
        for path in self.directories_in(root, manifest) {
            match Artifact::skill(path, project) {
                Ok(artifact) => found.push(artifact),
                Err(error) => self.failures.push(error.into()),
            }
        }
        found
    }

    /// Every immediate subdirectory of `root` holding `manifest`, in order.
    ///
    /// Only the immediate ones: the directory belongs to the artifact, assets
    /// and all, so walking into it would turn its own files into artifacts.
    fn directories_in(&mut self, root: &Path, manifest: &str) -> Vec<PathBuf> {
        let Some(mut directories) = self.read_dir(root) else {
            return Vec::new();
        };
        directories.sort();
        // One without the manifest is not a broken artifact: it is not one.
        // Shared assets and stray dotfolders both land there.
        directories.retain(|path| path.join(manifest).is_file());
        directories
    }

    /// One artifact per file with `extension`, at any depth under `root`.
    ///
    /// The name is the route below `root` without the extension, so folders
    /// group and mean nothing else.
    fn load_files(&mut self, root: &Path, extension: &str, project: &Path) -> Vec<Artifact> {
        let mut found = Vec::new();
        for path in self.files_in(root, extension) {
            let Some(name) = route(root, &path) else {
                self.failures.push(DiscoveryFailure::UnusableName { path });
                continue;
            };
            match Artifact::rule(&path, name, project) {
                Ok(artifact) => found.push(artifact),
                Err(error) => self.failures.push(error.into()),
            }
        }
        found
    }

    /// Every file with `extension` at any depth under `root`, in order,
    /// leaving out what is hidden.
    fn files_in(&mut self, root: &Path, extension: &str) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut files = Vec::new();

        while let Some(directory) = pending.pop() {
            let Some(entries) = self.read_dir(&directory) else {
                continue;
            };
            for path in entries {
                if is_hidden(&path) {
                    continue;
                }
                // Followed, so a rule shared by symlink is still a rule, but
                // only to decide file-or-directory. A symlinked directory is
                // not descended into, because a loop would hang discovery and
                // nothing about a rule needs one.
                let Ok(metadata) = fs::metadata(&path) else {
                    continue;
                };
                let symlinked = fs::symlink_metadata(&path)
                    .map(|meta| meta.file_type().is_symlink())
                    .unwrap_or(false);

                if metadata.is_dir() {
                    if !symlinked {
                        pending.push(path);
                    }
                } else if metadata.is_file() && has_extension(&path, extension) {
                    files.push(path);
                }
            }
        }

        files.sort();
        files
    }

    /// The entries of a directory, or nothing when it is absent.
    ///
    /// A kind's folder that does not exist is not a failure: it is a kind
    /// nobody has added anything to. One that exists and cannot be listed is.
    /// Whatever was read before a mid-walk failure is kept.
    fn read_dir(&mut self, path: &Path) -> Option<Vec<PathBuf>> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(source) => {
                self.failures.push(DiscoveryFailure::Directory {
                    path: path.to_path_buf(),
                    source,
                });
                return None;
            }
        };

        let mut found = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => found.push(entry.path()),
                Err(source) => self.failures.push(DiscoveryFailure::Directory {
                    path: path.to_path_buf(),
                    source,
                }),
            }
        }
        Some(found)
    }

    /// The artifacts found, grouped by kind and ordered by name.
    pub fn artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    /// What could not be read, ordered by path.
    pub fn failures(&self) -> &[DiscoveryFailure] {
        &self.failures
    }

    /// Both halves, owned, for a caller that passes the failures on.
    pub fn into_parts(self) -> (Vec<Artifact>, Vec<DiscoveryFailure>) {
        (self.artifacts, self.failures)
    }

    /// Whether anything at all was found.
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    /// Every kind present, in report order.
    ///
    /// What a front end needs to decide whether naming a kind tells the reader
    /// anything: with one kind in play, it does not.
    pub fn kinds(&self) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = self.artifacts.iter().map(Artifact::kind).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// Every artifact a reference names.
    ///
    /// More than one is possible and is not by itself an error: the same name
    /// in two projects is ordinary in a workspace that manages both, and one
    /// name can belong to two kinds. Which one wins is a decision for whoever
    /// is orchestrating them, so all are returned and the caller says so.
    pub fn find(&self, reference: &Reference) -> Vec<&Artifact> {
        self.artifacts
            .iter()
            .filter(|artifact| reference.matches(artifact))
            .collect()
    }
}

/// What a user typed to name one artifact.
///
/// A qualifier is separated by `:`, not `/`, and that is the whole reason the
/// two namespaces do not collide. A rule's name IS a route, as in
/// `git/no-force-push`, so a `/` qualifier would make `skills/naming` mean
/// "the skill `naming`" rather than "the rule filed under `skills/`", and a
/// rules folder grouping rules *about skills* is not an exotic thing to have.
/// With `:` a name is always a name: `rule:skills/naming` qualifies,
/// `skills/naming` does not.
///
/// It also removes the shape from Windows entirely, where a filename cannot
/// contain `:` at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    kind: Option<Kind>,
    name: String,
    typed: String,
}

/// What separates a kind from a name in a reference.
pub const QUALIFIER: char = ':';

impl Reference {
    /// Read what a user typed.
    pub fn parse(typed: &str) -> Self {
        let (kind, name) = match typed.split_once(QUALIFIER) {
            Some((head, rest)) => match head.parse::<Kind>() {
                Ok(kind) => (Some(kind), rest.to_owned()),
                Err(_) => (None, typed.to_owned()),
            },
            None => (None, typed.to_owned()),
        };
        Self {
            kind,
            name,
            typed: typed.to_owned(),
        }
    }

    /// The kind it was qualified with, if any.
    pub fn kind(&self) -> Option<Kind> {
        self.kind
    }

    /// The name half.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Exactly what the user typed.
    ///
    /// What an error quotes back. Reporting the parsed half instead would deny
    /// the existence of a name nobody typed: `mind show skill:deploy` failing
    /// with "nothing named `deploy` here" contradicts a listing that shows
    /// `deploy` right there.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// Whether this names that artifact.
    fn matches(&self, artifact: &Artifact) -> bool {
        artifact.name() == self.name && self.kind.is_none_or(|kind| kind == artifact.kind())
    }
}

/// Why an artifact, or a whole folder, could not be read, or why what was
/// read under one name is not one thing.
#[derive(Debug, Error)]
pub enum DiscoveryFailure {
    #[error("{path}: cannot be listed: {source}")]
    Directory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: its route is not valid UTF-8, so it cannot be named")]
    UnusableName { path: PathBuf },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    /// Copies of one name, in the folders a project's agents read, that do
    /// not all hold the same names and bytes. Listed all the same, as one
    /// artifact ([`Artifact::copies_differ`]).
    #[error("the copies of `{name}` at {} differ, so the agents reading them do not all load the same {kind}", spelled(.paths))]
    Diverged {
        kind: Kind,
        name: String,
        /// Every copy, the one listed first.
        paths: Vec<PathBuf>,
    },
    /// The ledger, read to say which copies were installed, could not be.
    #[error(transparent)]
    Ledger(LedgerError),
    /// A copy that could not be read through to be compared with the others
    /// of its name: one holding a link, or a file that cannot be read.
    #[error("{path}: cannot be compared with the other copies of `{name}`: {source}")]
    Uncompared {
        name: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl DiscoveryFailure {
    /// The path the failure is about: the copy listed, for copies that
    /// differ.
    pub fn path(&self) -> &Path {
        match self {
            DiscoveryFailure::Directory { path, .. }
            | DiscoveryFailure::UnusableName { path }
            | DiscoveryFailure::Uncompared { path, .. } => path,
            DiscoveryFailure::Artifact(error) => error.path(),
            DiscoveryFailure::Ledger(error) => error.path(),
            DiscoveryFailure::Diverged { paths, .. } => &paths[0],
        }
    }

    /// Whether something could not be read at all, rather than read and
    /// found to disagree with itself: what a count of unreadable files
    /// counts.
    pub fn is_unreadable(&self) -> bool {
        match self {
            DiscoveryFailure::Directory { .. }
            | DiscoveryFailure::UnusableName { .. }
            | DiscoveryFailure::Artifact(_)
            | DiscoveryFailure::Ledger(_)
            | DiscoveryFailure::Uncompared { .. } => true,
            DiscoveryFailure::Diverged { .. } => false,
        }
    }
}

/// `a`, `a and b`, `a, b and c`.
fn spelled(paths: &[PathBuf]) -> String {
    let shown: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    match shown.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// How far discovery reaches into a project marked by its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Every folder it reads: its agents' and its own.
    Agents,
    /// Only the folder it authors in.
    Authored,
}

/// One folder discovery reads a kind from in a project, and how what is in
/// it stands.
struct Folder {
    path: PathBuf,
    /// The project's agents that read it.
    agents: Vec<Agent>,
    /// Whether the project's marker names it for the kind.
    authored: bool,
    /// Whether a copy here of one name is one copy of a single artifact
    /// with every other of that name in such a folder: a folder its agents
    /// are sent copies in, or one they read.
    shared: bool,
}

/// [`Catalog::folders`], with how each stands.
fn read_from(project: &MindProject, kind: Kind) -> Vec<Folder> {
    let authored = project.directory_for(kind);
    let Some(settings) = project.settings() else {
        // A `mind.toml` names no agents: its own folder, as ever.
        return authored
            .into_iter()
            .map(|path| Folder {
                path,
                agents: Vec::new(),
                authored: true,
                shared: false,
            })
            .collect();
    };
    let agents = &settings.agents;
    let mut folders: Vec<Folder> = project
        .targets(kind)
        .into_iter()
        .map(|target| {
            let path = project.root().join(&target.path);
            Folder {
                agents: readers(project, kind, &path, agents),
                path,
                authored: false,
                shared: true,
            }
        })
        .collect();
    if let Some(authored) = authored {
        match folders
            .iter_mut()
            .find(|folder| same_directory(&folder.path, &authored))
        {
            Some(folder) => folder.authored = true,
            None => {
                let agents = readers(project, kind, &authored, agents);
                folders.push(Folder {
                    shared: !agents.is_empty(),
                    agents,
                    path: authored,
                    authored: true,
                });
            }
        }
    }
    folders
}

/// The agents among `agents` that read `kind` in `project` from `folder`,
/// each once, in report order: by where each reads ([`Agent::reads`]), not
/// where it is sent copies, since OpenCode reads Claude's folder too.
///
/// The folder is matched by its spelling, then through the filesystem, so
/// a folder named `./.claude/skills`, or reached through a link, is still
/// Claude's.
fn readers(project: &MindProject, kind: Kind, folder: &Path, agents: &[Agent]) -> Vec<Agent> {
    let mut readers: Vec<Agent> = agents
        .iter()
        .copied()
        .filter(|agent| {
            agent
                .reads(kind)
                .iter()
                .any(|read| same_directory(&project.root().join(read), folder))
        })
        .collect();
    readers.sort();
    readers.dedup();
    readers
}

/// A file's route below `root`, without its extension, `/` separated.
fn route(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let without_extension = relative.with_extension("");
    paths::to_config_string(&without_extension)
}

/// Whether a file carries an extension, whatever case it was saved in.
///
/// A case-insensitive filesystem will happily hand back `NOTES.MD`, and a rule
/// invisible because of how its name was capitalised is a bad half hour.
fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|found| found.to_str())
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Whether a path's own name starts with a dot.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}
