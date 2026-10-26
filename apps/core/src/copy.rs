//! Copying a directory, and telling whether two are the same.
//!
//! Shared by gathering, which brings an artifact into the workspace, and
//! installing, which takes it on into a project. Both move whole directories
//! rather than files: a skill's directory belongs to the skill, scripts and
//! references included.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Copy a directory and everything under it.
///
/// A symlink inside is copied as what it points at — what leaves a clone has
/// to keep working once that clone is replaced — but only when it points
/// somewhere inside `within` (the clone a skill was gathered from, the
/// workspace an installed one comes from) or inside `from` itself. One that
/// points out of both is refused, not followed, because following it is how a
/// key in `~/.ssh` or a `.env` ends up copied into a project that is about to
/// be committed. A link back into a directory already being copied is
/// refused too, rather than copied until the path is too long.
pub fn tree(from: &Path, to: &Path, within: &Path) -> Result<(), io::Error> {
    let bounds = [fs::canonicalize(within)?, fs::canonicalize(from)?];
    let mut path = Vec::new();
    copy_tree(from, to, &bounds, &mut path)
}

fn copy_tree(
    from: &Path,
    to: &Path,
    bounds: &[PathBuf; 2],
    path: &mut Vec<PathBuf>,
) -> Result<(), io::Error> {
    path.push(fs::canonicalize(from)?);
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let pointed = fs::canonicalize(&source)?;
            if !bounds.iter().any(|bound| pointed.starts_with(bound)) {
                return Err(io::Error::other(format!(
                    "{} links to {}, outside {}, so it is not copied",
                    source.display(),
                    pointed.display(),
                    bounds[0].display()
                )));
            }
            if pointed.is_dir() {
                if path.iter().any(|copying| copying.starts_with(&pointed)) {
                    return Err(io::Error::other(format!(
                        "{} links back into a directory it is being copied from, so it is not copied",
                        source.display()
                    )));
                }
                copy_tree(&pointed, &target, bounds, path)?;
            } else {
                fs::copy(&pointed, &target)?;
            }
        } else if kind.is_dir() {
            copy_tree(&source, &target, bounds, path)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    path.pop();
    Ok(())
}

/// Replace `to` with `from`, saying whether anything actually changed.
///
/// Replaced rather than merged, so a file the source no longer has does not
/// survive as a leftover of an older revision. An identical directory is left
/// alone entirely, down to its modification times, which is what lets a second
/// run report what moved rather than reporting everything.
///
/// All or nothing: the copy is made beside `to` and only put in its place
/// once it is whole, so a copy that fails — a refused link, a full disk —
/// leaves `to` exactly as it was, and leaves nothing where there was nothing.
/// `within` bounds the symlinks followed, as for [`tree`].
pub fn replace(from: &Path, to: &Path, within: &Path) -> Result<Change, io::Error> {
    let existed = to.exists();
    if existed && same(from, to)? {
        return Ok(Change::Unchanged);
    }
    let staging = beside(to, "new");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    if let Err(error) = tree(from, &staging, within) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if !existed {
        fs::rename(&staging, to)?;
        return Ok(Change::Added);
    }
    // Moved aside rather than deleted first, so there is never a moment
    // with neither the old copy nor the new one in place.
    let old = beside(to, "old");
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    fs::rename(to, &old)?;
    if let Err(error) = fs::rename(&staging, to) {
        let _ = fs::rename(&old, to);
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    fs::remove_dir_all(&old)?;
    Ok(Change::Updated)
}

/// A hidden sibling of `to`, for staging a replacement.
fn beside(to: &Path, what: &str) -> PathBuf {
    let name = to
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    to.with_file_name(format!(".{name}.mindflayer-{what}"))
}

/// What replacing a directory turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Updated,
    Unchanged,
}

/// Whether two directories hold the same names and the same bytes.
pub fn same(a: &Path, b: &Path) -> Result<bool, io::Error> {
    let (left, right) = (listing(a)?, listing(b)?);
    if left != right {
        return Ok(false);
    }
    for name in left {
        let (one, two) = (a.join(&name), b.join(&name));
        if one.is_dir() {
            if !same(&one, &two)? {
                return Ok(false);
            }
        } else if fs::read(&one)? != fs::read(&two)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The sorted names directly inside a directory.
fn listing(path: &Path) -> Result<Vec<OsString>, io::Error> {
    let mut names: Vec<OsString> = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort();
    Ok(names)
}
