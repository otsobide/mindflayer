//! Unit tests for `catalog.rs`, kept beside it rather than inside it.
//!
//! The integration suite reads whole projects; these hold the pieces it is
//! built of to their rules one at a time: how a reference is read, which
//! folders a project is read from and which agents read each, how a rule is
//! named by its route, what counts as hidden or as a rule's file, and how
//! failures settle.

use super::*;

use std::path::MAIN_SEPARATOR;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, SkillMother};
use crate::settings::Share;
use crate::workspace::Directories;

const DRAWS: usize = 200;

fn listed_failure(path: &str) -> DiscoveryFailure {
    DiscoveryFailure::Directory {
        path: PathBuf::from(path),
        source: io::Error::other("denied"),
    }
}

/// Folders as `read_from` hands them, each as a path from the project with
/// the agents that read it and how it stands.
fn stood(project: &MindProject, kind: Kind) -> Vec<(PathBuf, Vec<Agent>, bool, bool)> {
    read_from(project, kind)
        .into_iter()
        .map(|folder| {
            let path = folder
                .path
                .strip_prefix(project.root())
                .expect("a folder read is the project's")
                .to_path_buf();
            (path, folder.agents, folder.authored, folder.shared)
        })
        .collect()
}

fn skills_of(agent_folder: &str) -> PathBuf {
    Path::new(agent_folder).join("skills")
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn a_bare_name_is_a_reference_to_that_name_in_any_kind() {
    for _ in 0..DRAWS {
        let name = random::kebab();
        let reference = Reference::parse(&name);

        assert_eq!(reference.kind(), None, "{name}");
        assert_eq!(reference.name(), name);
        assert_eq!(reference.typed(), name);
    }
}

#[test]
fn a_name_after_a_kind_s_slug_and_a_colon_is_that_kind_s() {
    for kind in Kind::ALL {
        let name = random::kebab();
        let typed = format!("{}{QUALIFIER}{name}", kind.slug());
        let reference = Reference::parse(&typed);

        assert_eq!(reference.kind(), Some(kind), "{typed}");
        assert_eq!(reference.name(), name, "{typed}");
        assert_eq!(reference.typed(), typed);
    }
}

/// The plural is the kind's folder, which `mind list rules` already takes,
/// so `skills:deploy` qualifies as `skill:deploy` does.
#[test]
fn a_kind_s_folder_name_qualifies_a_reference_as_its_slug_does() {
    for kind in Kind::ALL {
        let typed = format!("{}:deploy", kind.folder());
        let reference = Reference::parse(&typed);

        assert_eq!(reference.kind(), Some(kind), "{typed}");
        assert_eq!(reference.name(), "deploy", "{typed}");
    }
}

/// What is before the first colon is a kind or nothing: a head that names
/// none leaves the whole text the name, so a name that holds a colon is
/// still found by it.
#[test]
fn a_head_that_names_no_kind_leaves_the_whole_text_the_name() {
    for typed in [
        "Skill:deploy",
        "SKILL:deploy",
        "skil:deploy",
        "agent:x",
        ":deploy",
        "c:/rules/x",
    ] {
        let reference = Reference::parse(typed);

        assert_eq!(reference.kind(), None, "{typed}");
        assert_eq!(reference.name(), typed);
    }
}

#[test]
fn only_the_first_colon_qualifies_and_the_rest_are_the_name_s() {
    let reference = Reference::parse("rule:ops:deploy:now");

    assert_eq!(reference.kind(), Some(Kind::Rule));
    assert_eq!(reference.name(), "ops:deploy:now");
}

/// A qualifier with nothing after it names the empty name, which no
/// artifact has: refused by finding nothing, as any other name would be.
#[test]
fn a_kind_with_nothing_after_its_colon_names_the_empty_name() {
    let reference = Reference::parse("skill:");

    assert_eq!(reference.kind(), Some(Kind::Skill));
    assert_eq!(reference.name(), "");
    assert_eq!(reference.typed(), "skill:");
}

#[test]
fn whatever_is_typed_is_kept_as_it_was_typed() {
    for _ in 0..DRAWS {
        let typed = random::pick(&[
            random::text(20),
            format!(
                "{}:{}",
                random::pick(&["skill", "rule", "x"]),
                random::text(10)
            ),
        ]);
        let reference = Reference::parse(&typed);

        assert_eq!(reference.typed(), typed);
        match reference.kind() {
            Some(_) => assert!(
                typed.ends_with(&format!(":{}", reference.name())),
                "{typed:?}"
            ),
            None => assert_eq!(reference.name(), typed),
        }
    }
}

#[test]
fn a_reference_serializes_exactly_what_it_read() {
    let reference = Reference::parse("skills:deploy");

    assert_eq!(
        serde_json::to_string(&reference).unwrap(),
        r#"{"kind":"skill","name":"deploy","typed":"skills:deploy"}"#
    );
}

#[test]
fn no_path_spells_as_nothing() {
    assert_eq!(spelled(&[]), "");
}

#[test]
fn one_path_spells_as_itself() {
    assert_eq!(
        spelled(&[PathBuf::from("/p/.claude/skills/x")]),
        "/p/.claude/skills/x"
    );
}

#[test]
fn two_paths_spell_with_and_between_them() {
    assert_eq!(
        spelled(&[PathBuf::from("a"), PathBuf::from("b")]),
        "a and b"
    );
}

#[test]
fn three_paths_spell_with_commas_and_and_before_the_last() {
    assert_eq!(
        spelled(&[PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")]),
        "a, b and c"
    );
}

#[test]
fn any_number_of_paths_spell_each_in_order_with_one_and() {
    for _ in 0..DRAWS {
        let paths: Vec<PathBuf> = (0..random::between(2, 8))
            .map(|_| PathBuf::from(format!("/{}/{}", random::word(), random::word())))
            .collect();
        let shown: Vec<String> = paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let (last, rest) = shown.split_last().unwrap();

        assert_eq!(
            spelled(&paths),
            format!("{} and {last}", rest.join(", ")),
            "{paths:?}"
        );
    }
}

#[test]
fn a_rule_is_named_by_its_route_below_the_folder_without_its_extension() {
    let root = Path::new("/p/rules");

    assert_eq!(
        route(root, &root.join("deploy.md")),
        Some(String::from("deploy"))
    );
    assert_eq!(
        route(root, &root.join("git").join("no-force.md")),
        Some(String::from("git/no-force"))
    );
}

/// Only the last extension goes: a dot inside the name is the name's.
#[test]
fn a_route_keeps_every_dot_but_the_extension_s() {
    let root = Path::new("/p/rules");

    assert_eq!(
        route(root, &root.join("v1.2.md")),
        Some(String::from("v1.2"))
    );
}

#[test]
fn a_route_is_slash_separated_however_deep_it_goes() {
    for _ in 0..DRAWS {
        let segments: Vec<String> = (0..random::between(1, 5))
            .map(|_| random::kebab())
            .collect();
        let root = PathBuf::from("/p/rules");
        let mut file = root.clone();
        for segment in &segments {
            file.push(segment);
        }
        file.set_extension("md");

        assert_eq!(route(&root, &file), Some(segments.join("/")), "{file:?}");
    }
}

#[test]
fn a_file_outside_the_folder_has_no_route() {
    assert_eq!(
        route(Path::new("/p/rules"), Path::new("/q/deploy.md")),
        None
    );
}

#[cfg(unix)]
#[test]
fn a_file_whose_name_is_not_utf_8_has_no_route() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let root = Path::new("/p/rules");
    let file = root.join(OsStr::from_bytes(b"caf\xe9.md"));

    assert_eq!(route(root, &file), None);
}

#[test]
fn a_rule_s_file_is_told_by_its_extension_in_any_case() {
    for name in ["a.md", "a.MD", "a.Md", "dir/a.mD"] {
        assert!(has_extension(Path::new(name), "md"), "{name}");
    }
}

#[test]
fn a_file_with_another_extension_or_none_is_no_rule_s_file() {
    for name in ["a.mdx", "a.markdown", "a.txt", "a", "a.md.bak", ".md", "md"] {
        assert!(!has_extension(Path::new(name), "md"), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn an_extension_that_is_not_utf_8_is_no_rule_s() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    assert!(!has_extension(
        Path::new(OsStr::from_bytes(b"a.m\xe9")),
        "md"
    ));
}

#[test]
fn a_name_starting_with_a_dot_is_hidden() {
    for path in [".git", ".hidden.md", "rules/.drafts", "/p/rules/.x/y/.z"] {
        assert!(is_hidden(Path::new(path)), "{path}");
    }
}

/// Only the path's own name counts: a hidden folder above it is the walk's
/// to leave out, which it does before it reaches anything inside.
#[test]
fn a_name_without_a_leading_dot_is_not_hidden_wherever_it_is() {
    for path in ["deploy.md", "a.b", "rules/.drafts/x.md", "x.", "/"] {
        assert!(!is_hidden(Path::new(path)), "{path}");
    }
}

/// `.` and `..` have no file name of their own, so they are not read as
/// hidden: a walk never lists them, and no rule is named by one.
#[test]
fn the_current_and_parent_folders_are_not_hidden() {
    for path in [".", ".."] {
        assert!(!is_hidden(Path::new(path)), "{path}");
    }
}

#[test]
fn a_skill_folder_is_read_by_the_agents_that_read_it() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    let root = project.root();

    assert_eq!(
        readers(
            &project,
            Kind::Skill,
            &root.join(skills_of(".claude")),
            &Agent::ALL
        ),
        [Agent::Claude, Agent::OpenCode]
    );
    assert_eq!(
        readers(
            &project,
            Kind::Skill,
            &root.join(skills_of(".agents")),
            &Agent::ALL
        ),
        [Agent::Codex, Agent::OpenCode]
    );
    assert_eq!(
        readers(
            &project,
            Kind::Skill,
            &root.join(skills_of(".opencode")),
            &Agent::ALL
        ),
        [Agent::OpenCode]
    );
    assert!(readers(&project, Kind::Skill, &root.join("skills"), &Agent::ALL).is_empty());
}

#[test]
fn only_the_agents_asked_about_are_said_to_read_a_folder() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    let claude = project.root().join(skills_of(".claude"));

    for _ in 0..50 {
        let asked = random::some_of(&Agent::ALL);
        let expected: Vec<Agent> = [Agent::Claude, Agent::OpenCode]
            .into_iter()
            .filter(|agent| asked.contains(agent))
            .collect();

        assert_eq!(
            readers(&project, Kind::Skill, &claude, &asked),
            expected,
            "{asked:?}"
        );
    }
}

/// Each once and in report order, however the agents were asked about: a
/// project whose settings name one twice still has one reader of it.
#[test]
fn the_readers_of_a_folder_are_each_named_once_in_order() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    let agents = project.root().join(skills_of(".agents"));
    let asked = [Agent::OpenCode, Agent::Codex, Agent::OpenCode, Agent::Codex];

    assert_eq!(
        readers(&project, Kind::Skill, &agents, &asked),
        [Agent::Codex, Agent::OpenCode]
    );
}

#[test]
fn a_folder_spelled_another_way_is_read_by_the_same_agents() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    let respelled = project
        .root()
        .join(".")
        .join(".claude")
        .join("x")
        .join("..")
        .join("skills");

    assert_eq!(
        readers(&project, Kind::Skill, &respelled, &Agent::ALL),
        [Agent::Claude, Agent::OpenCode]
    );
}

