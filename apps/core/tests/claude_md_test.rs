//! Whether Claude Code reads a project's `AGENTS.md`: each file that keeps it
//! from doing so, wherever above the project it is, and the user memory file,
//! which never does.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::claude_md;
use mindflayer_core::AgentHomes;
use tempfile::TempDir;

/// No agent's own folder, as a front end hands over when a test pins them.
const NO_HOMES: AgentHomes = AgentHomes {
    claude_config_dir: None,
    codex_home: None,
    opencode_config_dir: None,
    agents_dir: None,
};

/// A folder `route` below `root`, made.
fn folder(root: &Path, route: &str) -> PathBuf {
    let path = root.join(route);
    fs::create_dir_all(&path).unwrap();
    path
}

/// `contents` at `route` below `root`, with the folders it needs.
fn write(root: &Path, route: &str, contents: &str) -> PathBuf {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn reads(folder: &Path, homes: &AgentHomes) -> bool {
    claude_md::reads_agents_md(folder, homes).unwrap()
}

/// A temporary folder, held to have nothing above it that Claude would take
/// for a `CLAUDE.md`, or every test here would say so for the wrong reason.
fn clean() -> TempDir {
    let dir = TempDir::new().unwrap();
    assert!(
        reads(dir.path(), &NO_HOMES),
        "a CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md above {} keeps these tests \
         from telling anything",
        dir.path().display()
    );
    dir
}

#[test]
fn claude_reads_agents_md_where_no_claude_md_is_found() {
    let dir = clean();
    let app = folder(dir.path(), "work/app");
    write(&app, "AGENTS.md", "# Working here\n");
    // None of these is a CLAUDE.md to Claude's check.
    write(&app, ".claude/AGENTS.md", "# Mine\n");
    write(&app, ".claude/rules/mindflayer/style.md", "Be brief.\n");
    write(
        &app,
        ".claude/skills/review/SKILL.md",
        "---\nname: review\n---\n",
    );
    write(&app, "docs/CLAUDE.md", "# Below\n");
    folder(&app, "CLAUDE");
    assert!(reads(&app, &NO_HOMES));
}

#[test]
fn each_name_in_the_project_keeps_claude_from_reading_it() {
    for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
        let dir = clean();
        let app = folder(dir.path(), "app");
        write(&app, name, "# Team\n");
        assert!(!reads(&app, &NO_HOMES), "{name}");
    }
}

#[test]
fn each_name_in_any_folder_above_keeps_claude_from_reading_it() {
    for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
        let dir = clean();
        // A stand-in for ~/Work, above the git root and the project below it.
        let work = folder(dir.path(), "Work");
        let repo = folder(&work, "repo");
        gix::init(&repo).unwrap();
        let app = folder(&repo, "packages/app");
        write(&work, name, "# Everything I work on\n");
        assert!(!reads(&app, &NO_HOMES), "{name}");
        assert!(!reads(&repo, &NO_HOMES), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_found_through_a_link_counts_and_a_link_to_nothing_does_not() {
    let dir = clean();
    let app = folder(dir.path(), "app");
    let team = write(dir.path(), "team/CLAUDE.md", "# Team\n");
    std::os::unix::fs::symlink(&team, app.join("CLAUDE.md")).unwrap();
    assert!(!reads(&app, &NO_HOMES));

    let other = folder(dir.path(), "other");
    std::os::unix::fs::symlink(dir.path().join("gone.md"), other.join("CLAUDE.md")).unwrap();
    assert!(reads(&other, &NO_HOMES));
}

#[test]
fn the_user_memory_file_passed_in_never_counts() {
    let dir = clean();
    // A stand-in for the user's home, with the memory file where Claude keeps
    // it, and a project under that home, as most projects are.
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let homes = AgentHomes::defaults_under(&home);
    assert!(reads(&app, &homes));

    // Not passed in, it is the home's own `.claude/CLAUDE.md`, like any
    // other above the project.
    assert!(!reads(&app, &NO_HOMES));
    // And the memory file passed in leaves out only itself.
    write(&home, "CLAUDE.md", "# Home\n");
    assert!(!reads(&app, &homes));
}

#[cfg(unix)]
#[test]
fn the_user_memory_file_named_through_a_link_is_still_left_out() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let linked = dir.path().join("config");
    std::os::unix::fs::symlink(home.join(".claude"), &linked).unwrap();
    let homes = AgentHomes {
        claude_config_dir: Some(linked),
        ..NO_HOMES
    };
    assert!(reads(&app, &homes));
}

#[test]
fn claude_moved_elsewhere_leaves_the_homes_own_file_counting() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", "# Not Claude's any more\n");
    write(dir.path(), "elsewhere/CLAUDE.md", "# Everywhere\n");
    let app = folder(&home, "work/app");
    let homes = AgentHomes {
        claude_config_dir: Some(dir.path().join("elsewhere")),
        ..NO_HOMES
    };
    assert!(!reads(&app, &homes));
}

#[test]
fn a_folder_that_is_not_there_cannot_be_told() {
    let dir = clean();
    let error = claude_md::reads_agents_md(&dir.path().join("gone"), &NO_HOMES).unwrap_err();
    assert!(error.to_string().contains("gone"), "{error}");
}
