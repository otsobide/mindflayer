//! Lexical path arithmetic: normalising a path, and working out the route
//! from one to another.
//!
//! Everything here is **lexical**. Nothing touches the disk and nothing
//! resolves symlinks, which is deliberate twice over: a workspace can register
//! a project directory that does not exist yet, and a path that travels
//! through a symlink keeps the shape the user typed rather than the shape the
//! filesystem prefers. The cost is the usual one: `a/b/../c` is treated as
//! `a/c`, which differs from what the kernel would do if `a/b` were a symlink.

use std::path::{Component, Path, PathBuf};

/// Resolve `.` and `..` without touching the disk.
///
/// `..` at the root is dropped rather than escaping it, so the result of
/// normalising an absolute path is always still absolute.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                // A directory name is what `..` cancels out.
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // At a root there is nowhere to climb to, so the `..` is
                // dropped. Popping would turn an absolute path into a
                // relative one, which is never what it means.
                Some(Component::Prefix(_) | Component::RootDir) => {}
                // Nothing to cancel: on a relative path the `..` is part of
                // what the path means, and stacking them has to keep working
                // (`../..` is two levels, not one).
                Some(Component::ParentDir | Component::CurDir) | None => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The route from `base` to `target`, using `..` to climb where needed.
///
/// Both paths are normalised first. Returns `None` when there is no route at
/// all: two absolute paths on different Windows drives, or a relative path
/// compared against an absolute one.
pub fn relative_to(target: &Path, base: &Path) -> Option<PathBuf> {
    let target = normalize(target);
    let base = normalize(base);

    let mut target_parts = target.components().peekable();
    let mut base_parts = base.components().peekable();

    // Walk off the shared prefix; what is left of each side is the route.
    while let (Some(t), Some(b)) = (target_parts.peek(), base_parts.peek()) {
        if t != b {
            break;
        }
        target_parts.next();
        base_parts.next();
    }

    // A root or prefix left on either side means the two paths never met:
    // different Windows drives, or one absolute and one relative. Checking
    // both sides matters — an unmatched root on the *target* would otherwise
    // be pushed onto the route and silently reset it to an absolute path.
    let target_rest: Vec<Component> = target_parts.collect();
    let unrooted = |c: &Component| !matches!(c, Component::Prefix(_) | Component::RootDir);
    if !target_rest.iter().all(unrooted) {
        return None;
    }

    let mut route = PathBuf::new();
    for component in base_parts {
        match component {
            // Every directory still to be left behind is one `..` to climb.
            Component::Normal(_) => route.push(".."),
            Component::Prefix(_) | Component::RootDir => return None,
            // A leading `..` normalize() could not resolve: where that lands
            // depends on a working directory this function does not have.
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    for component in target_rest {
        route.push(component.as_os_str());
    }

    // The two are the same directory. `.` says so; an empty path says nothing
    // and would be written to a config file as "".
    if route.as_os_str().is_empty() {
        route.push(".");
    }
    Some(route)
}

/// A path as it should be written into a config file: `/` separated, so a
/// workspace registered on one platform still resolves on the other.
///
/// `None` when the path is not valid UTF-8, which TOML cannot represent and
/// this refuses to mangle.
pub fn to_config_string(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    Some(if std::path::MAIN_SEPARATOR == '/' {
        text.to_owned()
    } else {
        text.replace(std::path::MAIN_SEPARATOR, "/")
    })
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod tests;