#[test]
fn no_agent_is_said_to_read_a_folder_of_rules() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);
    for folder in [".claude/rules", "rules", ".agents/rules"] {
        assert!(
            readers(
                &project,
                Kind::Rule,
                &project.root().join(folder),
                &Agent::ALL
            )
            .is_empty(),
            "{folder}"
        );
    }
}

/// Each target in the order the project gives its targets, by path, then
/// the folder it authors in, which is read last so its copy of a name joins
/// those its agents load.
#[test]
fn a_project_for_every_agent_is_read_in_each_agent_s_folder_then_its_own() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);

    assert_eq!(
        stood(&project, Kind::Skill),
        vec![
            (
                skills_of(".agents"),
                vec![Agent::Codex, Agent::OpenCode],
                false,
                true
            ),
            (
                skills_of(".claude"),
                vec![Agent::Claude, Agent::OpenCode],
                false,
                true
            ),
            (PathBuf::from("skills"), Vec::new(), true, true),
        ]
    );
}

/// Codex and OpenCode are sent one folder, which is read once.
#[test]
fn two_agents_sent_one_folder_have_it_read_once() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(
        &dir.path().join("app"),
        &[Agent::Codex, Agent::OpenCode],
        Share::Local,
    );

    assert_eq!(
        stood(&project, Kind::Skill),
        vec![
            (
                skills_of(".agents"),
                vec![Agent::Codex, Agent::OpenCode],
                false,
                true
            ),
            (PathBuf::from("skills"), Vec::new(), true, true),
        ]
    );
}

