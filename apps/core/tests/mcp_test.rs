//! MCP servers as a kind of the library: where each marker keeps them, as
//! it was written, and what listing, showing, validating, renaming and
//! removing one does, through the lifecycle every kind goes through.
//!
//! A marker written before the kind came says nothing of it, and must not
//! be read as if it named a folder: a workspace keeps its own in its
//! library whatever its marker says (D-05), and a project only where its
//! settings name one, so a folder called `mcp` that holds a server's code is
//! nobody's definitions.

use std::fs;
use std::path::Path;

use mindweaver_core::lifecycle;
use mindweaver_core::mother::{
    McpMother, MindflayerMother, ProjectMother, SkillMother, WorkspaceMother,
};
use mindweaver_core::reading::Shown;
use mindweaver_core::{
    create, Catalog, CreateError, Declared, Directories, DiscoveryFailure, Kind, LifecycleError,
    MindProject, Reference, Share, ValidationIssue, WeaverWorkspace, WorkspaceError,
};
use tempfile::TempDir;

/// Write each of `files`, a route and what it holds, under `root`.
fn write(root: &Path, files: &[(&str, &str)]) {
    for (route, text) in files {
        let path = root.join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// What a repository implementing a server keeps at `mcp/`: its code, and
/// definitions it ships for its users, none of them the project's own.
fn unrelated_mcp_folder(root: &Path) {
    write(
        root,
        &[
            (
                "mcp/Cargo.toml",
                "[package]\nname = \"server\"\nversion = \"0.1.0\"\n",
            ),
            ("mcp/src/main.rs", "fn main() {}\n"),
            ("mcp/github.toml", &McpMother::stdio("github")),
            ("mcp/broken.toml", "format = 1\nname = \n"),
        ],
    );
}

/// Every MCP server `holder` is read as holding, by name, with what could
/// not be read.
fn servers(holder: &MindProject) -> (Vec<String>, Vec<String>) {
    let catalog = Catalog::discover_kinds(std::slice::from_ref(holder), &[Kind::Mcp]);
    let names = catalog
        .artifacts()
        .iter()
        .map(|artifact| artifact.name().to_string())
        .collect();
    let failures = catalog
        .failures()
        .iter()
        .map(|failure| failure.to_string())
        .collect();
    (names, failures)
}

// ---------------------------------------------------------------------------
// Where each marker keeps them
// ---------------------------------------------------------------------------

#[test]
fn a_new_workspace_keeps_its_servers_in_its_library_and_never_beside_its_projects() {
    let (dir, workspace) = WorkspaceMother::empty();
    let library = dir.path().join(".mindweaver/library/mcp");
    write(&library, &[("deploy.toml", &McpMother::http("deploy"))]);
    unrelated_mcp_folder(dir.path());

    assert_eq!(workspace.own().directory_for(Kind::Mcp), Some(library));
    assert_eq!(
        servers(&workspace.own()),
        (vec![String::from("deploy")], vec![])
    );
}

#[test]
fn a_marker_a_mindflayer_build_wrote_reads_its_library_and_never_the_root() {
    let (dir, workspace) = MindflayerMother::empty();
    let marker = fs::read_to_string(dir.path().join(".mindflayer/flayer.toml")).unwrap();
    assert!(!marker.contains("mcp"), "{marker}");
    write(
        &dir.path().join(".mindflayer/library/mcp"),
        &[("deploy.toml", &McpMother::sse("deploy"))],
    );
    unrelated_mcp_folder(dir.path());

    assert_eq!(
        workspace.own().directory_for(Kind::Mcp),
        Some(dir.path().join(".mindflayer/library/mcp"))
    );
    assert_eq!(
        servers(&workspace.own()),
        (vec![String::from("deploy")], vec![])
    );
}

#[test]
fn a_workspace_keeping_its_skills_beside_its_projects_keeps_its_servers_in_its_library() {
    let dir = TempDir::new().unwrap();
    let beside = Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules");
    let (workspace, _) = WeaverWorkspace::init_with(dir.path(), &beside).unwrap();
    unrelated_mcp_folder(dir.path());

    assert_eq!(
        workspace.own().directory_for(Kind::Mcp),
        Some(dir.path().join(".mindweaver/library/mcp"))
    );
    assert_eq!(servers(&workspace.own()), (vec![], vec![]));
}

#[test]
fn a_new_workspace_marker_names_no_folder_for_servers_and_says_a_kind_unnamed_is_in_the_library() {
    let (dir, _) = WorkspaceMother::empty();

    let marker = fs::read_to_string(dir.path().join(".mindweaver/weaver.toml")).unwrap();

    assert!(!marker.contains("mcp"), "{marker}");
    assert!(
        marker.contains("a kind not named\n# here is kept there too"),
        "{marker}"
    );
}

#[test]
fn a_version_1_or_2_project_with_an_unrelated_mcp_folder_lists_nothing_of_it_and_says_nothing() {
    let dir = TempDir::new().unwrap();
    let first = ProjectMother::version_1(&dir.path().join("first"));
    let second = ProjectMother::older(&dir.path().join("second"));

    for project in [first, second] {
        unrelated_mcp_folder(project.root());
        // Where a version 1 marker kept every kind it knew, too.
        write(
            &project.mind_dir(),
            &[("mcp/github.toml", &McpMother::stdio("github"))],
        );

        assert_eq!(project.directory_for(Kind::Mcp), None, "{project:?}");
        assert_eq!(
            Catalog::folders(&project, Kind::Mcp),
            Vec::<std::path::PathBuf>::new()
        );
        assert_eq!(servers(&project), (vec![], vec![]), "{project:?}");
        let every = Catalog::discover(std::slice::from_ref(&project));
        assert!(
            every
                .artifacts()
                .iter()
                .all(|artifact| artifact.kind() != Kind::Mcp),
            "{project:?}"
        );
        assert_eq!(every.failures().len(), 0, "{:?}", every.failures());
    }
}

#[test]
fn a_project_marked_by_its_settings_reads_servers_only_where_they_name_a_folder() {
    let dir = TempDir::new().unwrap();
    let silent = ProjectMother::create(&dir.path().join("silent"), &[], Share::Local);
    let naming = ProjectMother::with_mcp(&dir.path().join("naming"), &[], Share::Local);

    unrelated_mcp_folder(silent.root());
    assert_eq!(silent.directory_for(Kind::Mcp), None);
    assert_eq!(servers(&silent), (vec![], vec![]));

    write(
        naming.root(),
        &[("mcp/github.toml", &McpMother::stdio("github"))],
    );
    assert_eq!(
        naming.directory_for(Kind::Mcp),
        Some(naming.root().join("mcp"))
    );
    assert_eq!(servers(&naming), (vec![String::from("github")], vec![]));
}

#[test]
fn mind_init_names_no_folder_for_servers_and_makes_none() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(dir.path(), &[], Share::Local);

    let settings = fs::read_to_string(project.marker_path()).unwrap();

    assert!(!settings.contains("mcp"), "{settings}");
    assert!(!dir.path().join("mcp").exists());
}

// ---------------------------------------------------------------------------
// Listing and validating
// ---------------------------------------------------------------------------

/// A project whose settings name `mcp` for its servers, and that folder.
fn authoring() -> (TempDir, MindProject, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::with_mcp(&dir.path().join("app"), &[], Share::Local);
    let folder = project.directory_for(Kind::Mcp).unwrap();
    (dir, project, folder)
}

#[test]
fn a_server_is_one_file_directly_in_its_folder_named_by_its_stem() {
    let (_dir, project, folder) = authoring();
    let description = SkillMother::description();
    write(
        &folder,
        &[
            (
                "github.toml",
                &McpMother::create("github", &description, "stdio"),
            ),
            ("linear.toml", &McpMother::http("linear")),
            ("team/nested.toml", &McpMother::sse("nested")),
            (".hidden.toml", &McpMother::sse("hidden")),
            ("notes.md", "# Not a definition\n"),
        ],
    );

    let catalog = Catalog::discover(std::slice::from_ref(&project));

    let listed: Vec<(Kind, &str, Option<&str>)> = catalog
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind() == Kind::Mcp)
        .map(|artifact| {
            (
                artifact.kind(),
                artifact.name().as_str(),
                artifact.summary(),
            )
        })
        .collect();
    assert_eq!(listed[0], (Kind::Mcp, "github", Some(description.as_str())));
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(listed[1].1, "linear");
    let github = &catalog.artifacts()[0];
    assert_eq!(github.path(), folder.join("github.toml"));
    assert_eq!(github.file(), folder.join("github.toml"));
    assert!(matches!(github.declared(), Declared::Mcp(_)));
    assert_eq!(github.validate(), []);
    assert_eq!(catalog.failures().len(), 0, "{:?}", catalog.failures());
}

