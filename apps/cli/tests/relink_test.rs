//! `flayer relink` and `mind init --new-id`: following a project that moved
//! by the id it holds, and giving a copy made with its `.mind` one of its own.

mod common;

use common::*;

/// Under one directory: `Projects`, a workspace keeping `deploy` as its own
/// and linking `collapse`, a project marked by its settings in a repository
/// of its own, with `deploy` installed into it.
fn installed() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    let collapse = projects.join("collapse");
    fs::create_dir_all(&collapse).unwrap();
    flayer(&projects, &["init"]).unwrap();
    flayer(&projects, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    new_project(&collapse);
    flayer(&projects, &["link", "collapse"]).unwrap();
    flayer(&projects, &["install", "-p", "collapse", "deploy"]).unwrap();
    (dir, projects)
}

/// Copy the directory `from` to `to`, `.mind` and all, as `cp -r` would.
fn copied(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copied(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn id_of(root: &Path) -> String {
    MindProject::open(root)
        .unwrap()
        .settings()
        .unwrap()
        .id
        .to_string()
}

#[test]
fn a_suggestion_says_a_moved_project_moved_as_a_scan_does() {
    let (_dir, projects) = installed();
    fs::create_dir_all(projects.join("apps")).unwrap();
    fs::rename(projects.join("collapse"), projects.join("apps/collapse")).unwrap();

    let scan = flayer(&projects, &["scan", "--suggest"]).unwrap();

    // Its reason is its id, but what to do about it is a relink.
    assert_eq!(
        scan.stdout,
        "\
apps/collapse  not linked  moved: collapse -> apps/collapse

  follow one that moved with `flayer relink <old> <new>`
"
    );
    assert!(scan.ok, "{:?}", scan.stderr);
}

#[test]
fn a_moved_project_is_stale_then_found_moved_and_relinked_with_every_install_current() {
    let (_dir, projects) = installed();
    fs::create_dir_all(projects.join("apps")).unwrap();
    fs::rename(projects.join("collapse"), projects.join("apps/collapse")).unwrap();

    let status = flayer(&projects, &["status"]).unwrap();
    assert_eq!(status.stdout, "collapse  stale\n");
    assert!(!status.ok);

    let scan = flayer(&projects, &["scan"]).unwrap();
    assert_eq!(
        scan.stdout,
        "\
apps/collapse  not linked  moved: collapse -> apps/collapse

  follow one that moved with `flayer relink <old> <new>`
"
    );
    assert!(scan.ok, "{:?}", scan.stderr);

    let relinked = flayer(&projects, &["relink", "collapse", "apps/collapse"]).unwrap();
    assert_eq!(
        relinked.stdout,
        "relinked collapse as apps/collapse, from collapse\n  1 install kept with it\n"
    );
    assert!(relinked.ok, "{:?}", relinked.stderr);
    assert!(relinked.stderr.is_empty(), "{:?}", relinked.stderr);

    let status = flayer(&projects, &["status"]).unwrap();
    assert_eq!(
        status.stdout,
        "deploy  current  claude,codex,opencode  workspace\n"
    );
    assert!(status.ok, "{:?}", status.stderr);
    assert!(registry(&projects).contains("\"apps/collapse\""));
    assert!(!registry(&projects).contains("\"collapse\""));
    // Nothing moved is left to offer git where the repository is now.
    assert!(untracked(&projects.join("apps/collapse"))
        .iter()
        .all(|path| !path.contains("deploy")));
}

#[test]
fn a_project_moved_before_the_workspace_learned_its_id_is_pointed_at_relink_not_link() {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    let collapse = projects.join("collapse");
    fs::create_dir_all(&collapse).unwrap();
    flayer(&projects, &["init"]).unwrap();
    new_project(&collapse);
    flayer(&projects, &["link", "collapse"]).unwrap();
    fs::create_dir_all(projects.join("apps")).unwrap();
    fs::rename(&collapse, projects.join("apps/collapse")).unwrap();

    let scan = flayer(&projects, &["scan"]).unwrap();

    assert_eq!(
        scan.stdout,
        "\
apps/collapse  not linked  mind project

  follow one that moved from collapse with `flayer relink collapse <path>`
  link one with `flayer link <path>`
"
    );
    assert!(scan.ok, "{:?}", scan.stderr);
    let relinked = flayer(&projects, &["relink", "collapse", "apps/collapse"]).unwrap();
    assert_eq!(
        relinked.stdout,
        "relinked collapse as apps/collapse, from collapse\n"
    );
}

#[test]
fn a_directory_holding_another_id_is_refused_and_nothing_changes() {
    let (_dir, projects) = installed();
    fs::remove_dir_all(projects.join("collapse")).unwrap();
    let other = projects.join("other");
    fs::create_dir(&other).unwrap();
    new_project(&other);
    let before = registry(&projects);

    let failure = flayer(&projects, &["relink", "collapse", "other"]).unwrap_err();

    let said = failure.error.to_string();
    assert!(
        said.contains(&format!("holds project {}", id_of(&other))),
        "{said}"
    );
    assert!(said.ends_with("nothing was changed"), "{said}");
    assert_eq!(registry(&projects), before);
}

#[test]
fn a_copy_made_with_its_mind_is_a_duplicate_and_never_a_move() {
    let (_dir, projects) = installed();
    copied(&projects.join("collapse"), &projects.join("copy"));

    let scan = flayer(&projects, &["scan"]).unwrap();

    assert_eq!(
        scan.stdout,
        "\
collapse  linked
copy      not linked  mind project

  link one with `flayer link <path>`
"
    );
    assert_eq!(
        scan.stderr,
        [format!(
            "warning: {} and {} are both project {}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own",
            projects.join("collapse").display(),
            projects.join("copy").display(),
            id_of(&projects.join("copy"))
        )]
    );
    assert!(!scan.ok);
    let failure = flayer(&projects, &["relink", "collapse", "copy"]).unwrap_err();
    assert!(
        failure
            .error
            .to_string()
            .contains("copied with its .mind; `mind -C <copy> init --new-id`"),
        "{}",
        failure.error
    );
}

#[test]
fn relink_refuses_while_a_linked_copy_holds_the_id_until_it_runs_new_id() {
    let (_dir, projects) = installed();
    copied(&projects.join("collapse"), &projects.join("copy"));
    flayer(&projects, &["link", "copy"]).unwrap();
    let status = flayer(&projects, &["status"]).unwrap();
    assert!(
        status
            .stderr
            .iter()
            .any(|line| line.contains("are both project")),
        "{:?}",
        status.stderr
    );
    fs::rename(projects.join("collapse"), projects.join("moved")).unwrap();

    let scan = flayer(&projects, &["scan"]).unwrap();
    assert!(!scan.stdout.contains("moved:"), "{}", scan.stdout);
    let refused = flayer(&projects, &["relink", "collapse", "moved"]).unwrap_err();
    assert!(
        refused.error.to_string().contains("are both project"),
        "{}",
        refused.error
    );

    let was = id_of(&projects.join("copy"));
    let renewed = mind(&projects.join("copy"), &["init", "--new-id"]).unwrap();
    let id = id_of(&projects.join("copy"));
    assert_ne!(id, was);
    assert_eq!(
        renewed.stdout,
        // Called what the project it was copied from is called, as a copy is.
        format!("gave collapse a new id, {id} (it was {was})\n")
    );
    assert!(renewed.ok, "{:?}", renewed.stderr);

    let scan = flayer(&projects, &["scan"]).unwrap();
    assert!(
        scan.stdout.contains("moved: collapse -> moved"),
        "{}",
        scan.stdout
    );
    let relinked = flayer(&projects, &["relink", "collapse", "moved"]).unwrap();
    assert_eq!(
        relinked.stdout,
        "relinked collapse as moved, from collapse\n  1 install kept with it\n"
    );
}

#[test]
fn a_new_id_is_refused_where_settings_cannot_change_and_alongside_what_init_asks() {
    let dir = TempDir::new().unwrap();
    json_project(dir.path());

    let failure = mind(dir.path(), &["init", "--new-id"]).unwrap_err();
    assert!(
        failure.error.to_string().contains("`mind migrate`"),
        "{}",
        failure.error
    );

    let parsed = Cli::try_parse_from(["mind", "init", "--new-id", "--shared"]);
    assert!(parsed.is_err());
}
