//! Unit tests for `gather.rs`, kept beside it rather than inside it.
//!
//! The integration suite gathers from repositories; these hold the pieces
//! around a gather to their rules: a request as its builders leave it, a
//! source that shows no credential however it was typed, what a scrub
//! says once however often it was run, which folders are candidates, how a
//! path is filed in the ledger, and the codes and words of each failure.

use super::*;

use std::sync::Mutex;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{RevisionMother, SkillMother, SourceMother, UrlMother, WorkspaceMother};

const DRAWS: usize = 200;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A remote that is never to be reached, for a request that must be
/// refused before anything is fetched.
#[derive(Debug, Default)]
struct Unreached {
    asked: Mutex<usize>,
}

impl Remote for Unreached {
    fn fetch(
        &self,
        _url: &str,
        _reference: Option<&str>,
        _pin: Option<&Revision>,
        _into: &Path,
        _watch: &Watch,
    ) -> Result<crate::port::Cloned, GitError> {
        *self.asked.lock().unwrap() += 1;
        Err(GitError::Cancelled)
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        *self.asked.lock().unwrap() += 1;
        Ok(false)
    }
}

/// A remote that fetches by writing two skills, `deploy` holding a link
/// to `to`, as a repository can hold one.
#[cfg(unix)]
#[derive(Debug)]
struct Linking {
    to: PathBuf,
}

#[cfg(unix)]
impl Remote for Linking {
    fn fetch(
        &self,
        _url: &str,
        _reference: Option<&str>,
        _pin: Option<&Revision>,
        into: &Path,
        _watch: &Watch,
    ) -> Result<crate::port::Cloned, GitError> {
        let _ = fs::remove_dir_all(into);
        for name in ["deploy", "review"] {
            write(
                &into.join(format!("skills/{name}/SKILL.md")),
                &SkillMother::create(name, "About it"),
            );
        }
        std::os::unix::fs::symlink(&self.to, into.join("skills/deploy/note")).unwrap();
        Ok(crate::port::Cloned {
            root: into.to_path_buf(),
            revision: Some(Revision::from("0123abc")),
        })
    }

    fn scrub(&self, _clone: &Path) -> Result<bool, GitError> {
        Ok(false)
    }
}

fn scrub_failure(url: &str) -> ScrubFailure {
    ScrubFailure {
        url: Redacted::new(url),
        source: GitError::Cancelled,
    }
}

fn leftover(path: &str) -> Leftover {
    Leftover {
        path: PathBuf::from(path),
        source: io::Error::other("busy"),
    }
}

#[test]
fn a_request_for_a_git_source_searches_the_whole_repository_at_its_head() {
    let url = UrlMother::clean();
    let request = Request::git(url.clone());

    assert_eq!(
        request.source,
        Source::Git {
            url: url.clone(),
            reference: None,
        }
    );
    assert_eq!(request.subdirectory, DEFAULT_SUBDIRECTORY);
    assert_eq!(
        DEFAULT_SUBDIRECTORY, "",
        "no folder: the repository from its root"
    );
    assert_eq!(request.kind, Kind::Skill);
    assert_eq!(
        request.search,
        Search::Layouts,
        "every layout, as a repository's"
    );
    assert!(!request.watch.is_cancelled());
}

#[test]
fn a_request_for_an_address_takes_the_repository_branch_and_folder_it_names() {
    let tree = "https://github.com/acme/skills/tree/main/plugins/ops";
    for (typed, reference, folder, url, at, searched) in [
        (
            "acme/skills",
            None,
            None,
            "acme/skills",
            None,
            DEFAULT_SUBDIRECTORY,
        ),
        ("acme/skills@v2", None, None, "acme/skills", Some("v2"), ""),
        (
            "acme/skills@v2",
            Some("v2"),
            None,
            "acme/skills",
            Some("v2"),
            "",
        ),
        (
            "acme/skills",
            Some("v2"),
            Some("agents"),
            "acme/skills",
            Some("v2"),
            "agents",
        ),
        (
            tree,
            None,
            None,
            "https://github.com/acme/skills",
            Some("main"),
            "plugins/ops",
        ),
        (
            tree,
            Some("main"),
            Some("plugins/ops"),
            "https://github.com/acme/skills",
            Some("main"),
            "plugins/ops",
        ),
        (
            "https://github.com/acme/skills/tree/main",
            None,
            Some("skills"),
            "https://github.com/acme/skills",
            Some("main"),
            "skills",
        ),
        // As a browser hands them over, a query after them and an escape in
        // a folder's name.
        (
            "https://gitlab.com/acme/skills/-/tree/main/skills?ref_type=heads",
            Some("main"),
            Some("skills"),
            "https://gitlab.com/acme/skills",
            Some("main"),
            "skills",
        ),
        (
            "https://github.com/acme/skills/tree/main/skills/caf%C3%A9",
            None,
            Some("skills/café"),
            "https://github.com/acme/skills",
            Some("main"),
            "skills/café",
        ),
    ] {
        let request = Request::addressed(
            typed,
            reference.map(str::to_owned),
            folder.map(str::to_owned),
        )
        .unwrap();

        assert_eq!(request.source.url(), url, "{typed}");
        assert_eq!(request.source.reference(), at, "{typed}");
        assert_eq!(request.subdirectory, searched, "{typed}");
    }
}

#[test]
fn an_address_naming_another_branch_or_folder_than_its_option_is_refused() {
    let tree = "https://github.com/acme/skills/tree/main/skills";

    let branch = Request::addressed("acme/skills@v2", Some(String::from("v3")), None).unwrap_err();
    let tree_branch = Request::addressed(tree, Some(String::from("dev")), None).unwrap_err();
    let folder = Request::addressed(tree, None, Some(String::from("agents"))).unwrap_err();

    assert_eq!(branch.code(), "gather_ref_contradicted");
    assert_eq!(
        branch.to_string(),
        "acme/skills@v2 names the branch or tag `v2`, and --ref names `v3`: give one"
    );
    assert_eq!(tree_branch.code(), "gather_ref_contradicted");
    assert_eq!(folder.code(), "gather_path_contradicted");
    assert_eq!(
        folder.to_string(),
        "https://github.com/acme/skills/tree/main/skills names the folder `skills`, and --path \
         names `agents`: give one"
    );
}

#[test]
fn a_tree_url_whose_escapes_spell_no_name_is_refused_naming_where() {
    let typed = "https://github.com/acme/skills/tree/main/skills/a%2Fb";

    let refused = Request::addressed(typed, None, None).unwrap_err();

    assert_eq!(refused.code(), "gather_tree_undecodable");
    assert_eq!(
        refused.to_string(),
        "https://github.com/acme/skills/tree/main/skills/a%2Fb: `a%2Fb` names no branch or \
         folder once its escapes are read: give the repository's URL, with the branch after \
         --ref and the folder after --path"
    );
}

