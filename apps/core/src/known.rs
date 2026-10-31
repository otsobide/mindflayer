//! The flayer workspaces this machine knows: a `known` array in the default
//! workspace's marker, the one in home, so switching between several flayers
//! has somewhere to read them from.
//!
//! In the default workspace rather than a file of its own beside it: home is
//! the one place every flayer on a machine can find, its marker is already
//! edited a key at a time (`edit_array`), under its lock, with every comment
//! and every other key kept, and a list a person may read and prune by hand
//! belongs beside the `projects` and `loaded` it reads like. A file of its
//! own would be one more format to version and lock. The ledger would hide
//! it from that person, and would have to be made to hold it.
//!
//! Never made for the occasion. A default workspace nothing has made yet
//! remembers nothing, and `flayer init` making one to remember another in
//! would write a marker into somebody's home for a list nobody has asked to
//! read: the rule every read follows ([`crate::resolve::Intent::Read`]).
//! The default workspace is never in its own list either. It is listed for
//! being the default, whatever `known` says.
//!
//! What remembers is `flayer init`, of each workspace it makes and of one
//! already there, which it otherwise leaves as it was: so a workspace made
//! before the default one, or by a build from before this list, is one
//! idempotent command from being known. Activating one remembers nothing,
//! since activating writes nothing (`docs/architecture.md`, "The workspaces
//! this machine knows").

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::paths;
use crate::resolve::Reason;
use crate::workspace::{
    absolute, same_directory, FlayerWorkspace, Registration, WorkspaceError, FLAYER_CONFIG,
    FLAYER_DIR,
};

/// Every workspace this machine knows, as `flayer workspaces` lists them.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Known {
    /// The default workspace first, once it is made; then each one its
    /// `known` names, in the order it names them; then the one in use, when
    /// it is none of those. Each directory once, however many spellings
    /// point at it, as every report lists a project once.
    pub workspaces: Vec<KnownWorkspace>,
}

/// One workspace this machine knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct KnownWorkspace {
    /// What its marker calls it; none for one that would not open, which has
    /// no marker to read.
    pub name: Option<String>,
    /// Where it is.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The default workspace, in home: listed for being the default,
    /// whatever its own `known` says.
    pub default: bool,
    /// Whether the default workspace's `known` names it, which is what
    /// `flayer workspaces forget` takes out. False for the default itself,
    /// and for the one in use when nothing remembers it.
    pub remembered: bool,
    /// The one in use where the list was asked for, as `resolve` chose it.
    pub in_use: bool,
    /// How many projects it links, each directory once, stale ones among
    /// them: its own count, which says which of several flayers is which.
    pub projects: usize,
    /// How many of those would not open.
    pub stale_projects: usize,
    /// Why it would not open, when it would not: listed rather than left
    /// out, since a workspace that has gone is the one somebody has to find
    /// to forget it.
    ///
    /// Written through [`redacted`](crate::url::redacted), as an error's
    /// message is in a [`Said`](crate::code::Said): a marker that does not
    /// parse is quoted in its own words, a source's URL among them perhaps,
    /// and whoever wrote that file is no promise that it holds no
    /// credential.
    #[serde(serialize_with = "crate::url::serialize_redacted_option")]
    pub stale: Option<String>,
}

impl KnownWorkspace {
    /// A workspace that opened, with its projects counted.
    fn of(workspace: &FlayerWorkspace) -> Self {
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut stale_projects = 0;
        for member in workspace.members() {
            if roots.iter().any(|root| same_directory(root, &member.root)) {
                continue;
            }
            if member.project.is_err() {
                stale_projects += 1;
            }
            roots.push(member.root);
        }
        Self {
            name: Some(workspace.name().to_owned()),
            path: workspace.root().to_path_buf(),
            default: false,
            remembered: false,
            in_use: false,
            projects: roots.len(),
            stale_projects,
            stale: None,
        }
    }

    /// One at `path` that would not open, and why.
    fn gone(path: PathBuf, error: &WorkspaceError) -> Self {
        Self {
            name: None,
            path,
            default: false,
            remembered: false,
            in_use: false,
            projects: 0,
            stale_projects: 0,
            stale: Some(error.to_string()),
        }
    }
}

