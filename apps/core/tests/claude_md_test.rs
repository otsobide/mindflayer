//! Whether Claude Code reads a project's `AGENTS.md`: each file that keeps it
//! from doing so, wherever above the project it is, and the user memory file,
//! which never does; and how Claude is to reach the block of rules there,
//! natively, through an import in the project's own `CLAUDE.md`, or not at
//! all.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::claude_md::{self, ClaudeMdError, Fallback, Reach};
use mindflayer_core::claude_rules::Mode;
use mindflayer_core::{Agent, AgentHomes, Directories, MindProject, Share};
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

/// What a team's `CLAUDE.md` holds before Mindflayer writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A project at `app` in a git work tree, its settings sharing as `share`
/// says, in a folder with nothing above it in Claude's way.
struct Fixture {
    dir: TempDir,
    project: MindProject,
}

fn fixture(share: Share) -> Fixture {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let root = folder(dir.path(), "app");
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    Fixture { dir, project }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.project.root()
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }

    /// Whatever is in the work tree, put in the index: git tracks it.
    fn track(&self) {
        common::indexed(self.dir.path());
    }

    fn reach(&self, block_written: bool) -> Reach {
        claude_md::reach(&self.project, &NO_HOMES, block_written).unwrap()
    }
}

/// The three names Claude's check looks for in a folder.
const NAMES: [&str; 3] = ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"];

/// The reach a local project without `edit_tracked` is to have, holding the
/// files `present` names, tracked by git or not: the import in the first of
/// its own `CLAUDE.md` and `.claude/CLAUDE.md` while git does not track it,
/// and otherwise the rules folder, said for that file; with neither, the
/// rules folder for a `CLAUDE.local.md`, and `AGENTS.md` read natively
/// without one.
fn expected(fixture: &Fixture, present: &[&str], tracked: bool) -> Reach {
    let own = ["CLAUDE.md", ".claude/CLAUDE.md"]
        .into_iter()
        .find(|name| present.contains(name));
    match own {
        Some(name) if tracked => Reach::Fallback(Fallback::Tracked(fixture.file(name))),
        Some(name) => Reach::Import(fixture.file(name)),
        None if present.contains(&"CLAUDE.local.md") => {
            Reach::Fallback(Fallback::LocalOnly(fixture.file("CLAUDE.local.md")))
        }
        None => Reach::Native,
    }
}

/// Every combination of the three names, as the names present.
fn combinations() -> Vec<Vec<&'static str>> {
    (0..1 << NAMES.len())
        .map(|mask| {
            NAMES
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, name)| *name)
                .collect()
        })
        .collect()
}

#[test]
fn each_combination_of_claudes_files_tracked_or_not_chooses_its_reach() {
    for present in combinations() {
        for tracked in [false, true] {
            let fixture = fixture(Share::Local);
            for name in &present {
                write(fixture.root(), name, TEAM);
            }
            if tracked {
                fixture.track();
            }
            let reach = fixture.reach(true);
            assert_eq!(
                reach,
                expected(&fixture, &present, tracked),
                "{present:?}, tracked: {tracked}"
            );
            let mode = if matches!(reach, Reach::Fallback(_)) {
                Mode::Fallback
            } else {
                Mode::Scoped
            };
            assert_eq!(reach.mode(), mode);
        }
    }
}

#[test]
fn a_tracked_claude_md_takes_the_import_in_a_shared_project_or_with_edit_tracked() {
    for present in combinations() {
        for edit_tracked in [false, true] {
            let share = if edit_tracked {
                Share::Local
            } else {
                Share::Shared
            };
            let mut fixture = fixture(share);
            if edit_tracked {
                fixture
                    .project
                    .change_settings(|settings| settings.edit_tracked = true)
                    .unwrap();
            }
            for name in &present {
                write(fixture.root(), name, TEAM);
            }
            fixture.track();
            assert_eq!(
                fixture.reach(true),
                expected(&fixture, &present, false),
                "{present:?}, edit_tracked: {edit_tracked}"
            );
        }
    }
}