#[test]
fn a_contradiction_is_said_without_the_credentials_either_side_carries() {
    let secret = UrlMother::secret();
    let typed = format!("https://{secret}@github.com/acme/private/tree/main/skills");
    let given = format!("https://me:{secret}@example.com/x");

    for refused in [
        Request::addressed(&typed, Some(given.clone()), None).unwrap_err(),
        Request::addressed(&typed, None, Some(given.clone())).unwrap_err(),
    ] {
        let said = refused.to_string();
        assert!(!said.contains(&secret), "{said}");
    }
}

#[test]
fn a_folder_outside_the_repository_is_refused_before_the_remote_is_asked() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().unwrap();
    let remote = Arc::new(Unreached::default());

    for folder in ["..", "../..", "skills/../../x", "/srv/skills"] {
        let request = Request::git(UrlMother::https())
            .from_subdirectory(folder)
            .through(remote.clone());

        let refused = gather(&workspace, &ledger, &request).unwrap_err();
        let previewed = preview(&workspace, &ledger, &request).unwrap_err();

        assert_eq!(refused.code(), "gather_path_outside", "{folder}");
        assert_eq!(previewed.code(), "gather_path_outside", "{folder}");
    }
    let tree = Request::addressed("https://github.com/acme/skills/tree/main/../..", None, None)
        .unwrap()
        .through(remote.clone());
    let refused = gather(&workspace, &ledger, &tree).unwrap_err();

    assert_eq!(
        refused.to_string(),
        "`../..` is no folder of the repository: --path names one from its root, and never \
         climbs out of it"
    );
    assert_eq!(*remote.asked.lock().unwrap(), 0);
    assert_eq!(ledger.sources().unwrap(), []);
}

#[test]
fn a_folder_inside_the_repository_however_it_is_spelled_is_no_folder_outside_it() {
    for folder in [
        "",
        ".",
        "skills",
        "./skills/",
        "skills/..",
        "a/../b",
        "..skills",
    ] {
        assert!(!outside(folder), "{folder}");
    }
}

#[test]
fn a_credential_a_tree_url_carries_is_refused_before_the_remote_is_asked() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().unwrap();
    let remote = Arc::new(Unreached::default());
    let typed = format!(
        "https://{}@github.com/acme/private/tree/main/skills",
        UrlMother::secret()
    );

    let request = Request::addressed(&typed, None, None)
        .unwrap()
        .through(remote.clone());
    let refused = gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(&refused, GatherError::CredentialsInUrl { url } if url == "https://github.com/acme/private"),
        "{refused:?}"
    );
    assert_eq!(*remote.asked.lock().unwrap(), 0);
}

#[test]
fn a_request_takes_the_folder_and_the_branch_it_is_told() {
    for _ in 0..DRAWS {
        let (folder, branch) = (random::word(), random::word());

        let request = Request::git(UrlMother::https())
            .from_subdirectory(folder.clone())
            .at(Some(branch.clone()));

        assert_eq!(request.subdirectory, folder);
        assert_eq!(request.source.reference(), Some(branch.as_str()));
    }
}

#[test]
fn a_request_is_pinned_to_what_it_is_told_and_to_nothing_again_by_none() {
    let request =
        Request::git(UrlMother::https()).pinned(Some("4F9C2A7E1B3D5F60A8C4E2B6D0F1A3C5E7B9D1F3"));
    assert_eq!(
        request.pin.as_deref(),
        Some("4F9C2A7E1B3D5F60A8C4E2B6D0F1A3C5E7B9D1F3")
    );
    assert_eq!(Request::git(UrlMother::https()).pin, None);
    assert_eq!(request.pinned(None::<String>).pin, None);
}

#[test]
fn a_source_is_gathered_again_from_where_and_as_it_was_gathered_last() {
    for _ in 0..DRAWS {
        let mut source = SourceMother::random();
        source.subdirectory = random::word();
        source.pin = random::chance().then(RevisionMother::random);

        let request = Request::again(&source);

        assert_eq!(request.source.url(), source.url.as_str(), "{source:?}");
        assert_eq!(request.source.reference(), source.reference.as_deref());
        assert_eq!(request.subdirectory, source.subdirectory);
        assert_eq!(request.kind, Kind::Skill);
        assert_eq!(
            request.pin.as_deref(),
            source.pin.as_ref().map(|pin| pin.as_str())
        );
        // Under the name it has, which a request naming none leaves.
        assert_eq!(request.name, None);
    }
}

#[test]
fn a_pin_is_a_commits_full_id_in_lowercase_and_nothing_else() {
    for _ in 0..DRAWS {
        let digits: String = (0..40)
            .map(|_| random::pick(&b"0123456789abcdefABCDEF".map(char::from)))
            .collect();

        assert_eq!(
            pin(&digits).as_deref(),
            Some(digits.to_ascii_lowercase().as_str())
        );
    }
    let full = "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3";
    for refused in [
        "4f9c2a7",
        "main",
        "v1.0",
        "",
        &format!("{full}0"),
        &format!(" {}", &full[1..]),
        &full.replace('f', "g"),
        &"a".repeat(64),
    ] {
        assert_eq!(pin(refused), None, "{refused:?}");
    }
}

#[test]
fn a_pin_that_is_no_commits_id_is_refused_before_anything_is_registered_or_fetched() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    let remote = Arc::new(Unreached::default());
    let request = Request::git(UrlMother::https())
        .pinned(Some("main"))
        .through(remote.clone());

    let refused = gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(&refused, GatherError::PinInvalid { pin } if pin == "main"),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "`main` cannot pin a gather: it has to be a commit's full id, 40 hexadecimal digits (a \
         branch or a tag goes after --ref)"
    );
    assert_eq!(*remote.asked.lock().unwrap(), 0);
    assert!(ledger.sources().unwrap().is_empty());
}

/// `at(None)` is the remote's HEAD again, not the branch asked for before.
#[test]
fn a_request_at_no_branch_goes_back_to_the_remote_s_head() {
    let request = Request::git(UrlMother::https())
        .at(Some("main"))
        .at(None::<String>);

    assert_eq!(request.source.reference(), None);
}

/// A front end keeps a clone of the watch to cancel from wherever its keys
/// are read: the request's is that one, not a copy of its own.
#[test]
fn a_request_watched_by_a_watch_is_cancelled_by_cancelling_it() {
    let watch = Watch::new();
    let request = Request::git(UrlMother::https()).watched_by(watch.clone());

    watch.cancel();

    assert!(request.watch.is_cancelled());
}

#[test]
fn a_request_fetches_through_the_remote_it_is_handed() {
    let remote = Arc::new(Unreached::default());
    let request = Request::git(UrlMother::https()).through(remote.clone());

    let _ = request
        .remote
        .fetch("u", None, None, Path::new("/nowhere"), &request.watch);

    assert_eq!(*remote.asked.lock().unwrap(), 1);
}

