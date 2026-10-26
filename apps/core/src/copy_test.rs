//! Unit tests for `copy.rs`, kept beside it rather than inside it.

use super::*;
use tempfile::TempDir;

/// A folder holding a skill to copy, and somewhere to copy it to.
fn fixture() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("source").join("deploy");
    fs::create_dir_all(from.join("scripts")).unwrap();
    fs::write(from.join("SKILL.md"), "first").unwrap();
    fs::write(from.join("scripts").join("run.sh"), "echo hi\n").unwrap();
    let to = dir.path().join("project").join("skills").join("deploy");
    let scratch = dir.path().join("project").join(".mind").join("tmp");
    (dir, from, to, scratch)
}

/// The names directly inside `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The stages in a scratch folder, which is everything but its own two files.
fn stages(scratch: &Path) -> Vec<String> {
    names(scratch)
        .into_iter()
        .filter(|name| name != ".gitignore" && name != LOCK)
        .collect()
}

#[test]
fn a_scratch_folder_is_made_when_first_needed_and_ignores_itself() {
    let (_dir, _, _, scratch) = fixture();
    assert!(!scratch.exists());

    let stage = Stage::open(&scratch).unwrap();

    assert!(stage.path().starts_with(&scratch) && stage.path().is_dir());
    let ignore = fs::read_to_string(scratch.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == "*"), "{ignore}");
}

#[test]
fn a_gitignore_somebody_changed_is_left_as_it_is() {
    let (_dir, _, _, scratch) = fixture();
    fs::create_dir_all(&scratch).unwrap();
    fs::write(scratch.join(".gitignore"), "mine\n").unwrap();

    Stage::open(&scratch).unwrap();

    assert_eq!(
        fs::read_to_string(scratch.join(".gitignore")).unwrap(),
        "mine\n"
    );
}

#[test]
fn a_stage_is_named_after_its_process_and_something_random() {
    let (_dir, _, _, scratch) = fixture();

    let stage = Stage::open(&scratch).unwrap();

    let name = stage.path().file_name().unwrap().to_str().unwrap();
    let (pid, suffix) = name.split_once('-').unwrap();
    assert_eq!(pid, std::process::id().to_string());
    assert_eq!(suffix.len(), SUFFIX_LEN, "{name}");
    assert!(is_stage_name(name), "{name}");
}

#[test]
fn two_stages_open_at_once_never_share_a_folder() {
    let (_dir, from, to, scratch) = fixture();
    let mut first = Stage::open(&scratch).unwrap();
    fs::write(first.path().join("mine"), "first").unwrap();

    let mut second = Stage::open(&scratch).unwrap();

    assert_ne!(first.path(), second.path());
    // The first is in use, so the second must not have swept it.
    assert!(second.swept().is_empty(), "{:?}", second.swept());
    assert_eq!(
        fs::read_to_string(first.path().join("mine")).unwrap(),
        "first"
    );
    // Both build the same thing, each in its own folder.
    assert_eq!(first.replace(&from, &to, &from).unwrap(), Change::Added);
    assert_eq!(
        second.replace(&from, &to, &from).unwrap(),
        Change::Unchanged
    );
}

#[test]
fn a_dropped_stage_takes_its_folder_with_it() {
    let (_dir, _, _, scratch) = fixture();
    let stage = Stage::open(&scratch).unwrap();
    let path = stage.path().to_path_buf();

    drop(stage);

    assert!(!path.exists());
    assert!(stages(&scratch).is_empty(), "{:?}", stages(&scratch));
}

#[test]
fn what_an_interrupted_run_left_is_swept_by_the_next_and_said() {
    let (_dir, _, _, scratch) = fixture();
    // Named as a stage is, by a process that is not running any more, with
    // half a copy in it.
    let left = scratch.join("4242-00000000deadbeef");
    fs::create_dir_all(left.join("new").join("deploy")).unwrap();
    fs::write(left.join("new").join("deploy").join("SKILL.md"), "half").unwrap();

    let stage = Stage::open(&scratch).unwrap();

    assert_eq!(stage.swept(), std::slice::from_ref(&left));
    assert!(!left.exists());
}

#[test]
fn nothing_is_swept_while_another_run_holds_a_stage() {
    let (_dir, _, _, scratch) = fixture();
    let running = Stage::open(&scratch).unwrap();
    let left = scratch.join("4242-00000000deadbeef");
    fs::create_dir_all(&left).unwrap();

    // Whether that one is a leftover or another run's cannot be told while
    // a run is going, so it waits for a run that finds itself alone.
    let meanwhile = Stage::open(&scratch).unwrap();
    assert!(meanwhile.swept().is_empty(), "{:?}", meanwhile.swept());
    assert!(left.is_dir() && running.path().is_dir());

    drop((running, meanwhile));
    let alone = Stage::open(&scratch).unwrap();
    assert_eq!(alone.swept(), [left]);
}

