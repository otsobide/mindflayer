//! Copying a directory, and telling whether two are the same.
//!
//! Shared by gathering, which brings an artifact into the workspace, and
//! installing, which takes it on into a project. Both move whole directories
//! rather than files: a skill's directory belongs to the skill, scripts and
//! references included.
//!
//! [`replace`] stages a replacement beside what it replaces. A [`Stage`]
//! stages it in a scratch folder of its own instead, for when beside is
//! somewhere an agent looks.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    let old = beside(to, "old");
    let change = put(from, to, within, &beside(to, "new"), &old)?;
    if change == Change::Updated {
        fs::remove_dir_all(&old)?;
    }
    Ok(change)
}

/// Build `from` at `new` and put it at `to`, moving what `to` held to `old`
/// first, which is left for the caller to clear: what a leftover there costs
/// depends on where `old` is.
///
/// Wherever the two are, they have to be on the filesystem `to` is on, so
/// that putting the copy in place is a rename rather than a second copy that
/// could fail halfway.
fn put(from: &Path, to: &Path, within: &Path, new: &Path, old: &Path) -> Result<Change, io::Error> {
    let existed = to.exists();
    if existed && same(from, to)? {
        return Ok(Change::Unchanged);
    }
    if new.exists() {
        fs::remove_dir_all(new)?;
    }
    if let Err(error) = tree(from, new, within) {
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    // Made here rather than on the way to `new`, which need not be beside
    // `to`: a project whose skills folder is not there yet, as in a fresh
    // clone, still gets one.
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if !existed {
        if let Err(error) = moved(new, to) {
            let _ = fs::remove_dir_all(new);
            return Err(error);
        }
        return Ok(Change::Added);
    }
    // Moved aside rather than deleted first: the moment with neither copy in
    // place lasts from one rename to the next rather than a whole delete, and
    // a copy that cannot be put in place can have the old one put back.
    if old.exists() {
        fs::remove_dir_all(old)?;
    }
    if let Err(error) = moved(to, old) {
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    if let Err(error) = moved(new, to) {
        let _ = fs::rename(old, to);
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    Ok(Change::Updated)
}

/// `fs::rename`, saying what it was moving where when it fails.
///
/// A rename is what puts a whole copy in place at once, and it cannot cross
/// filesystems: a scratch folder on another one than what it replaces is an
/// error that names both, not a second copy that could fail halfway.
fn moved(from: &Path, to: &Path) -> Result<(), io::Error> {
    fs::rename(from, to).map_err(|error| {
        let why = if error.kind() == io::ErrorKind::CrossesDevices {
            format!("they are on different filesystems ({error})")
        } else {
            error.to_string()
        };
        io::Error::new(
            error.kind(),
            format!("cannot move {} to {}: {why}", from.display(), to.display()),
        )
    })
}

/// One run's own folder in a scratch folder, to build replacements in
/// somewhere other than beside what they replace.
///
/// Beside is where an agent looks. OpenCode's first line globs hidden folders
/// in a skills folder too, so a half-made `.deploy.mindflayer-new` next to
/// `deploy` is a skill it may load, and one an interrupted run leaves there it
/// loads every time after. Built in a scratch folder Mindflayer owns, a copy is out of
/// every agent's sight until a rename puts it in place whole, and what an
/// interrupted run leaves is out of sight too.
///
/// Named after the run, by its process id and a random suffix, rather than
/// after what it replaces, so two runs replacing the same thing at once never
/// build in one folder. Each run holds the scratch folder's lock shared for as
/// long as its stage lives. A run that can take that lock exclusively is alone,
/// so every stage it finds was left by a run that was interrupted, and it
/// clears them before taking its own ([`Stage::swept`]); a run that cannot
/// sweeps nothing, since what it finds may be in use. A lock rather than the
/// process ids in the names, because whether a process is still running is a
/// question each platform answers differently, and a lock released when its
/// process dies answers it on all of them.
///
/// The scratch folder has to be on the filesystem of what is replaced, for
/// the rename to be a move.
#[derive(Debug)]
pub struct Stage {
    dir: PathBuf,
    swept: Vec<PathBuf>,
    // Only ever dropped: closing it releases the lock, after `drop` has
    // removed the folder, so no sweep ever finds a stage being cleared.
    _lock: Option<File>,
}

/// The `.gitignore` a scratch folder is given, so nothing staged in it is
/// committed: `*`, as a Python venv ignores itself, inside the folder it is
/// about rather than in a team's `.gitignore`.
const IGNORE: &str = "\
# Mindflayer builds what it installs here, then moves it into place.
# Nothing in this folder is meant to be committed.
*
";

/// The file in a scratch folder every run with a stage there holds a lock on.
const LOCK: &str = ".lock";

impl Stage {
    /// Take a folder of one's own in `scratch`, making `scratch` first if it
    /// is not there, and clearing what interrupted runs left in it when no
    /// other run is using it.
    pub fn open(scratch: &Path) -> Result<Stage, io::Error> {
        fs::create_dir_all(scratch)?;
        ignore(scratch)?;
        let (lock, swept) = hold(scratch)?;
        let dir = claim(scratch)?;
        Ok(Stage {
            dir,
            swept,
            _lock: lock,
        })
    }

    /// The folder this run builds in.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// What interrupted runs had left that this stage cleared: the stages in
    /// the scratch folder when it was opened, then what earlier versions had
    /// staged beside each target it replaced.
    pub fn swept(&self) -> &[PathBuf] {
        &self.swept
    }

    /// [`replace`], building the copy in this stage instead of beside `to`.
    ///
    /// What an interrupted run of an earlier version left beside `to`, where
    /// they all staged, is cleared first ([`Stage::swept`]).
    ///
    /// Mutable, because a stage builds one copy at a time in one place.
    pub fn replace(&mut self, from: &Path, to: &Path, within: &Path) -> Result<Change, io::Error> {
        self.clear_beside(to)?;
        let old = self.dir.join("old");
        let change = put(from, to, within, &self.dir.join("new"), &old)?;
        // The new copy is in place whatever happens here, so an old one that
        // will not go is litter in the stage, not a failed install: dropping
        // the stage tries again, and the next run sweeps what that leaves.
        let _ = fs::remove_dir_all(&old);
        Ok(change)
    }

    /// Remove the `.<name>.mindflayer-new` and `-old` that an interrupted run
    /// of an earlier version left beside `to`: a skill an agent may load, in a
    /// folder no sweep looks in.
    ///
    /// Without the lock a sweep waits for, because no run of this version
    /// builds there, and runs of those versions cleared each other's anyway.
    /// One that will not go fails the replacement, as it failed theirs, rather
    /// than staying where an agent reads; one another run removed first is
    /// that run's to say.
    fn clear_beside(&mut self, to: &Path) -> Result<(), io::Error> {
        for what in ["new", "old"] {
            let left = beside(to, what);
            // A folder, not a link: no version ever staged one.
            if !fs::symlink_metadata(&left).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            match fs::remove_dir_all(&left) {
                Ok(()) => self.swept.push(left),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "cannot clear {}, left by an install that did not finish: {error}",
                            left.display()
                        ),
                    ))
                }
            }
        }
        Ok(())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Give `scratch` the `.gitignore` that keeps it out of git, unless it has
/// one. `create_new`, so one somebody changed is left as they left it.
fn ignore(scratch: &Path) -> Result<(), io::Error> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(scratch.join(".gitignore"))
    {
        Ok(mut file) => file.write_all(IGNORE.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// Hold `scratch`'s lock shared, sweeping first if it can be had exclusively.
///
/// Released between the two rather than downgraded, which `std` cannot do: a
/// run that takes the lock in between finds nothing of this one's to sweep,
/// because this one has no folder yet.
fn hold(scratch: &Path) -> Result<(Option<File>, Vec<PathBuf>), io::Error> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(scratch.join(LOCK))?;
    match lock.try_lock() {
        Ok(()) => {
            let swept = sweep(scratch);
            lock.unlock()?;
            lock.lock_shared()?;
            Ok((Some(lock), swept))
        }
        Err(TryLockError::WouldBlock) => {
            lock.lock_shared()?;
            Ok((Some(lock), Vec::new()))
        }
        // A filesystem without locks. No run there can tell a live stage from
        // a leftover, so none sweeps, and the names still keep runs apart.
        // Any error, not only `Unsupported`: NFS without its lock daemon says
        // "no locks available", which `std` leaves uncategorized, and failing
        // on it would fail every install there.
        Err(TryLockError::Error(_)) => Ok((None, Vec::new())),
    }
}

/// Remove every stage in `scratch`, saying which. Only for a run holding the
/// lock alone, for which none of them is in use.
///
/// Only what is named the way a stage is: anything else in here, somebody
/// else put here. And what cannot be removed is left for the next run rather
/// than failing this one, because it is litter in a folder nothing reads.
fn sweep(scratch: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(scratch) else {
        return Vec::new();
    };
    let mut swept = Vec::new();
    for entry in entries.flatten() {
        let is_stage = entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry.file_name().to_str().is_some_and(is_stage_name);
        if is_stage && fs::remove_dir_all(entry.path()).is_ok() {
            swept.push(entry.path());
        }
    }
    swept.sort();
    swept
}

/// How many hex digits follow the process id in a stage's name.
const SUFFIX_LEN: usize = 16;

/// Whether `name` is one [`claim`] could have given a stage.
fn is_stage_name(name: &str) -> bool {
    name.split_once('-').is_some_and(|(pid, suffix)| {
        !pid.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && suffix.len() == SUFFIX_LEN
            && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// A name for something this run makes that no other run picks: this
/// process's id and a random suffix.
///
/// The id says whose it is to anybody looking, and the suffix keeps apart
/// two runs of one process, or a process that reuses the id of one that
/// left something behind. Neither makes the name safe to take on its own:
/// whatever is made under it is made with `create_new` or `create_dir`, so
/// a name that is somehow there already is refused rather than shared.
pub(crate) fn unique_name() -> String {
    format!(
        "{}-{:0width$x}",
        std::process::id(),
        random(),
        width = SUFFIX_LEN
    )
}

/// Make a folder in `scratch` no other run has, named by [`unique_name`] and
/// made with `create_dir`, which refuses a name that is already there rather
/// than sharing it.
fn claim(scratch: &Path) -> Result<PathBuf, io::Error> {
    let mut attempts = 0;
    loop {
        let dir = scratch.join(unique_name());
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 8 => {
                attempts += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// A number nobody can guess, without a dependency for it: `std` draws its
/// hashers' keys from the operating system. The count keeps two calls in one
/// process apart even if those keys were not random.
fn random() -> u64 {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(CALLS.fetch_add(1, Ordering::Relaxed));
    hasher.finish()
}

/// A hidden sibling of `to`: where [`replace`] stages, and where installs
/// staged before [`Stage`].
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

#[cfg(test)]
#[path = "copy_test.rs"]
mod tests;
