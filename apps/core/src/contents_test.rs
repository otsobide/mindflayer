//! Unit tests for `contents.rs`: what a front end is handed of an artifact's
//! files, and what it never is.

use std::fs;

use tempfile::TempDir;

use super::*;

/// A project holding the skill `deploy`, with a reference and an asset
/// beside its `SKILL.md`, and the rule `git/commits`.
fn fixture() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path().join("api")).unwrap();
    let skill = skill_folder(&project, "deploy");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    fs::write(skill.join("references").join("guide.md"), "# Guide\n").unwrap();
    let rules = project.directory_for(Kind::Rule).unwrap().join("git");
    fs::create_dir_all(&rules).unwrap();
    fs::write(
        rules.join("commits.md"),
        "Write the subject in the imperative.\n",
    )
    .unwrap();
    fs::write(rules.join("branches.md"), "Branch from dev.\n").unwrap();
    (dir, project)
}

const SKILL: &str = "---\nname: deploy\ndescription: Ship it\n---\n\n# deploy\n";

fn skill_folder(project: &MindProject, name: &str) -> PathBuf {
    project.directory_for(Kind::Skill).unwrap().join(name)
}

fn code(result: Result<Contents, ContentsError>) -> &'static str {
    result.expect_err("the file was read").code()
}

#[test]
fn a_skills_manifest_comes_back_as_its_text() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "deploy", "SKILL.md").unwrap();

    assert_eq!(
        contents,
        Contents {
            kind: Kind::Skill,
            name: String::from("deploy"),
            file: String::from("SKILL.md"),
            size: SKILL.len() as u64,
            text: Some(String::from(SKILL)),
            withheld: None,
        }
    );
}

#[test]
fn a_file_further_down_a_skill_is_named_by_its_route() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "skill:deploy", "references/guide.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("# Guide\n"));
    assert_eq!(contents.file, "references/guide.md");
}

/// A route is plain names from the artifact's folder, so nothing that would
/// climb out of it, or start somewhere else, reaches the disk.
#[test]
fn a_route_that_is_not_plain_names_is_refused_before_the_disk_is_read() {
    let (_dir, project) = fixture();

    for file in [
        "../deploy/SKILL.md",
        "references/../SKILL.md",
        "./SKILL.md",
        "/etc/passwd",
        "",
        "references/",
        "references//guide.md",
        "references\\guide.md",
    ] {
        assert_eq!(
            code(Contents::of_project(&project, "deploy", file)),
            "contents_route_unfit",
            "{file:?}"
        );
    }
}

#[test]
fn a_rule_is_one_file_named_by_its_own_name() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "rule:git/commits", "commits.md").unwrap();

    assert_eq!(contents.kind, Kind::Rule);
    assert_eq!(contents.name, "git/commits");
    assert_eq!(
        contents.text.as_deref(),
        Some("Write the subject in the imperative.\n")
    );
    // Its neighbour is a rule of its own, not a file of this one.
    assert_eq!(
        code(Contents::of_project(&project, "git/commits", "branches.md")),
        "contents_route_unfit"
    );
}

#[test]
fn a_binary_file_is_reported_with_its_size_and_never_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(skill.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 0xff]).unwrap();
    fs::write(skill.join("nul.txt"), "plain\0text").unwrap();

    for (file, size) in [("logo.png", 6), ("nul.txt", 10)] {
        let contents = Contents::of_project(&project, "deploy", file).unwrap();
        assert_eq!(contents.text, None, "{file}");
        assert_eq!(contents.withheld, Some(Withheld::Binary), "{file}");
        assert_eq!(contents.size, size, "{file}");
    }
}

#[test]
fn a_file_over_the_cap_is_reported_with_its_size_and_never_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    let over = usize::try_from(CAP).unwrap() + 1;
    fs::write(skill.join("huge.md"), "a".repeat(over)).unwrap();
    fs::write(skill.join("full.md"), "a".repeat(over - 1)).unwrap();

    let huge = Contents::of_project(&project, "deploy", "huge.md").unwrap();
    assert_eq!(huge.text, None);
    assert_eq!(huge.withheld, Some(Withheld::TooLarge));
    assert_eq!(huge.size, CAP + 1);
    // The cap itself is still read.
    let full = Contents::of_project(&project, "deploy", "full.md").unwrap();
    assert_eq!(full.text.map(|text| text.len() as u64), Some(CAP));
}

