//! One file of an artifact, read for a front end that shows it where a file
//! must never run: the web panel, which asks for it over the local server
//! (#67).
//!
//! What a skill holds is somebody else's: gathered from a repository, or put
//! in an agent's folder by another tool. A `SKILL.md` can carry a `<script>`,
//! an asset can be an HTML page or an SVG with a script inside, and a link
//! can lead anywhere on the disk. So a file reaches a front end only as
//! this module reads it:
//!
//! - **As text inside JSON, or not at all.** [`Contents`] holds the text as
//!   a string, which a front end puts on a page as text; nothing here says
//!   what type of file it is, so nothing downstream can serve it as one, and
//!   no file is ever handed over as its own bytes. A file that is not UTF-8,
//!   or holds a NUL, is said to be binary, with its size, and its bytes stay
//!   where they are.
//! - **Held to its holder once its links are followed**, by the check a copy
//!   makes of every link it meets ([`copy::followed`]): a file, or a folder
//!   on the way to it, that leads out of the project it belongs to, or for
//!   the workspace's own out of the folder it keeps that kind in, is
//!   refused, never read. Before that, the file is named by a route of plain
//!   names from the artifact, so a `..` never gets as far as the disk.
//! - **No bigger than [`CAP`]**: one larger is said to be too large, with its
//!   size, and never read whole, so a browser cannot make the server read a
//!   disk image into memory by naming one.
//!
//! Rules in core, as every rule about a path is: the server carries what this
//! answers and the panel draws it, and neither could disagree with the other
//! about what may be read.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::slice;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::catalog::{Catalog, Reference};
use crate::code::codes;
use crate::copy;
use crate::install;
use crate::kind::{Kind, Layout};
use crate::lifecycle::{self, LifecycleError};
use crate::paths;
use crate::workspace::{FlayerWorkspace, MindProject};

/// The most a file may hold, in bytes, to have its text read: a mebibyte.
///
/// Room for any `SKILL.md` and the references beside it, which the Agent
/// Skills format asks to keep short, and little enough that every answer
/// is quick to build, send and draw. A file larger than this is no file a
/// person reads in a browser.
pub const CAP: u64 = 1 << 20;

/// One file of an artifact, as a front end may show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Contents {
    /// The artifact it belongs to.
    pub kind: Kind,
    pub name: String,
    /// The file, as it was asked for: a route from the artifact's folder,
    /// `/` separated, or a rule's own file name.
    pub file: String,
    /// How many bytes it holds.
    pub size: u64,
    /// What it says, when it is UTF-8 text no larger than [`CAP`]; none
    /// otherwise, and [`withheld`](Self::withheld) says why.
    pub text: Option<String>,
    /// Why there is no text, when there is none.
    pub withheld: Option<Withheld>,
}

/// Why a file's text is not handed over, though the file was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withheld {
    /// It is not UTF-8, or it holds a NUL: an image, an archive, a program.
    /// Text a browser could show would be a guess at what the bytes mean.
    Binary,
    /// It holds more than [`CAP`] bytes.
    TooLarge,
}

impl Withheld {
    /// How JSON says it: one word, which once printed is a contract.
    pub const fn slug(self) -> &'static str {
        match self {
            Withheld::Binary => "binary",
            Withheld::TooLarge => "too_large",
        }
    }
}