#[test]
fn only_what_is_named_as_a_stage_is_swept() {
    let (_dir, _, _, scratch) = fixture();
    fs::create_dir_all(scratch.join("notes")).unwrap();
    fs::create_dir_all(scratch.join("4242-short")).unwrap();
    fs::write(scratch.join("4242-00000000deadbeef"), "a file").unwrap();

    let stage = Stage::open(&scratch).unwrap();

    assert!(stage.swept().is_empty(), "{:?}", stage.swept());
    assert!(scratch.join("notes").is_dir());
    assert!(scratch.join("4242-short").is_dir());
    assert!(scratch.join("4242-00000000deadbeef").is_file());
}

#[test]
fn a_stage_name_is_digits_a_dash_and_sixteen_hex_digits() {
    assert!(is_stage_name("1-0123456789abcdef"));
    assert!(!is_stage_name("-0123456789abcdef"));
    assert!(!is_stage_name("1-0123456789abcde"));
    assert!(!is_stage_name("1-0123456789abcdeg"));
    assert!(!is_stage_name("x1-0123456789abcdef"));
    assert!(!is_stage_name(".gitignore"));
    assert!(!is_stage_name(LOCK));
}

#[test]
fn replacing_through_a_stage_leaves_nothing_beside_the_target() {
    let (_dir, from, to, scratch) = fixture();
    let skills = to.parent().unwrap().to_path_buf();
    let mut stage = Stage::open(&scratch).unwrap();

    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Added);
    assert_eq!(names(&skills), ["deploy"]);

    fs::write(from.join("SKILL.md"), "second").unwrap();
    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Updated);
    assert_eq!(names(&skills), ["deploy"]);
    assert_eq!(fs::read_to_string(to.join("SKILL.md")).unwrap(), "second");

    assert_eq!(stage.replace(&from, &to, &from).unwrap(), Change::Unchanged);
    // What was built and what was moved aside went with each replacement.
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

#[test]
fn replacing_makes_the_folder_the_target_goes_in() {
    let (_dir, from, to, scratch) = fixture();
    assert!(!to.parent().unwrap().exists());

    let change = Stage::open(&scratch)
        .unwrap()
        .replace(&from, &to, &from)
        .unwrap();

    assert_eq!(change, Change::Added);
    assert!(to.join("scripts").join("run.sh").is_file());
}

#[cfg(unix)]
#[test]
fn a_copy_that_fails_leaves_the_target_and_its_folder_as_they_were() {
    let (dir, from, to, scratch) = fixture();
    let mut stage = Stage::open(&scratch).unwrap();
    stage.replace(&from, &to, &from).unwrap();
    fs::write(dir.path().join("private.env"), "SECRET").unwrap();
    std::os::unix::fs::symlink(dir.path().join("private.env"), from.join("aaa.env")).unwrap();
    fs::write(from.join("SKILL.md"), "second").unwrap();

    let error = stage.replace(&from, &to, &from).unwrap_err();

    assert!(error.to_string().contains("outside"), "{error}");
    assert_eq!(fs::read_to_string(to.join("SKILL.md")).unwrap(), "first");
    assert_eq!(names(to.parent().unwrap()), ["deploy"]);
    assert!(names(stage.path()).is_empty(), "{:?}", names(stage.path()));
}

#[test]
fn a_rename_that_fails_says_what_it_was_moving_where() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("missing");
    let to = dir.path().join("there");

    let error = moved(&from, &to).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    let message = error.to_string();
    assert!(
        message.contains(&from.display().to_string())
            && message.contains(&to.display().to_string()),
        "{message}"
    );
}

#[test]
fn replacing_beside_still_leaves_nothing_behind() {
    // Gathering's way, onto the shelf: staged beside, and cleared after.
    let (_dir, from, to, _) = fixture();
    let skills = to.parent().unwrap().to_path_buf();

    assert_eq!(replace(&from, &to, &from).unwrap(), Change::Added);
    fs::write(from.join("SKILL.md"), "second").unwrap();
    assert_eq!(replace(&from, &to, &from).unwrap(), Change::Updated);

    assert_eq!(names(&skills), ["deploy"]);
}