/// Every workspace the default one at `home` knows, itself first, with
/// `in_use` marked, or listed after them when nothing remembers it; and
/// each that would not open, as a failure beside the list.
///
/// A read: the default workspace is opened only once it is made, and never
/// made for the occasion, and nothing is written. One that has gone is
/// listed with why rather than raised, so the rest are still shown. An
/// `in_use` that is not made, the default before anything has made it, is
/// no workspace to mark.
pub fn known(
    home: Option<&Path>,
    in_use: Option<&FlayerWorkspace>,
) -> (Known, Vec<WorkspaceError>) {
    let mut workspaces: Vec<KnownWorkspace> = Vec::new();
    let mut failures = Vec::new();
    match made_default(home) {
        Ok(Some(default)) => {
            workspaces.push(KnownWorkspace {
                default: true,
                ..KnownWorkspace::of(&default)
            });
            for entry in default.remembered() {
                if let Some(listed) = workspaces
                    .iter_mut()
                    .find(|listed| same_directory(&listed.path, &entry.root))
                {
                    listed.remembered = true;
                    continue;
                }
                let listed = match entry.workspace {
                    Ok(workspace) => KnownWorkspace::of(&workspace),
                    Err(error) => {
                        let gone = KnownWorkspace::gone(entry.root, &error);
                        failures.push(error);
                        gone
                    }
                };
                workspaces.push(KnownWorkspace {
                    remembered: true,
                    ..listed
                });
            }
        }
        Ok(None) => {}
        // Its marker is there but will not open, newer or malformed: listed
        // as the default that would not, since what it knows cannot be read.
        Err(error) => {
            let path = home.map(Path::to_path_buf).unwrap_or_default();
            workspaces.push(KnownWorkspace {
                default: true,
                ..KnownWorkspace::gone(absolute(&path).unwrap_or(path), &error)
            });
            failures.push(error);
        }
    }
    if let Some(in_use) = in_use.filter(|workspace| workspace.is_made()) {
        match workspaces
            .iter_mut()
            .find(|listed| same_directory(&listed.path, in_use.root()))
        {
            Some(listed) => listed.in_use = true,
            None => workspaces.push(KnownWorkspace {
                in_use: true,
                ..KnownWorkspace::of(in_use)
            }),
        }
    }
    (Known { workspaces }, failures)
}

/// Where a front end that stays on one workspace was launched: the
/// workspace it resolved there, and the rule that chose it.
///
/// No activated workspace beside it: an activation wins over walking up and
/// over the default, so whenever the front end read one it is this one,
/// and a `-W` given at launch switches the activation off, as it does for
/// every command.
#[derive(Debug, Clone, Copy)]
pub struct Launch<'a> {
    pub root: &'a Path,
    pub reason: Reason,
}

/// A workspace a front end that stays on one may switch to, as
/// [`switchable`] lists it: the row `flayer workspaces` prints, with how it
/// stands with the front end.
///
/// Beside the row rather than in it, as [`crate::view::KnownHere`] stands
/// beside it: the row is the machine's list, the same whoever asks, and
/// which one a front end was launched on is the front end's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Switchable {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub workspace: KnownWorkspace,
    /// Whether it is the one the front end was launched on.
    pub launched: bool,
    /// The rule it is chosen by once switched to, as a snapshot of it says:
    /// the launch's, for the one the front end was launched on, and named,
    /// as `-W` names it, for every other, since a switch names it to every
    /// line the front end runs from then on, and nothing else chose it.
    pub reason: Reason,
    /// Whether a switch to it is offered: it opens, it is not the one in
    /// use, and its path is text a command line can hold, since every line
    /// names the workspace by it.
    pub switchable: bool,
}