/// By its slug, for the reason a kind is: a renamed variant cannot change
/// what a front end compares.
impl Serialize for Withheld {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

/// Why a file was not read at all.
#[derive(Debug, Error)]
pub enum ContentsError {
    /// The reference names nothing the holder holds, or more than one thing.
    #[error(transparent)]
    Find(#[from] LifecycleError),
    #[error("`{file}` names no file of {artifact}: a file is named by its route from the artifact's folder, each part a plain name, and a rule kept in one file by that file's name")]
    Route { file: String, artifact: String },
    #[error("{path} leads to {resolved}, outside {holder}, so it is not read")]
    Outside {
        path: PathBuf,
        resolved: PathBuf,
        holder: PathBuf,
    },
    #[error("{path}: no such file")]
    Missing { path: PathBuf },
    #[error("{path}: not a file")]
    NotAFile { path: PathBuf },
    #[error("{path}: cannot be read: {source}")]
    Unreadable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(ContentsError {
    Self::Route { .. } => "contents_route_unfit",
    Self::Outside { .. } => "contents_outside",
    Self::Missing { .. } => "contents_missing",
    Self::NotAFile { .. } => "contents_not_a_file",
    Self::Unreadable { .. } => "contents_unreadable",
} carried {
    Self::Find(error) => error,
});

impl Contents {
    /// `file` of the artifact `reference` names among `workspace`'s own: the
    /// library a snapshot lists, of the kinds it keeps apart from a project
    /// at its root ([`FlayerWorkspace::own_kinds`]).
    ///
    /// Held to the folder the workspace keeps the artifact's kind in, not to
    /// its root as an install from it is: the default workspace's root is a
    /// home, where every secret its user keeps would count as inside it.
    pub fn of_library(
        workspace: &FlayerWorkspace,
        reference: &str,
        file: &str,
    ) -> Result<Contents, ContentsError> {
        read(
            &workspace.own(),
            &workspace.own_kinds(),
            Within::Kept,
            reference,
            file,
        )
    }

    /// `file` of the artifact `reference` names in `project`, wherever a
    /// snapshot lists it from: authored, an install's copy, or somebody's in
    /// one of its agents' folders.
    ///
    /// Held to the project's root, as an install from it holds a copy: a
    /// project is a repository, and a skill in it may link to a file
    /// elsewhere in it.
    pub fn of_project(
        project: &MindProject,
        reference: &str,
        file: &str,
    ) -> Result<Contents, ContentsError> {
        read(project, &Kind::ALL, Within::Root, reference, file)
    }
}

/// What a file is held to once its links are followed.
#[derive(Clone, Copy)]
enum Within {
    /// The holder's root.
    Root,
    /// The folder the holder keeps the artifact's kind in.
    Kept,
}

/// `file` of the one artifact of `kinds` that `reference` names in `holder`,
/// held `within` it.
fn read(
    holder: &MindProject,
    kinds: &[Kind],
    within: Within,
    reference: &str,
    file: &str,
) -> Result<Contents, ContentsError> {
    let reference = Reference::parse(reference);
    let (kind, name, at) = artifact(holder, kinds, &reference)?;
    let path = place(kind, &at, file).ok_or_else(|| ContentsError::Route {
        file: file.to_owned(),
        artifact: format!("{}{}{name}", kind.slug(), crate::catalog::QUALIFIER),
    })?;

    // A `Kept` read never falls back to the root in fact: every kind has a
    // folder in the workspace's own (`FlayerWorkspace::own`), and the
    // artifact was found in its kind's.
    let bound = match within {
        Within::Root => None,
        Within::Kept => holder.directory_for(kind),
    }
    .unwrap_or_else(|| holder.root().to_path_buf());
    let bound = fs::canonicalize(&bound).map_err(|source| ContentsError::Unreadable {
        path: bound,
        source,
    })?;
    let (resolved, inside) =
        copy::followed(&path, slice::from_ref(&bound)).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => ContentsError::Missing { path: path.clone() },
            _ => ContentsError::Unreadable {
                path: path.clone(),
                source,
            },
        })?;
    if !inside {
        return Err(ContentsError::Outside {
            path,
            resolved,
            holder: bound,
        });
    }

