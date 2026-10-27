//! Unit tests for `claude_md.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn the_walk_counts_the_folders_claudes_own_check_counted() {
    // The starts of the probe's runs, and how many folders Claude's debug
    // log said it searched from each (docs/agents.md).
    let from = |start: &str| searched(Path::new(start)).count();
    assert_eq!(from("/mnt/agent-probe/stage/claude-excluded"), 4);
    assert_eq!(from("/mnt/agent-probe/home/work/claude-user-memory"), 5);
}

#[test]
fn the_walk_goes_from_the_start_up_and_stops_below_the_root() {
    let folders: Vec<&Path> = searched(Path::new("/srv/work/app")).collect();
    assert_eq!(
        folders,
        [
            Path::new("/srv/work/app"),
            Path::new("/srv/work"),
            Path::new("/srv")
        ]
    );
    assert_eq!(searched(Path::new("/")).count(), 0);
}