/// With no agent to send a skill to, the project's own folder is all it
/// holds, and each skill there is one artifact a file, compared with none.
#[test]
fn a_project_for_no_agent_is_read_only_where_it_authors() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &[], Share::Shared);

    assert_eq!(
        stood(&project, Kind::Skill),
        vec![(PathBuf::from("skills"), Vec::new(), true, false)]
    );
}

/// No agent is sent a rule as a folder, so rules are read where the project
/// keeps them, one artifact a file, as in any project.
#[test]
fn a_project_s_rules_are_read_only_where_it_authors_them() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &Agent::ALL, Share::Local);

    assert_eq!(
        stood(&project, Kind::Rule),
        vec![(PathBuf::from("rules"), Vec::new(), true, false)]
    );
}

#[test]
fn a_project_authoring_in_an_agent_s_folder_reads_it_once_as_its_own() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init_asked(
        dir.path(),
        &[Agent::Claude],
        Share::Local,
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    )
    .unwrap();

    assert_eq!(
        stood(&project, Kind::Skill),
        vec![(skills_of(".claude"), vec![Agent::Claude], true, true)]
    );
}

#[test]
fn a_project_a_mind_toml_marks_is_read_only_from_the_folder_it_names() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::older(&dir.path().join("app"));

    for kind in Kind::ALL {
        let folders = stood(&project, kind);

        assert_eq!(folders.len(), 1, "{kind:?}: {folders:?}");
        let (_, agents, authored, shared) = &folders[0];
        assert!(
            agents.is_empty() && *authored && !*shared,
            "{kind:?}: {folders:?}"
        );
    }
}

