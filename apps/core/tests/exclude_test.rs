//! Keeping a project's paths out of git through `info/exclude`, checked
//! against what gix, reading the file the way git does, reports as ignored.

use std::fs;
use std::path::{Path, PathBuf};

use mindflayer_core::exclude::{self, Entry, ExcludeError, Excluded};
use tempfile::TempDir;

/// An empty repository, with the `info/exclude` `git init` writes.
fn repository() -> TempDir {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    dir
}

/// A directory inside `root`, made.
fn project(root: &Path, route: &str) -> PathBuf {
    let path = root.join(route);
    fs::create_dir_all(&path).unwrap();
    path
}

fn exclude_file(repository: &Path) -> PathBuf {
    repository.join(".git").join("info").join("exclude")
}

fn skill(name: &str) -> Entry {
    Entry::directory(format!(".agents/skills/{name}"))
}

/// Whether gix holds `path`, relative to the work tree, to be ignored. A
/// trailing `/` asks about a directory. Opened isolated, so nothing from the
/// machine's own git config takes part.
fn ignored(work_tree: &Path, path: &str) -> bool {
    let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
    let index = repository.index_or_empty().unwrap();
    let mut excludes = repository
        .excludes(&index, None, Default::default())
        .unwrap();
    let (path, mode) = match path.strip_suffix('/') {
        Some(directory) => (directory, Some(gix::index::entry::Mode::DIR)),
        None => (path, None),
    };
    excludes.at_path(path, mode).unwrap().is_excluded()
}

/// The lines of `key`'s group, header included, as the file holds them.
fn group(file: &Path, key: &str) -> Vec<String> {
    let text = fs::read_to_string(file).unwrap();
    let header = format!("# project {key}");
    text.lines()
        .skip_while(|line| *line != header)
        .enumerate()
        .take_while(|(at, line)| *at == 0 || !line.starts_with('#'))
        .map(|(_, line)| line.to_owned())
        .collect()
}

