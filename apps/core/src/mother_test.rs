//! The mothers make what they say they make.
//!
//! A test built on a mother trusts it the way it trusts a fixture written by
//! hand, and a mother that drifted (a "valid" name the rules now refuse, a
//! URL form whose clean spelling is no longer what redacting leaves) would
//! make every such test pass or fail for the wrong reason. Each is held here
//! to the rule it claims to follow, through core's own judgement of it.

use std::collections::BTreeSet;
use std::fs;

use super::*;
use crate::artifact::{name_issues, strict_issues, ValidationIssue};
use crate::frontmatter;
use crate::kind::Kind;
use crate::names::Names;
use crate::rule::RuleManifest;
use crate::settings::{Settings, Share};
use crate::skill::SkillManifest;
use crate::url::{carries_credentials, redacted};
use crate::workspace::{MindProject, WeaverWorkspace};

const DRAWS: usize = 100;

fn manifest(text: &str) -> SkillManifest {
    let document = frontmatter::split(text).expect("a skill's file has front matter");
    SkillManifest::parse(document.front_matter).expect("its front matter parses")
}

#[test]
fn each_credentialed_url_is_redacted_to_the_clean_url_beside_it() {
    for url in UrlMother::every_credentialed() {
        assert!(url.typed.contains(&url.secret), "{url:?}");
        assert!(!url.clean.contains(&url.secret), "{url:?}");
        assert_eq!(redacted(&url.typed), url.clean, "{url:?}");
    }
}

#[test]
fn the_credentialed_forms_are_all_different_with_secrets_of_their_own() {
    let every = UrlMother::every_credentialed();
    let typed: BTreeSet<&str> = every.iter().map(|url| url.typed.as_str()).collect();
    let secrets: BTreeSet<&str> = every.iter().map(|url| url.secret.as_str()).collect();

    assert_eq!(typed.len(), every.len());
    assert_eq!(secrets.len(), every.len());
}

#[test]
fn no_clean_url_carries_a_credential() {
    for _ in 0..DRAWS {
        for url in UrlMother::every_clean() {
            assert!(!carries_credentials(&url), "{url}");
        }
    }
}

#[test]
fn a_quoted_url_is_redacted_to_the_clean_words_beside_it() {
    for _ in 0..DRAWS {
        let quoted = UrlMother::quoted(&UrlMother::credentialed());

        assert_eq!(redacted(&quoted.typed), quoted.clean, "{quoted:?}");
    }
}

#[test]
fn every_text_a_hash_mother_calls_no_hash_is_refused_for_its_own_reason() {
    let not = ContentHashMother::not_hashes();
    let reasons: BTreeSet<&str> = not.iter().map(|(why, _)| *why).collect();

    assert_eq!(reasons.len(), not.len(), "a reason named twice");
    for (why, text) in not {
        assert!(crate::value::ContentHash::parse(&text).is_none(), "{why}");
    }
}

#[test]
fn every_text_an_id_mother_calls_no_id_is_refused() {
    for (why, text) in ProjectIdMother::not_ids() {
        assert!(crate::value::ProjectId::parse(&text).is_none(), "{why}");
    }
}

#[test]
fn every_unwritable_name_breaks_the_name_rule() {
    for (why, name) in ArtifactNameMother::unwritable() {
        assert!(!name_issues(&name).is_empty(), "{why}: {name:?}");
    }
}

#[test]
fn a_random_skill_is_one_mind_add_would_write() {
    for _ in 0..DRAWS {
        let (name, text) = SkillMother::random();

        let manifest = manifest(&text);
        assert_eq!(manifest.name, name);
        assert_eq!(strict_issues(&manifest, &name), [], "{text}");
    }
}

#[test]
fn a_skills_named_shapes_break_the_rule_they_are_named_for() {
    let name = random::kebab();

    let long = manifest(&SkillMother::with_long_description(&name));
    assert!(strict_issues(&long, &name)
        .iter()
        .any(|issue| matches!(issue, ValidationIssue::DescriptionTooLong { .. })));
    assert!(frontmatter::split(&SkillMother::without_front_matter(&name)).is_err());
    assert!(SkillManifest::parse(
        frontmatter::split(&SkillMother::without_description(&name))
            .unwrap()
            .front_matter
    )
    .is_err());
}