#[test]
fn the_folders_a_project_is_read_from_are_the_ones_discovery_reads() {
    for _ in 0..20 {
        let dir = TempDir::new().unwrap();
        let project = ProjectMother::random(dir.path());
        for kind in Kind::ALL {
            let read: Vec<PathBuf> = read_from(&project, kind)
                .into_iter()
                .map(|folder| folder.path)
                .collect();

            assert_eq!(Catalog::folders(&project, kind), read, "{project:?}");
        }
    }
}

#[test]
fn a_folder_that_is_not_there_is_looked_in_and_nothing_else() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("gone");
    for kind in Kind::ALL {
        assert_eq!(
            Catalog::looked_in(&missing, kind),
            std::slice::from_ref(&missing),
            "{kind:?}"
        );
    }
}

/// A skill is a folder: each folder in the kind's is looked in, one with
/// no manifest yet among them, and a file beside them is not.
#[test]
fn a_folder_of_skills_is_looked_in_and_each_folder_in_it_in_order() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("skills");
    for name in ["beta", "alpha", ".hidden"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    write(
        &root.join("alpha").join("SKILL.md"),
        &SkillMother::create("alpha", "A"),
    );
    write(&root.join("alpha").join("assets").join("x.md"), "x");
    write(&root.join("README.md"), "not a skill");

    assert_eq!(
        Catalog::looked_in(&root, Kind::Skill),
        [
            root.clone(),
            root.join(".hidden"),
            root.join("alpha"),
            root.join("beta"),
        ]
    );
}