#[test]
fn a_file_that_is_no_definition_is_a_failure_by_its_code_and_hides_none_beside_it() {
    let (_dir, project, folder) = authoring();
    let newer = McpMother::in_format("later", "2");
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("later.toml", &newer),
            ("broken.toml", "format = 1\nname = \n"),
        ],
    );

    let catalog = Catalog::discover_kinds(std::slice::from_ref(&project), &[Kind::Mcp]);

    let names: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|artifact| artifact.name().as_str())
        .collect();
    assert_eq!(names, ["github"]);
    let codes: Vec<(String, &str)> = catalog
        .failures()
        .iter()
        .map(|failure| {
            let DiscoveryFailure::Artifact(error) = failure else {
                panic!("{failure:?}");
            };
            (
                error
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                error.code(),
            )
        })
        .collect();
    assert_eq!(
        codes,
        [
            (String::from("broken.toml"), "mcp_unparsed"),
            (String::from("later.toml"), "mcp_format_too_new"),
        ]
    );
    assert!(
        catalog.failures()[1]
            .to_string()
            .ends_with("written in format 2, by a newer Mindweaver; this one reads format 1"),
        "{}",
        catalog.failures()[1]
    );
}

#[test]
fn validate_holds_a_server_to_its_file_its_name_rule_and_its_format() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("gh.toml", &McpMother::stdio("github")),
            ("Bad_Name.toml", &McpMother::sse("Bad_Name")),
            ("bare.toml", &McpMother::incomplete("bare", "http")),
        ],
    );

    let catalog = Catalog::discover_kinds(std::slice::from_ref(&project), &[Kind::Mcp]);
    let issues = |name: &str| {
        catalog
            .artifacts()
            .iter()
            .find(|artifact| artifact.name() == name)
            .unwrap()
            .validate()
    };

    assert_eq!(
        issues("gh"),
        [ValidationIssue::NameFileMismatch {
            name: String::from("github"),
            file: String::from("gh.toml"),
        }]
    );
    assert_eq!(
        issues("Bad_Name"),
        [ValidationIssue::NameNotKebabCase {
            segment: String::from("Bad_Name"),
        }]
    );
    assert_eq!(
        issues("bare"),
        [ValidationIssue::TransportIncomplete {
            transport: String::from("http"),
            field: String::from("url"),
        }]
    );
    assert!(issues("gh").iter().all(|issue| !issue.is_warning()));
}