#[test]
fn a_source_says_its_kind_url_and_branch_as_it_was_given() {
    for _ in 0..DRAWS {
        let typed = random::pick(&[UrlMother::clean(), UrlMother::credentialed().typed]);
        let branch = random::chance().then(random::word);
        let request = Request::git(typed.clone()).at(branch.clone());

        assert_eq!(request.source.kind(), SourceKind::Git);
        assert_eq!(request.source.url(), typed, "the URL as typed");
        assert_eq!(request.source.reference(), branch.as_deref());
    }
}

/// A request is built before a URL with a credential is refused, so what it
/// serializes to is held to having none whatever was typed.
#[test]
fn a_source_typed_with_any_credential_serializes_without_it() {
    for typed in UrlMother::every_credentialed() {
        let request = Request::git(typed.typed.clone()).at(Some("main"));

        let serialized = serde_json::to_string(&request.source).unwrap();

        assert!(!serialized.contains(&typed.secret), "{serialized}");
        assert_eq!(
            serialized,
            format!(
                r#"{{"kind":"git","url":{},"reference":"main"}}"#,
                serde_json::to_string(&typed.clean).unwrap()
            )
        );
    }
}

#[test]
fn a_source_at_the_remote_s_head_serializes_a_null_branch() {
    let url = UrlMother::https();

    assert_eq!(
        serde_json::to_string(&Request::git(url.clone()).source).unwrap(),
        format!(r#"{{"kind":"git","url":"{url}","reference":null}}"#)
    );
}

/// Refused before anything is registered or fetched, and with the URL said
/// without what it carried, which is the URL to gather instead.
#[test]
fn a_gather_from_any_credentialed_url_is_refused_before_the_remote_is_asked() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    for typed in UrlMother::every_credentialed() {
        let remote = Arc::new(Unreached::default());
        let request = Request::git(typed.typed.clone()).through(remote.clone());

        let refused = gather(&workspace, &ledger, &request).unwrap_err();

        assert!(
            matches!(&refused, GatherError::CredentialsInUrl { url } if url == &typed.clean),
            "{typed:?} gave {refused:?}"
        );
        assert!(!refused.to_string().contains(&typed.secret), "{refused}");
        assert_eq!(*remote.asked.lock().unwrap(), 0, "{typed:?}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    for entry in ledger.history(100).unwrap() {
        assert_eq!(entry.outcome, "failed");
        let said = serde_json::to_string(&entry).unwrap();
        for typed in UrlMother::every_credentialed() {
            assert!(!said.contains(&typed.secret), "{said}");
        }
    }
}

/// A URL typed where the branch or the folder goes would be kept as that
/// branch or folder, word for word, so it is refused as one typed where the
/// URL goes is, before anything is registered; and one typed as the pin,
/// which an error would repeat, is refused the same way rather than as no
/// commit's id.
#[test]
fn a_credentialed_url_typed_as_the_branch_the_folder_the_name_or_the_pin_is_refused_alike() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    for typed in UrlMother::every_credentialed() {
        let remote = Arc::new(Unreached::default());
        let request = match random::between(0, 3) {
            0 => Request::git(UrlMother::https()).at(Some(typed.typed.as_str())),
            1 => Request::git(UrlMother::https()).from_subdirectory(typed.typed.clone()),
            2 => Request::git(UrlMother::https()).named(Some(typed.typed.as_str())),
            _ => Request::git(UrlMother::https()).pinned(Some(typed.typed.as_str())),
        }
        .through(remote.clone());

        let refused = gather(&workspace, &ledger, &request).unwrap_err();

        assert!(
            matches!(&refused, GatherError::CredentialsInUrl { url } if url == &typed.clean),
            "{typed:?} gave {refused:?}"
        );
        assert_eq!(*remote.asked.lock().unwrap(), 0, "{typed:?}");
    }
    assert!(ledger.sources().unwrap().is_empty());
    let history = serde_json::to_string(&ledger.history(100).unwrap()).unwrap();
    for typed in UrlMother::every_credentialed() {
        assert!(!history.contains(&typed.secret), "{history}");
    }
}

#[test]
fn a_gather_of_rules_is_refused_and_fetches_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    let remote = Arc::new(Unreached::default());
    let mut request = Request::git(UrlMother::https()).through(remote.clone());
    request.kind = Kind::Rule;

    let refused = gather(&workspace, &ledger, &request).unwrap_err();

    assert!(
        matches!(refused, GatherError::NotGatherable { kind: Kind::Rule }),
        "{refused:?}"
    );
    assert_eq!(refused.to_string(), "rules cannot be gathered yet");
    assert_eq!(*remote.asked.lock().unwrap(), 0);
    assert!(ledger.sources().unwrap().is_empty());
}

#[test]
fn a_report_counts_everything_now_on_the_shelf_because_of_it() {
    let harvested = || Harvested {
        name: ArtifactName::from(random::kebab()),
        directory: random::word(),
        summary: None,
        path: PathBuf::from("/w"),
    };
    for _ in 0..50 {
        let (added, updated, unchanged, removed) = (
            random::between(0, 4),
            random::between(0, 4),
            random::between(0, 4),
            random::between(0, 4),
        );
        let report = Report {
            name: SourceMother::random().name,
            revision: None,
            directory: random::word(),
            added: (0..added).map(|_| harvested()).collect(),
            updated: (0..updated).map(|_| harvested()).collect(),
            unchanged: (0..unchanged).map(|_| harvested()).collect(),
            failures: vec![Failure::UnusableName {
                path: PathBuf::from("/w/x"),
            }],
            warnings: Vec::new(),
            // Off the shelf, so counted nowhere.
            removed: (0..removed).map(|_| harvested()).collect(),
        };

        assert_eq!(report.total(), added + updated + unchanged);
        assert_eq!(report.is_empty(), added + updated + unchanged == 0);
    }
}

#[test]
fn a_scrub_that_took_nothing_out_is_empty() {
    assert!(Scrub::default().is_empty());
}

#[test]
fn a_scrub_that_took_anything_out_is_not_empty() {
    let nonempty = [
        Scrub {
            sources: vec![SourceMother::random()],
            ..Scrub::default()
        },
        Scrub {
            actions: 1,
            ..Scrub::default()
        },
        Scrub {
            failures: vec![scrub_failure("https://example.com/a.git")],
            ..Scrub::default()
        },
        Scrub {
            leftovers: vec![leftover("/w/.mindweaver/cache/a")],
            ..Scrub::default()
        },
        Scrub {
            merged: vec![Merged {
                url: crate::value::Redacted::new(&UrlMother::https()),
                reference: None,
                directory: String::from("a"),
                into: SourceMother::random(),
            }],
            ..Scrub::default()
        },
    ];
    for scrub in nonempty {
        assert!(!scrub.is_empty(), "{scrub:?}");
    }
}