/// Every workspace a front end launched as `launch` says, and on
/// `in_use` now, may switch to: the ones this machine knows, as [`known`]
/// lists them, `in_use` among them, then `in_use` when it is not made and
/// none of those, and the one it was launched on when it is none of them
/// either.
///
/// An `in_use` that is not made is the default before anything has made
/// it, listed as it will be; or, anywhere else, one gone or broken since
/// the front end went to it, which it hands over unmade, listed with why
/// it does not open, as `known` lists one it remembers, and never as a
/// workspace holding nothing, which is all an unmade one says of itself.
///
/// The one place a front end that stays on one workspace learns where it
/// may go instead, so it offers no other, and is held to the same list when
/// asked to go ([`crate::policy::switched`]): a path it was sent is matched
/// against these, never opened as given. A read, as `known` is: nothing is
/// made, and a workspace that does not open is a row saying why, offered
/// nothing.
pub fn switchable(
    home: Option<&Path>,
    launch: &Launch<'_>,
    in_use: &FlayerWorkspace,
) -> Vec<Switchable> {
    let (known, _) = known(home, Some(in_use));
    let mut rows = known.workspaces;
    let listed = |rows: &[KnownWorkspace], root: &Path| {
        rows.iter().any(|row| same_directory(&row.path, root))
    };
    // `known` marks nothing unmade, but a front end on one is on it all the
    // same, and says which it is on.
    if !in_use.is_made() {
        let root = in_use.root();
        match rows.iter_mut().find(|row| same_directory(&row.path, root)) {
            Some(row) => row.in_use = true,
            None => {
                let default = home.is_some_and(|home| same_directory(home, root));
                let row = if default {
                    KnownWorkspace::of(in_use)
                } else {
                    match FlayerWorkspace::open(root) {
                        Ok(workspace) => KnownWorkspace::of(&workspace),
                        Err(error) => KnownWorkspace::gone(root.to_path_buf(), &error),
                    }
                };
                rows.push(KnownWorkspace {
                    default,
                    in_use: true,
                    ..row
                });
            }
        }
    }
    if !listed(&rows, launch.root) {
        rows.push(match FlayerWorkspace::open(launch.root) {
            Ok(workspace) => KnownWorkspace::of(&workspace),
            Err(error) => KnownWorkspace::gone(launch.root.to_path_buf(), &error),
        });
    }
    rows.into_iter()
        .map(|row| {
            let launched = same_directory(&row.path, launch.root);
            let switchable = row.stale.is_none() && !row.in_use && row.path.to_str().is_some();
            Switchable {
                launched,
                reason: if launched {
                    launch.reason
                } else {
                    Reason::Explicit
                },
                switchable,
                workspace: row,
            }
        })
        .collect()
}

/// The default workspace at `home`, when there is one and it is made.
///
/// Opened only when its marker is there: a read of an unmade one would
/// answer from the template, which knows no workspace, and a write must not
/// make it.
fn made_default(home: Option<&Path>) -> Result<Option<FlayerWorkspace>, WorkspaceError> {
    let Some(home) = home else {
        return Ok(None);
    };
    let home = absolute(home)?;
    if !home.join(FLAYER_DIR).join(FLAYER_CONFIG).is_file() {
        return Ok(None);
    }
    FlayerWorkspace::open(&home).map(Some)
}

/// Remember `workspace` in the default workspace at `home`: what `flayer
/// init` does with each workspace it makes, or finds already made.
///
/// `None` when nothing is written: there is no default (`home` is `None`),
/// it is not made yet, or `workspace` is the default one itself. Otherwise
/// the entry it is known by, and whether this added it or it was there
/// already, by this spelling or another.
pub fn remember(
    home: Option<&Path>,
    workspace: &FlayerWorkspace,
) -> Result<Option<(PathBuf, Registration)>, WorkspaceError> {
    let Some(mut default) = made_default(home)? else {
        return Ok(None);
    };
    if same_directory(default.root(), workspace.root()) {
        return Ok(None);
    }
    default.remember(workspace).map(Some)
}

/// Forget the workspace at `root` in the default workspace at `home`,
/// returning every entry that pointed at it, each spelling of it included.
///
/// A path rather than a workspace, since the entry most worth forgetting is
/// one whose workspace has gone. Refused, as `unlink` is, when nothing
/// remembers it: a typo far more often than a no-op. With no default made,
/// nothing remembers anything, which is the same refusal.
pub fn forget(home: &Path, root: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
    match made_default(Some(home))? {
        Some(mut default) => default.forget(root),
        None => Err(WorkspaceError::NotKnown {
            path: root.to_path_buf(),
            workspace: absolute(home)?,
        }),
    }
}