#[test]
fn show_says_what_it_is_for_and_prints_its_file_whole() {
    let (_dir, project, folder) = authoring();
    let description = SkillMother::description();
    let text = McpMother::create("github", &description, "http");
    write(&folder, &[("github.toml", &text)]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    let shown = Shown::of(target.artifact.as_ref().unwrap()).unwrap();

    assert_eq!(shown.kind, Kind::Mcp);
    assert_eq!(shown.description.as_deref(), Some(description.as_str()));
    assert_eq!(shown.summary.as_deref(), Some(description.as_str()));
    assert_eq!(shown.file, folder.join("github.toml"));
    assert_eq!(shown.body, text);
    assert_eq!((shown.allowed_tools, shown.paths), (None, None));
}

// ---------------------------------------------------------------------------
// Writing: add, rename, remove
// ---------------------------------------------------------------------------

#[test]
fn add_is_refused_for_a_server_saying_where_to_write_one() {
    let (_dir, project, folder) = authoring();

    let error = create(&project, Kind::Mcp, "github", "GitHub").unwrap_err();

    assert!(
        matches!(error, CreateError::McpUnscaffolded { .. }),
        "{error}"
    );
    assert_eq!(error.code(), "add_mcp_unscaffolded");
    assert!(
        error.to_string().ends_with(&format!(
            "write its definition as <name>.toml in {}",
            folder.display()
        )),
        "{error}"
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

#[test]
fn add_is_refused_in_a_project_that_keeps_no_servers_before_anything_else() {
    let dir = TempDir::new().unwrap();
    let older = ProjectMother::older(&dir.path().join("older"));
    let settings = ProjectMother::create(&dir.path().join("settings"), &[], Share::Local);

    for project in [older, settings] {
        let error = create(&project, Kind::Mcp, "github", "GitHub").unwrap_err();

        assert!(
            matches!(
                error,
                CreateError::Workspace(WorkspaceError::NoDirectory {
                    kind: Kind::Mcp,
                    ..
                })
            ),
            "{error}"
        );
        assert_eq!(error.code(), "no_folder");
        assert!(!project.root().join("mcp").exists());
    }
}

#[test]
fn rename_moves_the_file_and_rewrites_its_name_keeping_every_other_byte() {
    let (_dir, project, folder) = authoring();
    let text = McpMother::stdio("github").replacen(
        "name = \"github\"\n",
        "name = \"github\" # what agents key it by\n",
        1,
    );
    write(&folder, &[("github.toml", &format!("# Ours.\n{text}"))]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    let renamed = lifecycle::rename(None, &project, &target, "octocat").unwrap();

    assert!(!folder.join("github.toml").exists());
    let after = fs::read_to_string(folder.join("octocat.toml")).unwrap();
    assert_eq!(
        after,
        format!("# Ours.\n{text}").replacen("name = \"github\" #", "name = \"octocat\" #", 1)
    );
    assert_eq!(renamed.target.name, "octocat");
    assert_eq!(renamed.target.artifact.unwrap().validate(), []);
}

#[test]
fn rename_refuses_a_route_a_server_it_cannot_read_and_a_name_taken() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("linear.toml", &McpMother::http("linear")),
            ("later.toml", &McpMother::in_format("later", "2")),
        ],
    );
    let github = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
    let later = lifecycle::find(&project, &Reference::parse("mcp:later")).unwrap();

    let nested = lifecycle::rename(None, &project, &github, "team/github").unwrap_err();
    assert!(
        matches!(nested, LifecycleError::McpNested { .. }),
        "{nested}"
    );
    assert_eq!(nested.code(), "rename_mcp_nested");
    let taken = lifecycle::rename(None, &project, &github, "linear").unwrap_err();
    assert!(matches!(taken, LifecycleError::Exists { .. }), "{taken}");
    let unread = lifecycle::rename(None, &project, &later, "sooner").unwrap_err();
    assert_eq!(unread.code(), "rename_definition_unreadable");
    assert!(folder.join("github.toml").is_file() && folder.join("later.toml").is_file());
}

#[test]
fn remove_takes_the_one_file_away() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("linear.toml", &McpMother::http("linear")),
        ],
    );
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    assert_eq!(lifecycle::files(&target), 1);
    lifecycle::remove(None, &project, &target).unwrap();

    assert!(!folder.join("github.toml").exists());
    assert!(folder.join("linear.toml").is_file());
}

#[test]
fn contents_given_whole_are_held_to_the_format_and_to_the_files_name() {
    let (_dir, project, folder) = authoring();
    write(&folder, &[("github.toml", &McpMother::stdio("github"))]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
    let replace = |text: &str| {
        lifecycle::replace_contents(None, &project, &target, text.as_bytes(), None)
            .map(|edited| edited.changed)
    };

    let renamed = replace(&McpMother::stdio("octocat")).unwrap_err();
    assert_eq!(renamed.code(), "edit_mcp_name_changed");
    let unread = replace("format = 1\nname = \n").unwrap_err();
    assert_eq!(unread.code(), "edit_definition_unreadable");
    let incomplete = replace(&McpMother::incomplete("github", "stdio")).unwrap_err();
    assert_eq!(incomplete.code(), "edit_contents_unfit");
    assert!(
        incomplete.to_string().contains("has no `command`"),
        "{incomplete}"
    );

    let http = McpMother::http("github");
    assert!(replace(&http).unwrap());
    assert_eq!(
        fs::read_to_string(folder.join("github.toml")).unwrap(),
        http
    );
}