/// Two branches of one URL are one credential to tell of, and a clone that
/// failed to be rewritten fails again at every scrub: each is said once.
#[test]
fn a_later_scrub_absorbed_tells_each_url_once_and_counts_every_entry() {
    let url = UrlMother::https();
    let other = UrlMother::https();
    let mut first = Scrub {
        sources: vec![SourceMother::create(&url, Some("main"))],
        actions: 2,
        failures: vec![scrub_failure(&url)],
        leftovers: vec![leftover("/w/a")],
        merged: Vec::new(),
    };
    let later = Scrub {
        sources: vec![
            SourceMother::create(&url, Some("dev")),
            SourceMother::create(&other, None),
        ],
        actions: 3,
        failures: vec![scrub_failure(&url), scrub_failure(&other)],
        leftovers: vec![leftover("/w/a"), leftover("/w/b")],
        merged: Vec::new(),
    };

    first.absorb(later);

    let urls: Vec<&str> = first
        .sources
        .iter()
        .map(|source| source.url.as_str())
        .collect();
    assert_eq!(urls, [url.as_str(), other.as_str()]);
    assert_eq!(
        first.sources[0].reference.as_deref(),
        Some("main"),
        "the first kept"
    );
    assert_eq!(first.actions, 5);
    let failed: Vec<&str> = first
        .failures
        .iter()
        .map(|failure| failure.url.as_str())
        .collect();
    assert_eq!(failed, [url.as_str(), other.as_str()]);
    assert_eq!(first.leftovers.len(), 3, "a folder left is said each time");
}

#[test]
fn absorbing_an_empty_scrub_changes_nothing() {
    let mut scrub = Scrub {
        sources: vec![SourceMother::random()],
        actions: random::between(0, 9),
        ..Scrub::default()
    };
    let (sources, actions) = (scrub.sources.clone(), scrub.actions);

    scrub.absorb(Scrub::default());

    assert_eq!(scrub.sources, sources);
    assert_eq!(scrub.actions, actions);
}

#[test]
fn a_scrub_of_a_ledger_with_no_credential_asks_the_remote_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = Ledger::in_memory().unwrap();
    let remote = Unreached::default();

    let scrubbed = scrub(&workspace, &ledger, Duration::ZERO, &remote).unwrap();

    assert!(scrubbed.is_empty(), "{scrubbed:?}");
    assert_eq!(*remote.asked.lock().unwrap(), 0);
}

/// The candidates `root` holds, searched as a gather of a clone at `root`
/// searches it, with what keeps any from being taken.
fn found_in(root: &Path) -> (Vec<PathBuf>, Vec<Failure>) {
    let mut failures = Vec::new();
    let found = candidates(
        root,
        root,
        &Request::git("unused"),
        None,
        "SKILL.md",
        &mut failures,
    )
    .unwrap();
    (found, failures)
}

#[test]
fn candidates_are_the_folders_holding_a_manifest_in_order() {
    let dir = TempDir::new().unwrap();
    let mut names: Vec<String> = (0..random::between(1, 6))
        .map(|_| random::kebab())
        .filter(|name| !matches!(name.as_str(), "assets" | "deep"))
        .collect();
    for name in &names {
        write(
            &dir.path().join(name).join("SKILL.md"),
            &SkillMother::create(name, "x"),
        );
    }
    fs::create_dir_all(dir.path().join("assets")).unwrap();
    write(&dir.path().join("README.md"), "not a skill");
    write(&dir.path().join("deep").join("inner").join("SKILL.md"), "x");
    names.push(String::from("deep/inner"));
    names.sort();
    names.dedup();

    let (found, failures) = found_in(dir.path());

    let expected: Vec<PathBuf> = names.iter().map(|name| dir.path().join(name)).collect();
    assert_eq!(found, expected, "{names:?}");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn candidates_are_found_as_far_down_as_the_search_goes_and_no_further() {
    let dir = TempDir::new().unwrap();
    let mut route = PathBuf::new();
    let mut expected = Vec::new();
    for depth in 1..=SEARCH_DEPTH + 1 {
        // A skill at each depth, beside the folder the search goes on into.
        route.push(format!("level-{depth}"));
        let skill = route.with_file_name(format!("skill-{depth}"));
        write(&dir.path().join(&skill).join("SKILL.md"), "x");
        if depth <= SEARCH_DEPTH {
            expected.push(dir.path().join(skill));
        }
        fs::create_dir_all(dir.path().join(&route)).unwrap();
    }
    expected.sort();

    let (found, _) = found_in(dir.path());

    assert_eq!(found, expected);
}

#[test]
fn a_folder_holding_a_manifest_is_not_looked_into() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("skills/teach/SKILL.md"), "x");
    write(&dir.path().join("skills/teach/examples/demo/SKILL.md"), "x");

    let (found, _) = found_in(dir.path());

    assert_eq!(found, [dir.path().join("skills/teach")]);
}

#[test]
fn git_s_own_folder_and_a_package_manager_s_are_passed_over_wherever_they_are() {
    let dir = TempDir::new().unwrap();
    for passed in PASSED_OVER {
        write(&dir.path().join(passed).join("held/SKILL.md"), "x");
        write(
            &dir.path().join("tools").join(passed).join("x/SKILL.md"),
            "x",
        );
    }
    write(&dir.path().join("skills/deploy/SKILL.md"), "x");

    let (found, _) = found_in(dir.path());

    assert_eq!(found, [dir.path().join("skills/deploy")]);
}

#[cfg(unix)]
#[test]
fn a_linked_folder_is_not_searched_and_a_skill_linked_twice_is_found_where_it_is() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    write(&dir.path().join("skills/deploy/SKILL.md"), "x");
    write(&dir.path().join("vendor/review/SKILL.md"), "x");
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::create_dir_all(dir.path().join(".agents/skills")).unwrap();
    // A whole folder linked: what it holds is found where it is, once.
    symlink("../skills", dir.path().join(".claude/skills")).unwrap();
    // One skill linked: taken where the link leads, which is found there.
    symlink(
        "../../skills/deploy",
        dir.path().join(".agents/skills/deploy"),
    )
    .unwrap();
    // A skill kept only as a link, to a folder too deep to be found.
    write(&dir.path().join("a/b/c/d/e/audit/SKILL.md"), "x");
    symlink("a/b/c/d/e/audit", dir.path().join("audit")).unwrap();

    let (found, failures) = found_in(dir.path());

    assert_eq!(
        found,
        [
            dir.path().join("audit"),
            dir.path().join("skills/deploy"),
            dir.path().join("vendor/review"),
        ]
    );
    assert!(failures.is_empty(), "{failures:?}");
}

#[cfg(unix)]
#[test]
fn a_skill_s_folder_linking_out_of_the_repository_is_refused_and_said() {
    use std::os::unix::fs::symlink;

    let outside = TempDir::new().unwrap();
    write(&outside.path().join("SKILL.md"), "x");
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("skills/deploy/SKILL.md"), "x");
    symlink(outside.path(), dir.path().join("skills/keys")).unwrap();

    let (found, failures) = found_in(dir.path());

    assert_eq!(found, [dir.path().join("skills/deploy")]);
    assert!(
        matches!(
            &failures[..],
            [Failure::LinkOutside { path, to }]
                if *path == dir.path().join("skills/keys")
                    && *to == fs::canonicalize(outside.path()).unwrap()
        ),
        "{failures:?}"
    );
    assert_eq!(failures[0].code(), "gathered_link_outside");
}