/// A rule is found at any depth, so every folder the walk goes into is
/// looked in, and none it leaves alone: a hidden one, or one reached
/// through a link.
#[test]
fn a_folder_of_rules_is_looked_in_and_every_folder_the_walk_enters() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    write(&root.join("git").join("deep").join("x.md"), "x");
    write(&root.join(".drafts").join("y.md"), "y");
    write(&root.join("top.md"), "t");
    fs::create_dir_all(root.join("empty")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("git"), root.join("linked")).unwrap();

    assert_eq!(
        Catalog::looked_in(&root, Kind::Rule),
        [
            root.clone(),
            root.join("empty"),
            root.join("git"),
            root.join("git").join("deep"),
        ]
    );
}

#[test]
fn locating_skills_finds_each_folder_holding_a_manifest_in_order() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("skills");
    let mut names: Vec<String> = (0..random::between(1, 6))
        .map(|_| random::kebab())
        .collect();
    names.sort();
    names.dedup();
    for name in &names {
        write(&root.join(name).join("SKILL.md"), "not even front matter");
    }
    fs::create_dir_all(root.join("zz-no-manifest")).unwrap();
    write(&root.join("stray.md"), "x");

    let (found, failures) = Catalog::locate(&root, Kind::Skill);

    let expected: Vec<PathBuf> = names.iter().map(|name| root.join(name)).collect();
    assert_eq!(found, expected, "{names:?}");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn locating_rules_finds_every_markdown_file_but_the_hidden_ones() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    write(&root.join("b.md"), "b");
    write(&root.join("a").join("c.MD"), "c");
    write(&root.join("a").join("notes.txt"), "n");
    write(&root.join(".hidden.md"), "h");
    write(&root.join(".drafts").join("d.md"), "d");

    let (found, failures) = Catalog::locate(&root, Kind::Rule);

    assert_eq!(found, [root.join("a").join("c.MD"), root.join("b.md")]);
    assert!(failures.is_empty(), "{failures:?}");
}

/// A rule shared by a link is still a rule: the link is followed to tell
/// a file from a folder, never into a folder.
#[cfg(unix)]
#[test]
fn a_rule_reached_through_a_link_is_located_and_a_linked_folder_is_not_walked() {
    let dir = TempDir::new().unwrap();
    let elsewhere = dir.path().join("elsewhere");
    write(&elsewhere.join("shared.md"), "s");
    write(&elsewhere.join("inner").join("deep.md"), "d");
    let root = dir.path().join("rules");
    fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("shared.md"), root.join("shared.md")).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("inner"), root.join("inner")).unwrap();

    let (found, _) = Catalog::locate(&root, Kind::Rule);

    assert_eq!(found, [root.join("shared.md")]);
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_listed_is_a_failure_and_nothing_found() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    write(&root.join("a.md"), "a");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
    // A process that may list anything, as root may, finds what is there.
    let unlisted = fs::read_dir(&root).is_err();

    let (found, failures) = Catalog::locate(&root, Kind::Rule);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

    if unlisted {
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].path(), root);
        assert_eq!(failures[0].code(), "folder_unlisted");
    }
}

#[test]
fn an_empty_catalog_holds_nothing_of_any_kind() {
    let catalog = Catalog::default();

    assert!(catalog.is_empty());
    assert!(catalog.kinds().is_empty());
    assert!(catalog.failures().is_empty());
    assert!(catalog.find(&Reference::parse("anything")).is_empty());
}

/// Failures settle by path, each path once, so a ledger that failed for
/// every project it was asked about is said once.
#[test]
fn failures_settle_in_path_order_each_path_once() {
    let catalog = Catalog::default()
        .failing(listed_failure("/p/b"))
        .failing(listed_failure("/p/a"))
        .failing(listed_failure("/p/b"))
        .failing(DiscoveryFailure::UnusableName {
            path: PathBuf::from("/p/a"),
        });

    let paths: Vec<&Path> = catalog
        .failures()
        .iter()
        .map(DiscoveryFailure::path)
        .collect();
    assert_eq!(paths, [Path::new("/p/a"), Path::new("/p/b")]);
    assert!(
        matches!(catalog.failures()[0], DiscoveryFailure::Directory { .. }),
        "the first said of a path is kept: {:?}",
        catalog.failures()
    );
}