#[test]
fn a_tracked_claude_md_gives_way_to_a_dot_claude_one_git_does_not_track() {
    let fixture = fixture(Share::Local);
    write(fixture.root(), "CLAUDE.md", TEAM);
    fixture.track();
    write(fixture.root(), ".claude/CLAUDE.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file(".claude/CLAUDE.md"))
    );
}

#[test]
fn a_project_a_mind_toml_marks_was_committed_by_design_and_takes_the_import() {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let root = folder(dir.path(), "app");
    let (project, _) = MindProject::init(&root).unwrap();
    write(&root, "CLAUDE.md", TEAM);
    common::indexed(dir.path());
    assert_eq!(
        claude_md::reach(&project, &NO_HOMES, true).unwrap(),
        Reach::Import(root.join("CLAUDE.md"))
    );
}

#[test]
fn no_block_written_is_the_fallback_whatever_the_files_say() {
    for present in combinations() {
        let fixture = fixture(Share::Shared);
        for name in &present {
            write(fixture.root(), name, TEAM);
        }
        assert_eq!(
            fixture.reach(false),
            Reach::Fallback(Fallback::NoBlock),
            "{present:?}"
        );
        assert_eq!(fixture.reach(false).mode(), Mode::Fallback);
    }
}

#[test]
fn a_claude_md_above_the_git_root_gives_the_fallback() {
    let dir = clean();
    // A stand-in for ~/Work, holding what its owner wants every agent to know.
    let work = write(dir.path(), "Work/CLAUDE.md", TEAM);
    let repo = folder(dir.path(), "Work/repo");
    gix::init(&repo).unwrap();
    let (project, _) =
        MindProject::init_settings(&repo, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let reach = claude_md::reach(&project, &NO_HOMES, true).unwrap();
    assert_eq!(reach, Reach::Fallback(Fallback::Above(resolved(&work))));
    assert_eq!(reach.mode(), Mode::Fallback);
}

#[test]
fn a_project_nested_under_a_repository_root_claude_md_gives_the_fallback() {
    let dir = clean();
    gix::init(dir.path()).unwrap();
    let team = write(dir.path(), "CLAUDE.md", TEAM);
    let app = folder(dir.path(), "packages/app");
    let (project, _) =
        MindProject::init_settings(&app, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    assert_eq!(
        claude_md::reach(&project, &NO_HOMES, true).unwrap(),
        Reach::Fallback(Fallback::Above(resolved(&team)))
    );
}

#[test]
fn a_claude_local_md_nearer_than_one_above_is_the_reason_given() {
    let fixture = fixture(Share::Shared);
    write(fixture.dir.path(), "CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.local.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::LocalOnly(fixture.file("CLAUDE.local.md")))
    );
}

#[test]
fn a_project_with_a_claude_md_of_its_own_takes_the_import_whatever_is_above() {
    let fixture = fixture(Share::Shared);
    write(fixture.dir.path(), "CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.md", TEAM);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file("CLAUDE.md"))
    );
}

#[test]
fn the_user_memory_file_passed_in_never_counts_for_the_reach() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", TEAM);
    let app = folder(&home, "work/app");
    let (project, _) =
        MindProject::init_settings(&app, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let homes = AgentHomes::defaults_under(&home);
    assert_eq!(
        claude_md::reach(&project, &homes, true).unwrap(),
        Reach::Native
    );
}