#[cfg(unix)]
#[test]
fn a_skill_a_plugin_names_through_a_link_out_of_the_repository_is_refused_and_said() {
    use std::os::unix::fs::symlink;

    let outside = TempDir::new().unwrap();
    write(&outside.path().join("SKILL.md"), "x");
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("skills/xlsx/SKILL.md"), "x");
    symlink(outside.path(), dir.path().join("skills/keys")).unwrap();
    let request = Request::git("unused").searching(Search::Plugin {
        skills: Some(vec![
            String::from("./skills/xlsx"),
            String::from("./skills/keys"),
        ]),
        license: None,
        dialect: Dialect::Claude,
    });
    let mut failures = Vec::new();

    let found = candidates(
        dir.path(),
        dir.path(),
        &request,
        None,
        "SKILL.md",
        &mut failures,
    )
    .unwrap();

    assert_eq!(found, [dir.path().join("skills/xlsx")]);
    assert!(
        matches!(
            &failures[..],
            [Failure::LinkOutside { path, to }]
                if *path == dir.path().join("skills/keys")
                    && *to == fs::canonicalize(outside.path()).unwrap()
        ),
        "{failures:?}"
    );
}

/// The folder was checked to be there just before: one that cannot be
/// listed is a gather that cannot happen, unlike discovery's absence.
#[test]
fn candidates_in_a_folder_that_is_not_there_cannot_be_read() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("skills");

    let refused = candidates(
        &missing,
        dir.path(),
        &Request::git("unused"),
        None,
        "SKILL.md",
        &mut Vec::new(),
    )
    .unwrap_err();

    assert!(
        matches!(&refused, GatherError::Read { path, .. } if path == &missing),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "gather_read_failed");
}

#[test]
fn a_path_in_the_workspace_is_filed_slash_separated_from_its_root() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let path = workspace
        .gathered_dir(Kind::Skill)
        .join("github.com-acme-skills")
        .join("deploy");

    assert_eq!(
        relative(&workspace, &path),
        format!(
            ".mindweaver/{}/github.com-acme-skills/deploy",
            Kind::Skill.folder()
        )
    );
}

#[test]
fn any_path_below_the_workspace_is_filed_by_its_route() {
    let (_dir, workspace) = WorkspaceMother::empty();
    for _ in 0..DRAWS {
        let segments: Vec<String> = (0..random::between(1, 4))
            .map(|_| random::kebab())
            .collect();
        let mut path = workspace.root().to_path_buf();
        for segment in &segments {
            path.push(segment);
        }

        assert_eq!(relative(&workspace, &path), segments.join("/"), "{path:?}");
    }
}

#[test]
fn a_gathered_folder_is_where_it_is_in_the_repository_from_its_root() {
    let clone = Path::new("/w/.mindweaver/cache/acme-skills");
    for _ in 0..DRAWS {
        let segments: Vec<String> = (0..random::between(1, 4))
            .map(|_| random::kebab())
            .collect();
        let folder = random::kebab();
        let mut directory = clone.to_path_buf();
        for segment in &segments {
            directory.push(segment);
        }
        // As a folder typed `./<route>/` leads there.
        let typed = clone.join(".").join(segments.join("/") + "/").join(&folder);

        let route = format!("{}/{folder}", segments.join("/"));
        assert_eq!(upstream(&directory.join(&folder), clone, &folder), route);
        assert_eq!(upstream(&typed, clone, &folder), route, "{typed:?}");
    }
    // A skill one folder down is its folder alone, and one that is the
    // whole repository, at its root, is the root, `.` as a route says so.
    assert_eq!(upstream(&clone.join("lint"), clone, "lint"), "lint");
    assert_eq!(upstream(clone, clone, "lint"), ".");
    assert_eq!(upstream(&clone.join("."), clone, "lint"), ".");
}

#[test]
fn a_licence_the_manifest_states_is_the_licence_whatever_a_file_beside_it_says() {
    let folder = TempDir::new().unwrap();
    write(&folder.path().join("LICENSE"), "MIT License\n");

    for stated in ["Apache-2.0", "Complete terms in LICENSE.txt", "  MIT  "] {
        assert_eq!(
            license_of(Some(stated), folder.path()).as_deref(),
            Some(stated.trim()),
            "{stated:?}"
        );
    }
}

#[test]
fn a_licence_file_names_the_licence_by_its_spdx_identifier_else_by_its_first_line() {
    for (text, named) in [
        ("MIT License\n\nCopyright (c) Acme\n", "MIT License"),
        ("\n\n   Apache License\n   Version 2.0\n", "Apache License"),
        (
            "Copyright Acme\nSPDX-License-Identifier: MIT OR Apache-2.0\n",
            "MIT OR Apache-2.0",
        ),
        (
            "<!-- SPDX-License-Identifier: GPL-3.0-only -->\n# The GPL\n",
            "GPL-3.0-only",
        ),
        (
            "/* SPDX-License-Identifier: BSD-3-Clause */\n",
            "BSD-3-Clause",
        ),
        // A tag naming nothing is no identifier, and the first line stands.
        ("ISC License\nSPDX-License-Identifier:  \n", "ISC License"),
        // A byte order mark opens the file, not its first line.
        ("\u{feff}MIT License\n", "MIT License"),
    ] {
        let folder = TempDir::new().unwrap();
        write(&folder.path().join("LICENSE"), text);

        assert_eq!(
            license_of(None, folder.path()).as_deref(),
            Some(named),
            "{text:?}"
        );
    }
}

#[test]
fn a_licence_file_is_found_by_either_spelling_in_any_case_and_a_stated_blank_says_nothing() {
    for name in LICENSE_FILES
        .iter()
        .map(|name| name.to_string())
        .chain(["license.md".to_owned(), "Licence.TXT".to_owned()])
    {
        let folder = TempDir::new().unwrap();
        write(&folder.path().join(&name), "ISC License\n");

        assert_eq!(
            license_of(Some("  "), folder.path()).as_deref(),
            Some("ISC License"),
            "{name}"
        );
    }
    // Of two, the bare one first.
    let folder = TempDir::new().unwrap();
    write(&folder.path().join("LICENSE.md"), "From the Markdown\n");
    write(&folder.path().join("LICENSE"), "From the bare one\n");
    assert_eq!(
        license_of(None, folder.path()).as_deref(),
        Some("From the bare one")
    );
}