/// The asset a gathered skill might carry to run in a browser comes back as
/// text in a string, with nothing to say it is a page or an image.
#[test]
fn markup_comes_back_as_text_with_no_type_to_render_it_by() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    let page = "<html><script>fetch('/api/commands')</script></html>\n";
    let image = "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>\n";
    fs::write(skill.join("page.html"), page).unwrap();
    fs::write(skill.join("logo.svg"), image).unwrap();

    for (file, text) in [("page.html", page), ("logo.svg", image)] {
        let contents = Contents::of_project(&project, "deploy", file).unwrap();
        assert_eq!(contents.text.as_deref(), Some(text));
        let json = serde_json::to_value(&contents).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["file", "kind", "name", "size", "text", "withheld"]);
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_links_out_of_its_holder_is_refused() {
    let (dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(dir.path().join("secret.env"), "TOKEN=1\n").unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret.env"), skill.join("env.md")).unwrap();

    let error = Contents::of_project(&project, "deploy", "env.md").unwrap_err();

    assert_eq!(error.code(), "contents_outside");
    assert!(error.to_string().contains("secret.env"), "{error}");
}

/// Discovery follows the link to read what the skill declares, so the skill
/// is listed; its file is still not read out.
#[cfg(unix)]
#[test]
fn a_manifest_that_links_out_of_its_holder_is_refused() {
    let (dir, project) = fixture();
    let skill = skill_folder(&project, "linked");
    fs::create_dir_all(&skill).unwrap();
    let outside = dir.path().join("SKILL.md");
    fs::write(&outside, SKILL.replace("deploy", "linked")).unwrap();
    std::os::unix::fs::symlink(&outside, skill.join("SKILL.md")).unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "linked", "SKILL.md")),
        "contents_outside"
    );
}

#[cfg(unix)]
#[test]
fn a_folder_that_links_out_of_its_holder_takes_its_files_with_it() {
    let (dir, project) = fixture();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("notes.md"), "private\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, skill_folder(&project, "deploy").join("notes")).unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "deploy", "notes/notes.md")),
        "contents_outside"
    );
}

#[cfg(unix)]
#[test]
fn a_link_that_stays_inside_its_holder_is_followed() {
    let (_dir, project) = fixture();
    let shared = project.root().join("docs");
    fs::create_dir_all(&shared).unwrap();
    fs::write(shared.join("style.md"), "Be brief.\n").unwrap();
    std::os::unix::fs::symlink(
        shared.join("style.md"),
        skill_folder(&project, "deploy").join("style.md"),
    )
    .unwrap();

    let contents = Contents::of_project(&project, "deploy", "style.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("Be brief.\n"));
}

#[test]
fn nothing_there_and_a_folder_are_no_file_to_read() {
    let (_dir, project) = fixture();

    assert_eq!(
        code(Contents::of_project(&project, "deploy", "missing.md")),
        "contents_missing"
    );
    assert_eq!(
        code(Contents::of_project(&project, "deploy", "references")),
        "contents_not_a_file"
    );
}

#[test]
fn a_reference_names_one_artifact_or_is_refused() {
    let (_dir, project) = fixture();
    let rule = project.directory_for(Kind::Rule).unwrap().join("deploy.md");
    fs::write(rule, "Deploy on Tuesdays.\n").unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "nothing", "SKILL.md")),
        "not_found"
    );
    assert_eq!(
        code(Contents::of_project(&project, "deploy", "SKILL.md")),
        "ambiguous_name"
    );
    assert_eq!(
        Contents::of_project(&project, "skill:deploy", "SKILL.md")
            .unwrap()
            .kind,
        Kind::Skill
    );
    assert_eq!(
        Contents::of_project(&project, "rule:deploy", "deploy.md")
            .unwrap()
            .kind,
        Kind::Rule
    );
}

/// A file that does not read is the one most in need of being looked at, as
/// a snapshot lists it under the name its place gives it.
#[test]
fn a_skill_whose_manifest_does_not_read_can_still_be_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "broken");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "no front matter here\n").unwrap();

    let contents = Contents::of_project(&project, "broken", "SKILL.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("no front matter here\n"));
}

#[test]
fn the_workspaces_own_are_read_from_its_library() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();

    let contents = Contents::of_library(&workspace, "deploy", "SKILL.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some(SKILL));
    assert_eq!(
        serde_json::to_value(&contents).unwrap(),
        serde_json::json!({
            "kind": "skill",
            "name": "deploy",
            "file": "SKILL.md",
            "size": SKILL.len(),
            "text": SKILL,
            "withheld": null,
        })
    );
}

/// The default workspace's root is a home: a library file that links to
/// a secret kept there leads out of the library, though not out of the
/// root.
#[cfg(unix)]
#[test]
fn a_workspaces_own_file_that_links_out_of_its_library_is_refused() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = FlayerWorkspace::init(dir.path()).unwrap();
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    let ssh = dir.path().join(".ssh");
    fs::create_dir_all(&ssh).unwrap();
    fs::write(ssh.join("id_ed25519"), "PRIVATE KEY\n").unwrap();
    std::os::unix::fs::symlink(ssh.join("id_ed25519"), skill.join("key.md")).unwrap();

    let error = Contents::of_library(&workspace, "deploy", "key.md").unwrap_err();

    assert_eq!(error.code(), "contents_outside");
    assert!(error.to_string().contains("id_ed25519"), "{error}");
}

#[test]
fn what_is_withheld_says_why_in_one_word() {
    assert_eq!(
        serde_json::to_value([Withheld::Binary, Withheld::TooLarge]).unwrap(),
        serde_json::json!(["binary", "too_large"])
    );
}