#[test]
fn a_rule_has_something_to_say_and_a_scoped_one_says_which_files() {
    for _ in 0..DRAWS {
        let rule = RuleMother::random();
        assert!(
            crate::artifact::rule_text_issues(&rule).is_empty(),
            "{rule}"
        );
        assert!(!RuleManifest::read(&rule).0.is_scoped(), "{rule}");
    }
    let scoped = RuleMother::scoped("tests", &["**/*_test.rs", "tests/**"]);
    let (manifest, body) = RuleManifest::read(&scoped);

    assert!(manifest.is_scoped());
    assert_eq!(
        manifest.paths.stated().map(Vec::as_slice),
        Some(&[String::from("**/*_test.rs"), String::from("tests/**")][..])
    );
    assert_eq!(body.trim_start(), RuleMother::create("tests"));
}

#[test]
fn every_definition_the_mother_makes_reads_as_the_transport_it_is_named_for() {
    use crate::mcp::{McpDefinition, Transport};

    let name = random::kebab();
    for (text, slug) in [
        (McpMother::stdio(&name), "stdio"),
        (McpMother::http(&name), "http"),
        (McpMother::sse(&name), "sse"),
    ] {
        let definition = McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(definition.name, name.as_str());
        assert_eq!(definition.transport.slug(), slug);
        assert!(!definition.inputs.is_empty(), "{slug}");
        assert_eq!(definition.issues(), [], "{text}");
    }
    let sse = McpDefinition::parse(&McpMother::sse(&name)).unwrap();
    assert!(matches!(sse.transport, Transport::Sse(_)));
    for _ in 0..DRAWS {
        let (name, text) = McpMother::random();
        assert!(name_issues(&name).is_empty(), "{name}");
        assert_eq!(McpDefinition::parse(&text).unwrap().issues(), [], "{text}");
    }
}

#[test]
fn a_definitions_named_shapes_break_the_rule_they_are_named_for() {
    use crate::mcp::{McpDefinition, McpError};

    let name = random::kebab();

    assert!(matches!(
        McpDefinition::parse(&McpMother::in_format(&name, "2")),
        Err(McpError::FormatTooNew { found: 2 })
    ));
    for transport in ["stdio", "http", "sse"] {
        let incomplete = McpDefinition::parse(&McpMother::incomplete(&name, transport)).unwrap();
        assert!(matches!(
            &incomplete.issues()[..],
            [ValidationIssue::TransportIncomplete { .. }]
        ));
    }
    let unreferenced = McpDefinition::parse(&McpMother::unreferenced(&name)).unwrap();
    assert!(unreferenced
        .issues()
        .iter()
        .any(|issue| matches!(issue, ValidationIssue::InputUnreferenced { .. })));
}

#[test]
fn input_and_variable_names_are_drawn_as_they_are_usually_written() {
    for _ in 0..DRAWS {
        let input = InputNameMother::random();
        let variable = VariableNameMother::random();

        assert!(
            input.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{input}"
        );
        assert!(
            variable.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
            "{variable}"
        );
        assert_eq!(InputNameMother::create(&input), input);
        assert_eq!(VariableNameMother::create(&variable), variable);
    }
}

#[test]
fn random_settings_are_written_and_read_back_as_they_were() {
    let dir = tempfile::TempDir::new().unwrap();
    for _ in 0..DRAWS {
        let settings = SettingsMother::random();
        let path = dir.path().join(crate::settings::SETTINGS_FILE);
        fs::write(&path, settings.text().unwrap()).unwrap();

        assert_eq!(Settings::read(&path).unwrap(), settings);
    }
}

#[test]
fn an_install_record_mother_files_each_copy_where_its_kind_goes() {
    let skill = InstallRecordMother::random();
    assert_eq!(skill.kind, Kind::Skill);
    assert!(skill.content_hash.is_some() && skill.origin.is_some());
    assert!(skill
        .targets
        .iter()
        .all(|target| target.path.ends_with(skill.name.as_str()) && target.content_hash.is_none()));

    let route = ArtifactNameMother::route();
    let rule = InstallRecordMother::rule(&route);
    assert_eq!(rule.kind, Kind::Rule);
    assert!(rule
        .targets
        .iter()
        .all(|target| target.content_hash.is_some()));
    assert_ne!(rule.targets[0].content_hash, rule.targets[1].content_hash);

    let old = InstallRecordMother::before_hashes("deploy");
    assert_eq!((old.origin, old.content_hash), (None, None));
}