#[test]
fn a_licence_is_unknown_when_nothing_names_one() {
    let none = TempDir::new().unwrap();
    write(&none.path().join("SKILL.md"), "---\nname: x\n---\n");
    let empty = TempDir::new().unwrap();
    write(&empty.path().join("LICENSE"), "\n  \n");
    // A folder of that name is not a file.
    let folder = TempDir::new().unwrap();
    write(&folder.path().join("LICENSE").join("MIT"), "MIT License\n");
    let elsewhere = TempDir::new().unwrap();
    write(&elsewhere.path().join("COPYING.LESSER"), "GNU LGPL\n");

    for unknown in [&none, &empty, &folder, &elsewhere] {
        assert_eq!(license_of(None, unknown.path()), None, "{unknown:?}");
    }
    assert_eq!(license_of(None, &none.path().join("gone")), None);
}

#[cfg(unix)]
#[test]
fn a_skill_whose_folder_is_not_utf_8_cannot_be_filed() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new().unwrap();
    let folder = dir.path().join(OsStr::from_bytes(b"caf\xe9"));
    write(&folder.join("SKILL.md"), &SkillMother::create("cafe", "x"));
    let mut failures = Vec::new();

    let taken = read(&folder, dir.path(), None, &mut failures);

    assert!(taken.is_none());
    assert!(
        matches!(&failures[..], [Failure::UnusableName { path }] if path == &folder),
        "{failures:?}"
    );
    assert_eq!(failures[0].code(), "gathered_name_not_utf8");
}

#[test]
fn a_skill_at_the_repository_s_root_is_filed_under_the_repository_s_name() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("SKILL.md"),
        &SkillMother::create("pdf", "Read PDFs"),
    );
    let mut failures = Vec::new();

    let (path, filed, artifact) = read(
        &dir.path().join("."),
        dir.path(),
        Some("pdf-skill"),
        &mut failures,
    )
    .unwrap();

    assert_eq!((path, filed.as_str()), (dir.path().join("."), "pdf-skill"));
    assert_eq!(artifact.name().as_str(), "pdf");
    assert!(failures.is_empty(), "{failures:?}");
    // Held to the folder it is filed under, not to the clone's.
    assert_eq!(
        filed_issues(&artifact, &filed),
        [ValidationIssue::NameDirectoryMismatch {
            name: String::from("pdf"),
            directory: String::from("pdf-skill"),
        }]
    );
    assert_eq!(filed_issues(&artifact, "pdf"), []);
}

#[test]
fn a_skill_at_the_root_of_a_repository_its_url_names_no_name_of_is_not_taken() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("SKILL.md"),
        &SkillMother::create("pdf", "x"),
    );
    let mut failures = Vec::new();

    for repository in [None, Some(".."), Some("a/b")] {
        failures.clear();

        assert!(read(dir.path(), dir.path(), repository, &mut failures).is_none());

        assert!(
            matches!(&failures[..], [Failure::Unnamed { path }] if path == dir.path()),
            "{failures:?}"
        );
    }
    assert_eq!(failures[0].code(), "gathered_root_unnamed");
}

#[test]
fn the_folder_searched_holding_a_manifest_is_the_one_candidate() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("SKILL.md"), "x");
    write(&dir.path().join("skills/deploy/SKILL.md"), "x");

    let (found, _) = found_in(dir.path());

    assert_eq!(found, [dir.path()]);
}

#[test]
fn a_dry_run_s_failure_is_said_by_where_it_is_in_the_repository_and_only_there() {
    let clone = Path::new("/w/.mindweaver/cache/+dry-run-1-2");
    let unread = Failure::Artifact(ArtifactError::Read {
        path: clone.join("skills/broken/SKILL.md"),
        source: io::Error::other("denied"),
    });
    let unnamed = Failure::Unnamed {
        path: clone.to_path_buf(),
    };
    let outside = Failure::LinkOutside {
        path: clone.join("skills/keys"),
        to: PathBuf::from("/home/you/.ssh"),
    };
    let shelved = Failure::Unhashed {
        path: PathBuf::from("/w/.mindweaver/skills/s/deploy"),
        source: io::Error::other("gone"),
    };

    let said: Vec<String> = [unread, unnamed, outside, shelved]
        .into_iter()
        .map(|failure| failure.in_repository(clone).path().display().to_string())
        .collect();

    assert_eq!(
        said,
        [
            "skills/broken/SKILL.md",
            ".",
            "skills/keys",
            "/w/.mindweaver/skills/s/deploy"
        ]
    );
}

/// A Ctrl-C ends a dry run without its taking its folder away: the next
/// dry run or gather does, once no run holds its lock, and never one a run
/// going on holds, nor one being made.
#[test]
fn a_dry_run_or_a_gather_takes_away_what_an_interrupted_dry_run_left_and_only_that() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().unwrap();
    let cache = workspace.cache_dir();
    let going = DryRun::open(&cache).unwrap();
    let left = |name: &str, locked: bool| {
        let folder = cache.join(format!("{DRY_RUN}{name}"));
        write(&folder.join("clone/skills/deploy/SKILL.md"), "x");
        if locked {
            write(&folder.join(DRY_RUN_LOCK), "");
        }
        folder
    };
    let request = Request::git(UrlMother::https()).through(Arc::new(Unreached::default()));
    let made = left("1-0", false);

    let before_a_dry_run = left("2-0", true);
    let _ = preview(&workspace, &ledger, &request);
    let before_a_gather = left("3-0", true);
    let _ = gather(&workspace, &ledger, &request);

    assert!(!before_a_dry_run.exists());
    assert!(!before_a_gather.exists());
    assert!(made.exists(), "no lock in it yet");
    assert!(going.folder.join(DRY_RUN_LOCK).is_file());
    let folder = going.folder.clone();
    drop(going);
    assert!(!folder.exists());
}

/// What a gather cannot copy, a link out of the repository or to nothing,
/// a dry run cannot either, and says as the gather does, so its exit is
/// the gather's.
#[cfg(unix)]
#[test]
fn a_dry_run_refuses_to_copy_what_a_gather_refuses_and_says_so_as_it_does() {
    let outside = TempDir::new().unwrap();
    write(&outside.path().join("secret"), "key");
    for to in [outside.path().join("secret"), PathBuf::from("nowhere")] {
        let (_dir, workspace) = WorkspaceMother::empty();
        let ledger = workspace.ledger().unwrap();
        let request =
            Request::git(UrlMother::https()).through(Arc::new(Linking { to: to.clone() }));

        let previewed = preview(&workspace, &ledger, &request).unwrap();
        let gathered = gather(&workspace, &ledger, &request).unwrap();

        let found: Vec<&str> = previewed.found.iter().map(|it| it.name.as_str()).collect();
        assert_eq!(found, ["review"], "{}", to.display());
        assert!(
            matches!(
                &previewed.failures[..],
                [Failure::Copy { path, .. }] if path == Path::new("skills/deploy")
            ),
            "{:?}",
            previewed.failures
        );
        let codes = |failures: &[Failure]| failures.iter().map(Failure::code).collect::<Vec<_>>();
        assert_eq!(codes(&previewed.failures), ["gathered_copy_failed"]);
        assert_eq!(codes(&gathered.failures), codes(&previewed.failures));
        let added: Vec<&str> = gathered.added.iter().map(|it| it.name.as_str()).collect();
        assert_eq!(added, found);
    }
}

