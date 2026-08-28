//! Copying a directory, and telling whether two are the same.
//!
//! Shared by gathering, which brings an artifact into the workspace, and
//! installing, which takes it on into a project. Both move whole directories
//! rather than files: a skill's directory belongs to the skill, scripts and
//! references included.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

/// Copy a directory and everything under it.
///
/// A symlink inside is copied as the file it points at: what leaves a clone
/// has to keep working once that clone is replaced.
pub fn tree(from: &Path, to: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            tree(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

/// Replace `to` with `from`, saying whether anything actually changed.
///
/// Replaced rather than merged, so a file the source no longer has does not
/// survive as a leftover of an older revision. An identical directory is left
/// alone entirely, down to its modification times, which is what lets a second
/// run report what moved rather than reporting everything.
pub fn replace(from: &Path, to: &Path) -> Result<Change, io::Error> {
    if !to.exists() {
        tree(from, to)?;
        return Ok(Change::Added);
    }
    if same(from, to)? {
        return Ok(Change::Unchanged);
    }
    fs::remove_dir_all(to)?;
    tree(from, to)?;
    Ok(Change::Updated)
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
