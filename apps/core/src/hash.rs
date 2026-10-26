//! The content hash of a copy: what the ledger keeps of what an install
//! wrote, so that a copy nobody has touched can later be told from one
//! somebody edited (D-04).
//!
//! Defined here and nowhere else. Every target of an installation is compared
//! with the one hash recorded for it, on whichever machine the comparison
//! runs, so two spellings of the recipe (or one that depended on the
//! platform) would call an untouched copy edited.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The SHA-256 of everything under `root`, as 64 lowercase hex digits.
///
/// What is hashed is each folder and file under `root`, in the order of
/// their paths relative to it (names joined by `/`, compared as bytes), each
/// written as:
///
/// - a folder: `d`, its path, a NUL byte;
/// - a file: `f`, its path, a NUL byte, its length as eight big-endian bytes,
///   then its bytes.
///
/// `root`'s own name is not part of it, so the same copy filed in two agents'
/// folders hashes alike in both. Paths use `/` on every platform and are
/// sorted rather than taken in the order a directory lists them, which is the
/// filesystem's own and differs between ext4, APFS and NTFS. No name can
/// contain a NUL byte, and every file says how long it is, so where one entry
/// ends is never in doubt and two different trees never write the same bytes.
/// A name is its UTF-8 wherever it is valid Unicode, which every name on every
/// platform the CI runs is, and otherwise the bytes the platform holds
/// ([`std::ffi::OsStr::as_encoded_bytes`]), so two names never read as one.
///
/// Folders count, empty ones included, so two trees hash alike exactly when
/// [`crate::copy::same`] says they hold the same names and bytes. Permissions
/// and times do not: Windows has no executable bit to agree on, and a time
/// changes when nothing in the file has.
///
/// A link, or anything else that is neither a file nor a folder, is refused
/// rather than followed. Following it could read anything (a link to
/// `/dev/zero` never ends), and the text of a link is spelled differently on
/// Windows. Mindflayer never writes one into a copy, since [`crate::copy::tree`]
/// copies what a link points at, so one found there was put there afterwards.
pub fn tree(root: &Path) -> Result<String, io::Error> {
    let mut entries = Vec::new();
    walk(root, &[], &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let mut hasher = Sha256::new();
    for entry in entries {
        match entry.file {
            None => {
                hasher.update(b"d");
                hasher.update(&entry.path);
                hasher.update([0]);
            }
            Some(file) => {
                // Read whole, so the length written is the length of the bytes
                // hashed even if the file changes while it is being read.
                let bytes = fs::read(&file)?;
                hasher.update(b"f");
                hasher.update(&entry.path);
                hasher.update([0]);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(&bytes);
            }
        }
    }

    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        // Writing to a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// One folder or file found under the root.
struct Entry {
    /// Its path relative to the root, `/` separated.
    path: Vec<u8>,
    /// Where to read it, for a file; `None` for a folder.
    file: Option<PathBuf>,
}

fn walk(dir: &Path, prefix: &[u8], entries: &mut Vec<Entry>) -> Result<(), io::Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let mut path = prefix.to_vec();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(entry.file_name().as_encoded_bytes());

        // The entry's own type, which a link does not hand on to what it
        // points at.
        let kind = entry.file_type()?;
        if kind.is_dir() {
            entries.push(Entry {
                path: path.clone(),
                file: None,
            });
            walk(&entry.path(), &path, entries)?;
        } else if kind.is_file() {
            entries.push(Entry {
                path,
                file: Some(entry.path()),
            });
        } else {
            return Err(io::Error::other(format!(
                "{} is neither a file nor a folder, so it is not hashed",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "hash_test.rs"]
mod tests;