/// The folder keeps the name the source gave it, whatever the skill
/// declares: the disagreement is `validate`'s to say.
#[test]
fn a_skill_is_filed_under_its_folder_s_name_whatever_it_declares() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy-old");
    write(
        &folder.join("SKILL.md"),
        &SkillMother::create("deploy", "Ship it"),
    );
    let mut failures = Vec::new();

    let (path, filed, artifact) = read(&folder, dir.path(), None, &mut failures).unwrap();

    assert_eq!(path, folder);
    assert_eq!(filed, "deploy-old");
    assert_eq!(artifact.name().as_str(), "deploy");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn a_skill_that_cannot_be_read_is_a_failure_and_not_taken() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("broken");
    write(&folder.join("SKILL.md"), "---\nname: [unclosed\n---\n");
    let mut failures = Vec::new();

    assert!(read(&folder, dir.path(), None, &mut failures).is_none());
    assert!(
        matches!(&failures[..], [Failure::Artifact(_)]),
        "{failures:?}"
    );
}

#[test]
fn each_gather_failure_names_its_path_and_gives_its_code() {
    let copy = Failure::Copy {
        path: PathBuf::from("/w/.mindweaver/skills/s/deploy"),
        source: io::Error::other("full"),
    };
    let unusable = Failure::UnusableName {
        path: PathBuf::from("/w/.mindweaver/cache/s/x"),
    };
    let unhashed = Failure::Unhashed {
        path: PathBuf::from("/w/.mindweaver/skills/s/review"),
        source: io::Error::other("gone"),
    };

    assert_eq!(copy.code(), "gathered_copy_failed");
    assert_eq!(unhashed.code(), "gathered_unhashed");
    assert_eq!(unhashed.path(), Path::new("/w/.mindweaver/skills/s/review"));
    assert_eq!(
        unhashed.to_string(),
        "/w/.mindweaver/skills/s/review: put on the shelf, but cannot be read back to be hashed, \
         so it is recorded with no hash: gone"
    );
    assert_eq!(copy.path(), Path::new("/w/.mindweaver/skills/s/deploy"));
    assert_eq!(
        copy.to_string(),
        "/w/.mindweaver/skills/s/deploy: cannot be copied into the workspace: full"
    );
    assert_eq!(unusable.path(), Path::new("/w/.mindweaver/cache/s/x"));
    let unremoved = Failure::Unremoved {
        path: PathBuf::from("/w/.mindweaver/skills/s/gone"),
        source: io::Error::other("busy"),
    };
    assert_eq!(unremoved.code(), "gathered_unremoved");
    assert_eq!(unremoved.path(), Path::new("/w/.mindweaver/skills/s/gone"));
    assert_eq!(
        unremoved.to_string(),
        "/w/.mindweaver/skills/s/gone: no longer in its source, but cannot be taken off the \
         shelf, so it is kept, and still offered: busy"
    );
    let outside = Failure::LinkOutside {
        path: PathBuf::from("/w/.mindweaver/cache/s/skills/keys"),
        to: PathBuf::from("/home/you/.ssh"),
    };
    assert_eq!(
        outside.path(),
        Path::new("/w/.mindweaver/cache/s/skills/keys")
    );
    assert_eq!(
        outside.to_string(),
        "/w/.mindweaver/cache/s/skills/keys: links to /home/you/.ssh, outside the repository, \
         so it is not gathered"
    );
    let unnamed = Failure::Unnamed {
        path: PathBuf::from("/w/.mindweaver/cache/s"),
    };
    assert_eq!(unnamed.path(), Path::new("/w/.mindweaver/cache/s"));
    assert_eq!(
        unnamed.to_string(),
        "/w/.mindweaver/cache/s: a SKILL.md at the repository's root is filed under the \
         repository's name, and its URL names none, so it is not gathered: gather it by a URL \
         or a path that ends in a name"
    );
    let clash = Failure::Clash {
        path: PathBuf::from("/w/.mindweaver/cache/s/skills/pdf"),
        name: String::from("pdf"),
        with: vec![
            String::from(".claude/skills/pdf"),
            String::from("vendor/pdf"),
        ],
    };
    assert_eq!(clash.path(), Path::new("/w/.mindweaver/cache/s/skills/pdf"));
    assert_eq!(
        clash.to_string(),
        "/w/.mindweaver/cache/s/skills/pdf: another skill in this source is called `pdf` too, \
         at .claude/skills/pdf, vendor/pdf, so neither is gathered: gather the one you want \
         with --path naming its folder"
    );
    let listed_outside = Failure::ListedOutside {
        path: PathBuf::from("/w/.mindweaver/cache/s/../../etc"),
        listed: String::from("../../etc"),
    };
    assert_eq!(
        listed_outside.path(),
        Path::new("/w/.mindweaver/cache/s/../../etc")
    );
    assert_eq!(
        listed_outside.to_string(),
        "/w/.mindweaver/cache/s/../../etc: its plugin's catalog entry names `../../etc` as a \
         skill, which is no folder of the repository, so it is not gathered"
    );
    let listed_missing = Failure::ListedMissing {
        path: PathBuf::from("/w/.mindweaver/cache/s/skills/gone"),
    };
    assert_eq!(
        listed_missing.to_string(),
        "/w/.mindweaver/cache/s/skills/gone: its plugin's catalog entry names it as a skill, \
         and it holds no SKILL.md, so it is not gathered"
    );
    let unreadable = Failure::ManifestUnreadable {
        path: PathBuf::from("/w/.mindweaver/cache/s/.codex-plugin/plugin.json"),
        reason: String::from("it is not a JSON object"),
    };
    assert_eq!(
        unreadable.path(),
        Path::new("/w/.mindweaver/cache/s/.codex-plugin/plugin.json")
    );
    assert_eq!(
        unreadable.to_string(),
        "/w/.mindweaver/cache/s/.codex-plugin/plugin.json: its plugin's manifest cannot be read: \
         it is not a JSON object, so its plugin is searched as if it had none"
    );
    let manifest_outside = Failure::ManifestOutside {
        path: PathBuf::from("/w/.mindweaver/cache/s/.claude-plugin/plugin.json"),
        named: String::from("../../etc"),
    };
    assert_eq!(
        manifest_outside.path(),
        Path::new("/w/.mindweaver/cache/s/.claude-plugin/plugin.json")
    );
    assert_eq!(
        manifest_outside.to_string(),
        "/w/.mindweaver/cache/s/.claude-plugin/plugin.json: names `../../etc` as a folder of its \
         plugin's skills, which is no folder of the repository, so nothing there is gathered"
    );
    assert_eq!(
        Failure::CODES,
        [
            "gathered_name_not_utf8",
            "gathered_copy_failed",
            "gathered_unhashed",
            "gathered_unremoved",
            "gathered_link_outside",
            "gathered_root_unnamed",
            "gathered_name_clash",
            "gathered_listed_outside",
            "gathered_listed_missing",
            "gathered_manifest_unreadable",
            "gathered_manifest_outside"
        ]
    );
}