#[test]
fn the_user_memory_file_in_a_project_at_the_home_is_never_its_claude_md() {
    let dir = clean();
    let home = folder(dir.path(), "home");
    write(&home, ".claude/CLAUDE.md", TEAM);
    let (project, _) =
        MindProject::init_settings(&home, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let homes = AgentHomes::defaults_under(&home);
    assert_eq!(
        claude_md::reach(&project, &homes, true).unwrap(),
        Reach::Native
    );
}

#[test]
fn an_import_already_in_a_tracked_file_is_how_claude_reaches_the_block() {
    let fixture = fixture(Share::Local);
    let imported =
        format!("<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n\n{TEAM}");
    write(fixture.root(), ".claude/CLAUDE.md", TEAM);
    write(fixture.root(), "CLAUDE.md", &imported);
    fixture.track();
    // Committed with the import in it: Claude reads it there, so its rules
    // folder must not give them a second time.
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file("CLAUDE.md"))
    );
}

#[test]
fn an_import_in_the_second_file_keeps_it_there() {
    let fixture = fixture(Share::Shared);
    let imported =
        format!("<!-- mindflayer:begin -->\n@../AGENTS.md\n<!-- mindflayer:end -->\n\n{TEAM}");
    write(fixture.root(), "CLAUDE.md", TEAM);
    write(fixture.root(), ".claude/CLAUDE.md", &imported);
    assert_eq!(
        fixture.reach(true),
        Reach::Import(fixture.file(".claude/CLAUDE.md"))
    );
}

#[test]
fn a_malformed_block_in_the_projects_claude_md_is_refused() {
    let fixture = fixture(Share::Shared);
    write(
        fixture.root(),
        "CLAUDE.md",
        "<!-- mindflayer:begin -->\n@AGENTS.md\n",
    );
    let error = claude_md::reach(&fixture.project, &NO_HOMES, true).unwrap_err();
    assert!(matches!(error, ClaudeMdError::Malformed { .. }), "{error}");
}

#[cfg(unix)]
#[test]
fn a_claude_md_that_is_the_agents_md_reads_the_block_as_it_is() {
    for (link, target) in [("CLAUDE.md", "AGENTS.md"), ("AGENTS.md", "CLAUDE.md")] {
        let fixture = fixture(Share::Local);
        write(fixture.root(), target, TEAM);
        std::os::unix::fs::symlink(target, fixture.file(link)).unwrap();
        fixture.track();
        let reach = fixture.reach(true);
        assert_eq!(reach, Reach::Same(fixture.file("CLAUDE.md")), "{link}");
        assert_eq!(reach.mode(), Mode::Scoped);
    }
}

#[cfg(unix)]
#[test]
fn a_dot_claude_md_that_is_the_agents_md_wins_over_a_claude_md_to_import_into() {
    let fixture = fixture(Share::Shared);
    write(fixture.root(), "AGENTS.md", TEAM);
    write(fixture.root(), "CLAUDE.md", TEAM);
    folder(fixture.root(), ".claude");
    std::os::unix::fs::symlink("../AGENTS.md", fixture.file(".claude/CLAUDE.md")).unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Same(fixture.file(".claude/CLAUDE.md"))
    );
}

#[cfg(unix)]
#[test]
fn a_claude_md_linked_to_another_file_in_the_project_is_never_imported_into() {
    let fixture = fixture(Share::Shared);
    let docs = write(fixture.root(), "docs/claude.md", TEAM);
    std::os::unix::fs::symlink("docs/claude.md", fixture.file("CLAUDE.md")).unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::Linked {
            file: fixture.file("CLAUDE.md"),
            target: resolved(&docs),
        })
    );
}

#[cfg(unix)]
#[test]
fn a_dot_claude_linked_out_of_the_project_is_never_imported_into() {
    let fixture = fixture(Share::Shared);
    let elsewhere = write(fixture.dir.path(), "config/claude/CLAUDE.md", TEAM);
    std::os::unix::fs::symlink(
        fixture.dir.path().join("config/claude"),
        fixture.file(".claude"),
    )
    .unwrap();
    assert_eq!(
        fixture.reach(true),
        Reach::Fallback(Fallback::Linked {
            file: fixture.file(".claude/CLAUDE.md"),
            target: resolved(&elsewhere),
        })
    );
}

/// `path` resolved, as the walk up from a project spells what it finds.
fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}