#[test]
fn entries_are_added_and_removed_and_gix_reports_them_ignored() {
    let repo = repository();
    let file = exclude_file(repo.path());
    let before = fs::read(&file).unwrap();

    let added = exclude::add(repo.path(), ".", &[skill("commit-style")]).unwrap();
    assert_eq!(added, Excluded::Written { file: file.clone() });
    assert!(ignored(repo.path(), ".agents/skills/commit-style/"));
    assert!(ignored(repo.path(), ".agents/skills/commit-style/SKILL.md"));
    assert!(!ignored(repo.path(), ".agents/skills/review/"));
    let written = fs::read(&file).unwrap();
    assert!(written.starts_with(&before));
    assert_eq!(
        String::from_utf8_lossy(&written[before.len()..]),
        "# mindflayer:begin\n# project .\n/.agents/skills/commit-style/\n# mindflayer:end\n"
    );

    let removed = exclude::remove(repo.path(), ".", &[skill("commit-style")]).unwrap();
    assert_eq!(removed, Excluded::Written { file: file.clone() });
    assert!(!ignored(repo.path(), ".agents/skills/commit-style/"));
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn lines_on_both_sides_of_the_block_stay_byte_identical() {
    let repo = repository();
    let file = exclude_file(repo.path());
    // CRLF, no final newline: what somebody wrote, exactly as they wrote it.
    fs::write(&file, b"*.log\r\n/scratch/").unwrap();
    exclude::add(repo.path(), ".", &[skill("one")]).unwrap();
    let mut text = fs::read(&file).unwrap();
    text.extend_from_slice(b"target/\r\n");
    fs::write(&file, &text).unwrap();

    exclude::add(repo.path(), ".", &[skill("two")]).unwrap();
    assert_eq!(
        String::from_utf8(fs::read(&file).unwrap()).unwrap(),
        "*.log\r\n/scratch/\r\n# mindflayer:begin\r\n# project .\r\n/.agents/skills/one/\r\n\
         /.agents/skills/two/\r\n# mindflayer:end\r\ntarget/\r\n"
    );
    assert!(ignored(repo.path(), ".agents/skills/two/"));
    assert!(ignored(repo.path(), "target/"));

    exclude::remove(repo.path(), ".", &[skill("one"), skill("two")]).unwrap();
    assert_eq!(
        fs::read(&file).unwrap(),
        b"*.log\r\n/scratch/\r\ntarget/\r\n"
    );
}

#[test]
fn adding_what_is_there_writes_nothing() {
    let repo = repository();
    let file = exclude_file(repo.path());
    exclude::add(repo.path(), ".", &[skill("one")]).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    assert_eq!(
        exclude::add(repo.path(), ".", &[skill("one")]).unwrap(),
        Excluded::Unchanged { file: file.clone() }
    );
    assert_eq!(
        exclude::remove(repo.path(), ".", &[skill("two")]).unwrap(),
        Excluded::Unchanged { file: file.clone() }
    );
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
}

#[test]
fn a_repository_without_an_info_folder_gets_one() {
    let repo = repository();
    fs::remove_dir_all(repo.path().join(".git").join("info")).unwrap();
    exclude::add(repo.path(), ".", &[Entry::file("AGENTS.md")]).unwrap();
    assert!(ignored(repo.path(), "AGENTS.md"));
    assert!(!ignored(repo.path(), "docs/AGENTS.md"));
}

#[test]
fn a_nested_project_gets_entries_relative_to_the_work_tree() {
    let repo = repository();
    let app = project(repo.path(), "packages/app");

    exclude::add(&app, "packages/app", &[skill("commit-style")]).unwrap();
    assert_eq!(
        group(&exclude_file(repo.path()), "packages/app"),
        [
            "# project packages/app",
            "/packages/app/.agents/skills/commit-style/"
        ]
    );
    assert!(ignored(
        repo.path(),
        "packages/app/.agents/skills/commit-style/"
    ));
    // Anchored: the same folder anywhere else in the tree is not caught.
    assert!(!ignored(repo.path(), ".agents/skills/commit-style/"));
    assert!(!ignored(
        repo.path(),
        "packages/web/.agents/skills/commit-style/"
    ));
}

#[test]
fn a_route_with_glob_characters_is_matched_literally() {
    let repo = repository();
    let legacy = project(repo.path(), "apps/[legacy] app");

    exclude::add(&legacy, "apps/[legacy] app", &[skill("review")]).unwrap();
    assert!(ignored(
        repo.path(),
        "apps/[legacy] app/.agents/skills/review/"
    ));
    // What `[legacy]` would match as a character class.
    assert!(!ignored(repo.path(), "apps/l app/.agents/skills/review/"));
}

#[test]
fn two_projects_install_and_uninstall_independently() {
    let repo = repository();
    let file = exclude_file(repo.path());
    let before = fs::read(&file).unwrap();
    let app = project(repo.path(), "packages/app");
    let web = project(repo.path(), "packages/web");

    exclude::add(&app, "packages/app", &[skill("one")]).unwrap();
    exclude::add(&web, "packages/web", &[skill("review")]).unwrap();
    let webs = group(&file, "packages/web");
    assert_eq!(
        webs,
        [
            "# project packages/web",
            "/packages/web/.agents/skills/review/"
        ]
    );

    exclude::add(&app, "packages/app", &[skill("two")]).unwrap();
    exclude::remove(&app, "packages/app", &[skill("one")]).unwrap();
    assert_eq!(group(&file, "packages/web"), webs);
    assert_eq!(
        group(&file, "packages/app"),
        [
            "# project packages/app",
            "/packages/app/.agents/skills/two/"
        ]
    );

    exclude::remove(&app, "packages/app", &[skill("two")]).unwrap();
    assert!(group(&file, "packages/app").is_empty());
    assert_eq!(group(&file, "packages/web"), webs);
    assert!(ignored(repo.path(), "packages/web/.agents/skills/review/"));
    assert!(!ignored(repo.path(), "packages/app/.agents/skills/two/"));

    let apps = {
        exclude::add(&app, "packages/app", &[skill("three")]).unwrap();
        group(&file, "packages/app")
    };
    exclude::remove(&web, "packages/web", &[skill("review")]).unwrap();
    assert_eq!(group(&file, "packages/app"), apps);
    exclude::remove(&app, "packages/app", &[skill("three")]).unwrap();
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn a_project_outside_any_repository_gets_a_warning_and_nothing_is_written() {
    let dir = TempDir::new().unwrap();
    let alone = project(dir.path(), "alone");

    let outcome = exclude::add(&alone, "alone", &[skill("one")]).unwrap();
    assert_eq!(
        outcome,
        Excluded::Outside {
            project: alone.clone()
        }
    );
    assert_eq!(
        outcome.warning().unwrap(),
        format!(
            "{}: not in a git work tree, so nothing keeps what is installed there out of git",
            alone.display()
        )
    );
    let outcome = exclude::remove(&alone, "alone", &[skill("one")]).unwrap();
    assert!(outcome.warning().is_some());

    let left: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["alone"]);
    assert_eq!(fs::read_dir(&alone).unwrap().count(), 0);
}

#[test]
fn a_change_that_happened_says_nothing_to_warn_about() {
    let repo = repository();
    let outcome = exclude::add(repo.path(), ".", &[skill("one")]).unwrap();
    assert_eq!(outcome.warning(), None);
}

#[test]
fn a_malformed_block_is_a_failure_and_nothing_is_written() {
    let repo = repository();
    let file = exclude_file(repo.path());
    let before = b"*.log\n# mindflayer:begin\n# project .\n/.agents/skills/one/\n";
    fs::write(&file, before).unwrap();

    let error = exclude::add(repo.path(), ".", &[skill("two")]).unwrap_err();
    assert!(matches!(error, ExcludeError::Malformed { .. }));
    assert_eq!(
        error.to_string(),
        format!(
            "{}: line 2: `# mindflayer:begin` is never closed by `# mindflayer:end`, \
             so nothing was written",
            file.display()
        )
    );
    assert!(exclude::remove(repo.path(), ".", &[skill("one")]).is_err());
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn an_entry_outside_the_project_is_refused_in_git_or_out() {
    let repo = repository();
    let file = exclude_file(repo.path());
    let before = fs::read(&file).unwrap();
    let escape = [Entry::directory("../elsewhere")];
    assert!(matches!(
        exclude::add(repo.path(), ".", &escape),
        Err(ExcludeError::Path { .. })
    ));
    assert_eq!(fs::read(&file).unwrap(), before);

    let dir = TempDir::new().unwrap();
    assert!(matches!(
        exclude::add(dir.path(), ".", &escape),
        Err(ExcludeError::Path { .. })
    ));
}