    // The resolved path from here on, never the one asked for: a link put
    // in its place after the check would otherwise be followed by the read.
    let unreadable = |source| ContentsError::Unreadable {
        path: path.clone(),
        source,
    };
    let metadata = fs::metadata(&resolved).map_err(unreadable)?;
    if !metadata.is_file() {
        return Err(ContentsError::NotAFile { path });
    }
    let withheld = |size, withheld| Contents {
        kind,
        name: name.clone(),
        file: file.to_owned(),
        size,
        text: None,
        withheld: Some(withheld),
    };
    if metadata.len() > CAP {
        return Ok(withheld(metadata.len(), Withheld::TooLarge));
    }
    // One byte past the cap, so a file that grew since it was measured is
    // caught rather than cut short and passed off as whole.
    let mut bytes = Vec::new();
    File::open(&resolved)
        .and_then(|opened| opened.take(CAP + 1).read_to_end(&mut bytes))
        .map_err(unreadable)?;
    let size = bytes.len() as u64;
    if size > CAP {
        return Ok(withheld(size, Withheld::TooLarge));
    }
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => Ok(Contents {
            kind,
            name,
            file: file.to_owned(),
            size,
            text: Some(text),
            withheld: None,
        }),
        _ => Ok(withheld(size, Withheld::Binary)),
    }
}

/// The kind and name of the one artifact `reference` names in `holder`, and
/// where it is: the folder a skill is, or the file a rule is.
///
/// As a snapshot lists it, which takes in an artifact whose file does not
/// read under the name its place gives it ([`lifecycle::identify`]): that
/// file is the one most in need of being looked at.
fn artifact(
    holder: &MindProject,
    kinds: &[Kind],
    reference: &Reference,
) -> Result<(Kind, String, PathBuf), LifecycleError> {
    let kinds: Vec<Kind> = kinds
        .iter()
        .copied()
        .filter(|kind| reference.kind().is_none_or(|asked| asked == *kind))
        .collect();
    let catalog = Catalog::discover_kinds(slice::from_ref(holder), &kinds);
    let mut found: Vec<(Kind, String, PathBuf)> = catalog
        .find(reference)
        .into_iter()
        .map(|artifact| {
            (
                artifact.kind(),
                artifact.name().to_owned(),
                artifact.path().to_path_buf(),
            )
        })
        .collect();
    for failure in catalog.failures() {
        let Some((kind, name)) = lifecycle::identify(holder, failure.path()) else {
            continue;
        };
        if name != reference.name() || !kinds.contains(&kind) {
            continue;
        }
        let Some(at) = lifecycle::place(holder, kind, &name) else {
            continue;
        };
        if !found.iter().any(|(_, _, path)| *path == at) {
            found.push((kind, name, at));
        }
    }

    match found.len() {
        0 => Err(LifecycleError::NotFound {
            typed: reference.typed().to_owned(),
        }),
        1 => Ok(found.remove(0)),
        _ => Err(LifecycleError::Ambiguous {
            typed: reference.typed().to_owned(),
            matches: found
                .iter()
                .map(|(kind, name, path)| {
                    let at =
                        paths::relative_to(path, holder.root()).unwrap_or_else(|| path.clone());
                    format!(
                        "{}{}{name} at {}",
                        kind.slug(),
                        crate::catalog::QUALIFIER,
                        at.display()
                    )
                })
                .collect(),
        }),
    }
}

/// Where `file` is in an artifact of `kind` at `at`; none when it names no
/// file of it.
///
/// A route of plain names for a skill, each one folder or file name, so a
/// `..`, a root or an empty part never reaches the disk; a rule is one
/// file, so its own file name is the one route it has. Joined a part at a
/// time, so a `/` means the same on every platform.
fn place(kind: Kind, at: &Path, file: &str) -> Option<PathBuf> {
    match kind.layout() {
        Layout::Directory { .. } => file.split('/').all(install::is_folder_name).then(|| {
            file.split('/')
                .fold(at.to_path_buf(), |path, part| path.join(part))
        }),
        Layout::Files { .. } => {
            (at.file_name() == Some(OsStr::new(file))).then(|| at.to_path_buf())
        }
    }
}

#[cfg(test)]
#[path = "contents_test.rs"]
mod tests;