#[test]
fn two_catalogs_merged_settle_their_failures_together() {
    let one = Catalog::default().failing(listed_failure("/p/c"));
    let other = Catalog::default()
        .failing(listed_failure("/p/a"))
        .failing(listed_failure("/p/c"));

    let (artifacts, failures) = one.merge(other).into_parts();

    assert!(artifacts.is_empty());
    let paths: Vec<&Path> = failures.iter().map(DiscoveryFailure::path).collect();
    assert_eq!(paths, [Path::new("/p/a"), Path::new("/p/c")]);
}

#[test]
fn each_discovery_failure_of_its_own_gives_its_own_code() {
    let failures = [
        (listed_failure("/p"), "folder_unlisted"),
        (
            DiscoveryFailure::UnusableName {
                path: PathBuf::from("/p"),
            },
            "route_not_utf8",
        ),
        (
            DiscoveryFailure::Diverged {
                kind: Kind::Skill,
                name: String::from("deploy"),
                paths: vec![PathBuf::from("/p/a"), PathBuf::from("/p/b")],
            },
            "copies_diverged",
        ),
        (
            DiscoveryFailure::Uncompared {
                name: String::from("deploy"),
                path: PathBuf::from("/p"),
                source: io::Error::other("a link"),
            },
            "copy_uncompared",
        ),
    ];
    for (failure, code) in failures {
        assert_eq!(failure.code(), code, "{failure:?}");
    }
    assert_eq!(
        DiscoveryFailure::CODES,
        [
            "folder_unlisted",
            "route_not_utf8",
            "copies_diverged",
            "copy_uncompared"
        ]
    );
}

#[test]
fn a_ledger_that_cannot_be_read_is_said_with_the_ledger_s_code_and_file() {
    let failure = DiscoveryFailure::Ledger(LedgerError::Unmade {
        path: PathBuf::from("/w/.mindflayer/mindflayer.db"),
    });

    assert_eq!(failure.code(), "ledger_unmade");
    assert_eq!(failure.path(), Path::new("/w/.mindflayer/mindflayer.db"));
    assert!(failure.is_unreadable());
}

/// The first copy an agent reads is where the failure is said, so
/// settling by path keeps it beside whatever else is wrong there.
#[test]
fn copies_that_differ_are_about_the_first_and_spelled_in_order() {
    let failure = DiscoveryFailure::Diverged {
        kind: Kind::Skill,
        name: String::from("deploy"),
        paths: vec![
            PathBuf::from("/p/.claude/skills/deploy"),
            PathBuf::from("/p/.agents/skills/deploy"),
            PathBuf::from("/p/.opencode/skills/deploy"),
        ],
    };

    assert_eq!(failure.path(), Path::new("/p/.claude/skills/deploy"));
    assert_eq!(
        failure.to_string(),
        "the copies of `deploy` at /p/.claude/skills/deploy, /p/.agents/skills/deploy and \
         /p/.opencode/skills/deploy differ, so the agents reading them do not all load the same skill"
    );
}

/// What a count of unreadable files counts: every failure but copies that
/// were read and found to disagree.
#[test]
fn only_copies_that_differ_were_read() {
    let diverged = DiscoveryFailure::Diverged {
        kind: Kind::Skill,
        name: String::from("deploy"),
        paths: vec![PathBuf::from("/p/a")],
    };
    let unreadable = [
        listed_failure("/p"),
        DiscoveryFailure::UnusableName {
            path: PathBuf::from("/p"),
        },
        DiscoveryFailure::Uncompared {
            name: String::from("deploy"),
            path: PathBuf::from("/p"),
            source: io::Error::other("a link"),
        },
    ];

    assert!(!diverged.is_unreadable());
    for failure in unreadable {
        assert!(failure.is_unreadable(), "{failure:?}");
    }
}

#[test]
fn a_name_that_is_not_utf_8_says_why_it_cannot_be_named() {
    let failure = DiscoveryFailure::UnusableName {
        path: PathBuf::from(format!("{MAIN_SEPARATOR}p{MAIN_SEPARATOR}x.md")),
    };

    assert!(
        failure
            .to_string()
            .ends_with(": its route is not valid UTF-8, so it cannot be named"),
        "{failure}"
    );
}
