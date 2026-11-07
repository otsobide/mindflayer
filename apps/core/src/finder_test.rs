//! Unit tests for `finder.rs`: which finders a workspace enables, and which
//! a search asks. A search through a fake transport is
//! `tests/finder_test.rs`'s.

use std::fs;

use super::*;
use crate::mother::{ProjectMother, WorkspaceMother};

/// A workspace whose marker lists `finders` after what `init` wrote, the
/// way somebody would add the key by hand.
fn listing(finders: &str) -> (tempfile::TempDir, WeaverWorkspace) {
    let (dir, workspace) = WorkspaceMother::empty();
    let marker = workspace.config_path();
    let mut text = fs::read_to_string(&marker).unwrap();
    text = text.replacen(
        "loaded = []\n",
        &format!("loaded = []\n# Read their terms first.\nfinders = {finders}\n"),
        1,
    );
    fs::write(&marker, text).unwrap();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    (dir, workspace)
}

#[test]
fn a_finder_is_named_by_its_service() {
    for finder in Finder::ALL {
        assert_eq!(Finder::named(finder.name()), Some(finder));
        assert_eq!(finder.to_string(), finder.name());
        assert_eq!(
            serde_json::to_string(&finder).unwrap(),
            format!("\"{}\"", finder.name())
        );
    }
    let names: Vec<&str> = Finder::ALL.iter().map(|finder| finder.name()).collect();
    assert_eq!(names, ["github", "skills.sh", "claude-plugins.dev"]);
    assert_eq!(Finder::named("GitHub"), None);
}

/// GitHub's code search is documented and wants a token; the other two
/// answer anybody, and are read as they answer today.
#[test]
fn github_needs_a_token_and_the_other_two_are_unstable() {
    let unstable: Vec<Finder> = Finder::ALL
        .into_iter()
        .filter(|finder| finder.is_unstable())
        .collect();
    let tokened: Vec<Finder> = Finder::ALL
        .into_iter()
        .filter(|finder| finder.needs_token())
        .collect();

    assert_eq!(unstable, [Finder::SkillsSh, Finder::ClaudePlugins]);
    assert_eq!(tokened, [Finder::Github]);
}

#[test]
fn a_workspace_enables_no_finder_until_its_marker_lists_one() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let enabled = enabled(&workspace);

    assert!(enabled.finders.is_empty());
    assert!(enabled.unknown.is_empty());
    assert_eq!(
        enabled.asked(&[]),
        Err(SearchError::NoneEnabled {
            config: workspace.config_path()
        })
    );
    assert_eq!(
        enabled.asked(&[]).unwrap_err().to_string(),
        format!(
            "no finder is enabled, so nothing was searched: list one in `finders` in {} (github, \
             skills.sh, claude-plugins.dev; the last two are unstable and undocumented, so read \
             their terms first)",
            workspace.config_path().display()
        )
    );
}