#[test]
fn a_warning_names_the_manifest_and_what_is_wrong_with_it() {
    let warning = Warning {
        path: PathBuf::from("/w/.mindweaver/skills/s/deploy/SKILL.md"),
        issue: ValidationIssue::MetadataNotAMap,
    };

    assert_eq!(
        warning.to_string(),
        format!(
            "/w/.mindweaver/skills/s/deploy/SKILL.md: {}",
            ValidationIssue::MetadataNotAMap
        )
    );
}

#[test]
fn each_gather_error_of_its_own_gives_its_own_code() {
    let errors = [
        (GatherError::Cancelled, "gather_cancelled"),
        (
            GatherError::CredentialsInUrl {
                url: UrlMother::https(),
            },
            "credentials_in_url",
        ),
        (
            GatherError::NoSuchFolder {
                subdirectory: String::from("skills"),
                url: UrlMother::https(),
            },
            "gather_no_such_folder",
        ),
        (
            GatherError::PathOutside {
                subdirectory: String::from("../.."),
            },
            "gather_path_outside",
        ),
        (
            GatherError::NameInvalid {
                name: String::from("acme/skills"),
            },
            "source_name_invalid",
        ),
        (
            GatherError::NameTaken {
                name: SourceMother::random().name,
                origin: SourceMother::random().origin(),
            },
            "source_name_taken",
        ),
        (
            GatherError::PinInvalid {
                pin: String::from("4f9c2a7"),
            },
            "gather_pin_invalid",
        ),
        (
            GatherError::NotGatherable { kind: Kind::Rule },
            "kind_not_gatherable",
        ),
        (
            GatherError::RefContradicted {
                address: String::from("acme/skills@v2"),
                named: String::from("v2"),
                given: String::from("v3"),
            },
            "gather_ref_contradicted",
        ),
        (
            GatherError::PathContradicted {
                address: String::from("https://github.com/acme/skills/tree/main/skills"),
                named: String::from("skills"),
                given: String::from("agents"),
            },
            "gather_path_contradicted",
        ),
        (
            GatherError::TreeUndecodable {
                address: String::from("https://github.com/acme/skills/tree/main/a%2Fb"),
                segment: String::from("a%2Fb"),
            },
            "gather_tree_undecodable",
        ),
        (
            GatherError::Read {
                path: PathBuf::from("/w"),
                source: io::Error::other("denied"),
            },
            "gather_read_failed",
        ),
    ];
    let given: Vec<&str> = errors.iter().map(|(_, code)| *code).collect();
    for (error, code) in errors {
        assert_eq!(error.code(), code, "{error:?}");
    }
    assert_eq!(GatherError::CODES, given.as_slice());
}

/// A source recorded with a folder that is not its own is the condition
/// `gather remove` meets, and is given its code.
#[test]
fn a_gather_error_echoes_the_code_a_source_recorded_with_a_folder_not_its_own_has() {
    let error = GatherError::FolderNotItsOwn {
        name: SourceMother::random().name,
        directory: String::from(".."),
    };

    assert_eq!(error.code(), "source_folder_not_its_own");
    assert_eq!(GatherError::ECHOED, [error.code()]);
    assert!(crate::sources::SourceError::CODES.contains(&error.code()));
}

/// A gather carrying another of core's failures says that failure's code,
/// so the panel tells a busy workspace or a refused fetch apart the same
/// way whatever met it.
#[test]
fn a_gather_error_carrying_another_of_core_s_gives_its_code() {
    let carried = [
        (
            GatherError::from(GitError::Fetch {
                url: UrlMother::https(),
                detail: String::from("refused"),
            }),
            "git_fetch_failed",
        ),
        (
            GatherError::from(LedgerError::Unmade {
                path: PathBuf::from("/w/.mindweaver/mindweaver.db"),
            }),
            "ledger_unmade",
        ),
        (
            GatherError::from(LockError::Busy {
                path: PathBuf::from("/w/.mindweaver/lock"),
                pid: None,
                waited: crate::lock::WAIT,
            }),
            "workspace_busy",
        ),
    ];
    for (error, code) in carried {
        assert_eq!(error.code(), code, "{error:?}");
        assert!(!GatherError::CODES.contains(&code), "{code}");
    }
}

#[test]
fn a_folder_the_source_does_not_have_is_named_with_the_url() {
    let url = UrlMother::https();
    let error = GatherError::NoSuchFolder {
        subdirectory: String::from("agents/skills"),
        url: url.clone(),
    };

    assert_eq!(
        error.to_string(),
        format!("{url}: has no `agents/skills` folder to gather from")
    );
}

#[test]
fn a_clone_that_kept_its_credential_is_said_by_its_url_without_it() {
    for typed in UrlMother::every_credentialed() {
        let failure = scrub_failure(&typed.typed);

        assert!(!failure.to_string().contains(&typed.secret), "{failure}");
        assert!(
            failure
                .to_string()
                .starts_with(&format!("{}: ", typed.clean)),
            "{failure}"
        );
        assert_eq!(failure.code(), "clone_unscrubbed");
    }
}

#[test]
fn a_folder_left_behind_is_said_with_why_and_its_code() {
    let left = leftover("/w/.mindweaver/cache/a");

    assert_eq!(
        left.to_string(),
        "/w/.mindweaver/cache/a: left by a source that is now one with another at the same URL, \
         and cannot be removed: busy"
    );
    assert_eq!(left.code(), "source_leftover");
}

/// A directory that is no one folder name was named by no ledger, and is
/// left rather than followed out of the workspace.
#[test]
fn the_folders_of_a_source_whose_directory_is_no_folder_name_are_left_alone() {
    let (dir, workspace) = WorkspaceMother::empty();
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let mut leftovers = Vec::new();

    remove_folders_of(&workspace, "../../outside", &mut leftovers);

    assert!(outside.is_dir());
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_folders_of_a_source_that_went_are_removed_from_the_cache_and_every_shelf() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let gone = SourceMother::random();
    let folders: Vec<PathBuf> = std::iter::once(workspace.cache_dir())
        .chain(Kind::ALL.iter().map(|kind| workspace.gathered_dir(*kind)))
        .map(|folder| folder.join(&gone.directory))
        .collect();
    for folder in &folders {
        write(&folder.join("x"), "x");
    }
    let mut leftovers = Vec::new();

    remove_folders_of(&workspace, &gone.directory, &mut leftovers);

    for folder in &folders {
        assert!(!folder.exists(), "{folder:?}");
    }
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn removing_the_folders_of_a_source_that_has_none_is_no_failure() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let mut leftovers = Vec::new();

    remove_folders_of(
        &workspace,
        &SourceMother::random().directory,
        &mut leftovers,
    );

    assert!(leftovers.is_empty(), "{leftovers:?}");
}