#[test]
fn a_gathered_mother_files_what_it_gathered_under_its_sources_folder() {
    for _ in 0..DRAWS {
        let gathered = GatheredMother::random();

        assert!(
            gathered
                .path
                .ends_with(&format!("{}/{}", gathered.source.directory, gathered.name)),
            "{gathered:?}"
        );
        assert_eq!(gathered.revision, gathered.source.revision);
    }
}

#[test]
fn an_entry_mother_holds_no_credential() {
    for _ in 0..DRAWS {
        let entry = EntryMother::random();

        for text in entry.target.iter().chain(entry.detail.iter()) {
            assert!(!carries_credentials(text), "{entry:?}");
        }
    }
}

#[test]
fn the_fixture_mothers_make_a_workspace_a_project_and_a_repository() {
    let (dir, workspace) = WorkspaceMother::empty();
    assert!(WeaverWorkspace::open(dir.path()).is_ok());

    let project = ProjectMother::random(dir.path());
    assert!(MindProject::is_marked(project.root()));
    assert!(project.settings().is_some());

    let older = ProjectMother::older(&dir.path().join("older"));
    assert!(MindProject::is_marked(older.root()));
    assert!(older.settings().is_none());

    let first = ProjectMother::version_1(&dir.path().join("first"));
    assert_eq!(first.config().version(), 1);
    assert_eq!(
        first.directory_for(Kind::Skill),
        Some(first.mind_dir().join("skills"))
    );

    let authoring = ProjectMother::with_mcp(&dir.path().join("authoring"), &[], Share::Local);
    assert_eq!(
        authoring.directory_for(Kind::Mcp),
        Some(authoring.root().join("mcp"))
    );

    let mut workspace = workspace;
    let linked = ProjectMother::linked(&mut workspace, "linked");
    assert!(workspace.is_linked(linked.root()));

    let repository = GitRepositoryMother::with_skills(&["deploy"]);
    let opened = gix::open(repository.path()).unwrap();
    assert!(opened.head_id().is_ok());
    assert!(repository.path().join("skills/deploy/SKILL.md").is_file());
}

#[test]
fn a_mindflayer_mother_makes_a_workspace_under_the_old_names() {
    let (dir, workspace) = MindflayerMother::empty();

    assert!(dir.path().join(".mindflayer/flayer.toml").is_file());
    assert!(!dir.path().join(".mindweaver").exists());
    let opened = WeaverWorkspace::open(dir.path()).unwrap();
    assert_eq!(opened.names(), Names::Mindflayer);
    assert_eq!(opened, workspace);
    assert_eq!(
        workspace.ledger_path(),
        dir.path().join(".mindflayer/mindflayer.db")
    );
}

#[test]
fn a_shelf_mother_gathers_every_skill_it_is_given() {
    let (_dir, _workspace, ledger) = ShelfMother::gathered(&["deploy", "review"]);

    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.name.into_string())
        .collect();
    assert_eq!(names, ["deploy", "review"]);
}

#[test]
fn a_mindflayer_mother_leaves_a_projects_blocks_under_the_old_markers() {
    let dir = tempfile::TempDir::new().unwrap();
    let new = format!(
        "# Team\n\n{}\n{}a -->\nRule a.\n{}\n",
        crate::agents_md::BEGIN,
        crate::agents_md::RULE,
        crate::agents_md::END
    );
    fs::write(dir.path().join("AGENTS.md"), &new).unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
        format!(
            "{}\n@AGENTS.md\n{}\n",
            crate::agents_md::BEGIN,
            crate::agents_md::END
        ),
    )
    .unwrap();

    MindflayerMother::rendered_into(dir.path());

    assert_eq!(
        fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        "# Team\n\n<!-- mindflayer:begin -->\n<!-- mindflayer:rule a -->\nRule a.\n<!-- mindflayer:end -->\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
        "<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n"
    );
}

#[test]
fn a_mindflayer_mother_leaves_a_projects_exclude_block_under_the_old_markers() {
    let dir = tempfile::TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    crate::exclude::add(
        dir.path(),
        "app",
        &[crate::exclude::Entry::file("AGENTS.md")],
    )
    .unwrap();

    MindflayerMother::rendered_into(dir.path());

    let text = fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert!(
        text.ends_with("# mindflayer:begin\n# project app\n/AGENTS.md\n# mindflayer:end\n"),
        "{text}"
    );
}