/// In core's order whatever the marker's, each once; a name this build
/// does not know is said and passed over, never refused.
#[test]
fn the_finders_listed_are_enabled_and_an_unknown_name_is_passed_over() {
    let (_dir, workspace) = listing(r#"["skills.sh", "skillsmp", "github", "skills.sh", "x\ny"]"#);

    let enabled = enabled(&workspace);

    assert_eq!(enabled.finders, [Finder::Github, Finder::SkillsSh]);
    let unknown: Vec<&str> = enabled
        .unknown
        .iter()
        .map(|unknown| unknown.name.as_str())
        .collect();
    assert_eq!(unknown, ["skillsmp", "x y"]);
    assert_eq!(enabled.unknown[0].code(), "finder_unknown");
    assert_eq!(
        enabled.unknown[0].to_string(),
        format!(
            "{}: `skillsmp` is among its finders, and this build knows none by that name, so it \
             is passed over",
            workspace.config_path().display()
        )
    );
}

/// None named asks every one enabled; those named are asked alone, and
/// one named that is not enabled is refused before anything is asked.
#[test]
fn a_search_asks_the_finders_named_only_when_each_is_enabled() {
    let (_dir, workspace) = listing(r#"["claude-plugins.dev", "github"]"#);
    let enabled = enabled(&workspace);

    assert_eq!(
        enabled.asked(&[]).unwrap(),
        [Finder::Github, Finder::ClaudePlugins]
    );
    assert_eq!(
        enabled
            .asked(&[Finder::ClaudePlugins, Finder::Github])
            .unwrap(),
        [Finder::Github, Finder::ClaudePlugins]
    );
    assert_eq!(
        enabled.asked(&[Finder::ClaudePlugins]).unwrap(),
        [Finder::ClaudePlugins]
    );
    let refused = enabled
        .asked(&[Finder::Github, Finder::SkillsSh])
        .unwrap_err();
    assert_eq!(refused.code(), "finder_not_enabled");
    assert_eq!(
        refused.to_string(),
        format!(
            "skills.sh is not enabled, so it was not asked: list it in `finders` in {} to search \
             it",
            workspace.config_path().display()
        )
    );
}

/// The key is somebody's, with the comment they wrote above it: an edit of
/// the marker through `toml_edit` keeps both.
#[test]
fn an_edit_of_the_marker_keeps_the_finders_and_their_comment() {
    let (dir, mut workspace) = listing(r#"["github"]"#);
    let project = ProjectMother::random(dir.path());

    workspace.link(&project).unwrap();

    let text = fs::read_to_string(workspace.config_path()).unwrap();
    assert!(
        text.contains("# Read their terms first.\nfinders = [\"github\"]\n"),
        "{text}"
    );
    assert_eq!(enabled(&workspace).finders, [Finder::Github]);
}

/// What a search found serializes to, name by name: what `--json` prints
/// in the envelope, each failure as its code and its message.
#[test]
fn what_a_search_found_serializes_name_by_name() {
    let found = Found {
        hits: vec![Hit {
            name: String::from("pdf"),
            repository: Redacted::new("https://github.com/acme/skills"),
            path: Some(String::from("skills/pdf")),
            reference: Some(String::from("main")),
            installs: Some(12),
            stars: None,
            finders: vec![Finder::Github, Finder::ClaudePlugins],
        }],
        skipped: vec![Finder::Github],
        failures: vec![Failure::Unreadable {
            finder: Finder::SkillsSh,
            detail: Redacted::new("missing field `skills`"),
        }],
        warnings: Vec::new(),
    };

    assert_eq!(
        serde_json::to_value(&found).unwrap(),
        serde_json::json!({
            "hits": [{
                "name": "pdf",
                "repository": "https://github.com/acme/skills",
                "path": "skills/pdf",
                "reference": "main",
                "installs": 12,
                "stars": null,
                "finders": ["github", "claude-plugins.dev"],
            }],
            "skipped": ["github"],
            "failures": [{
                "code": "finder_answer_unreadable",
                "message": "skills.sh: its answer is not in the shape this build reads, so \
                            nothing it found is listed: missing field `skills`",
            }],
            "warnings": [],
        })
    );
}

/// An empty token is none, and a token is never shown.
#[test]
fn a_token_is_never_shown_and_an_empty_one_is_none() {
    assert_eq!(Token::new(""), None);
    let token = Token::new("t0ps3cret").unwrap();
    assert_eq!(format!("{token:?}"), "Token(<credential>)");
}

/// What `gather git` would take of a result: a repository elsewhere, or
/// here from a finder here; never one with a credential, nor a folder that
/// climbs out of it.
#[test]
fn a_result_resolves_only_to_a_git_source_a_gather_would_take() {
    let listed = |address: &str, path: Option<&str>| Listed {
        name: Some(String::from("pdf")),
        address: Some(address.to_owned()),
        path: path.map(str::to_owned),
        ..Listed::default()
    };
    let why = |listed: Listed, local: bool| listed.resolved(local).unwrap_err().1;

    let hit = listed("Acme/Skills", Some("/skills/pdf/"))
        .resolved(false)
        .unwrap();
    assert_eq!(hit.repository, "https://github.com/acme/skills");
    assert_eq!(hit.path.as_deref(), Some("skills/pdf"));
    let hit = listed("git@gitlab.com:acme/skills.git", Some("."))
        .resolved(false)
        .unwrap();
    assert_eq!(hit.repository, "https://gitlab.com/acme/skills");
    assert_eq!(hit.path, None);
    assert_eq!(
        listed("/srv/skills", None)
            .resolved(true)
            .unwrap()
            .repository,
        "/srv/skills"
    );

    assert_eq!(
        why(listed("/srv/skills", None), false),
        "it names a folder on this machine, which only a finder on this machine may"
    );
    assert_eq!(
        why(listed("./skills", None), false),
        "it names a folder on this machine, which only a finder on this machine may"
    );
    assert_eq!(
        why(
            listed("https://me:secret@github.com/acme/skills", None),
            false
        ),
        "its repository's address carries a credential"
    );
    assert_eq!(
        why(listed("acme/skills", Some("../elsewhere")), false),
        "its folder climbs out of the repository"
    );
    assert_eq!(
        why(listed("acme/skills\n", None), false),
        "its repository has a control character or a space at either end"
    );
    assert_eq!(
        why(
            Listed {
                address: None,
                ..listed("", None)
            },
            false
        ),
        "it names no repository"
    );
}
