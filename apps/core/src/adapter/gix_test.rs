//! Unit tests for `gix.rs`, kept beside it rather than inside it.

use super::*;

use gix::credentials::helper::Action;
use gix::credentials::protocol;

/// What the credentials callback notes when the server turns down the login a
/// helper gave for `connected`, the URL `gix` connected to.
// The stub helper's error is `gix`'s, fixed by the callback's signature.
#[allow(clippy::result_large_err)]
fn refused_at(connected: &str) -> Option<Shortfall> {
    let shortfalls = Shortfalls::default();
    let mut callback = credentials::never_prompting(
        |action, _| match action {
            Action::Get(context) => Ok(Some(protocol::Outcome {
                identity: gix::sec::identity::Account {
                    username: "git".into(),
                    password: "hunter2".into(),
                    oauth_refresh_token: None,
                },
                next: context.into(),
            })),
            Action::Store(_) | Action::Erase(_) => Ok(None),
        },
        shortfalls.clone(),
    );
    let Ok(Some(answer)) = callback(Action::get_for_url(connected)) else {
        panic!("{connected}: the helper's credential was not passed on");
    };
    callback(answer.next.erase()).unwrap();
    shortfalls.last()
}

#[test]
fn a_missing_credential_names_the_url_without_its_userinfo_and_the_ways_to_provide_one() {
    let error = fetch_failed(
        "https://me:hunter2@example.com/acme/private.git",
        Some(Shortfall::Missing),
        &"Failed to obtain credentials",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: needs credentials, and gathering never \
         prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL"
    );
}

#[test]
fn a_refused_login_is_judged_by_the_url_gix_connected_to_not_the_one_typed() {
    // `url."git@example.com:".insteadOf = https://example.com/`: typed as
    // https, fetched over ssh, and ssh had no key the server takes.
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        refused_at("git@example.com:acme/private.git"),
        &"not accepted by the remote",
    );
    assert!(matches!(error, GitError::Credentials { .. }), "{error}");

    // The reverse rewrite: typed as ssh, fetched over https, and the server
    // turned down the credential a helper gave.
    let error = fetch_failed(
        "git@example.com:acme/private.git",
        refused_at("https://example.com/acme/private.git"),
        &"not accepted by the remote",
    );
    assert_eq!(
        error.to_string(),
        "git@example.com:acme/private.git: cannot be fetched: not accepted by the remote"
    );
}

#[test]
fn a_refused_https_credential_keeps_what_gix_said() {
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        Some(Shortfall::Refused),
        &"not accepted by the remote",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: cannot be fetched: not accepted by the remote"
    );
}

#[test]
fn a_fetch_that_needed_no_credential_keeps_what_gix_said() {
    let error = fetch_failed(
        "https://example.com/acme/skills",
        None,
        &"connection refused",
    );

    assert!(matches!(error, GitError::Fetch { .. }), "{error}");
}

#[test]
fn a_failed_fetch_names_no_credential_even_where_gix_repeats_one() {
    // `url."https://me:hunter2@example.com/".insteadOf = https://example.com/`
    // gave the URL gix connected to a token the typed one never had.
    let error = fetch_failed(
        "https://example.com/acme/private.git",
        None,
        &"error sending request for url \
          (https://me:hunter2@example.com/acme/private.git/info/refs?service=git-upload-pack)",
    );

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: cannot be fetched: error sending request for url \
         (https://example.com/acme/private.git/info/refs?service=git-upload-pack)"
    );
}

#[test]
fn a_url_gix_cannot_parse_is_named_without_its_credentials_and_so_is_what_gix_said() {
    let cache = tempfile::TempDir::new().unwrap();
    let error = clone(
        "https://me:hunter2@exa mple.com/acme/private.git",
        At::Reference(None),
        &cache.path().join("clone"),
        &Watch::new(),
    )
    .unwrap_err();

    assert!(matches!(error, GitError::Url { .. }), "{error}");
    assert!(!error.to_string().contains("hunter2"), "{error}");
}

/// A clone's `.git` as `gix` leaves it after cloning `url`, without the
/// objects, which no scrub reads.
fn clone_made_from(url: &str) -> tempfile::TempDir {
    let clone = tempfile::TempDir::new().unwrap();
    let git = clone.path().join(".git");
    let logs = git.join("logs").join("refs").join("remotes").join("origin");
    fs::create_dir_all(&logs).unwrap();
    fs::write(
        git.join("config"),
        format!(
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = {url}\n\
             \tfetch = +refs/heads/main:refs/remotes/origin/main\n"
        ),
    )
    .unwrap();
    let entry = format!("0000 1111 Fixture <fixture@example.com> 0 +0000\tclone: from {url}\n");
    fs::write(git.join("logs").join("HEAD"), &entry).unwrap();
    fs::write(logs.join("HEAD"), &entry).unwrap();
    clone
}

/// Every file under `root`, read as text, for asserting on what is left.
fn everything_in(root: &Path) -> String {
    let mut files = Vec::new();
    files_under(root, &mut files).unwrap();
    files
        .iter()
        .map(|file| fs::read_to_string(file).unwrap())
        .collect()
}

#[test]
fn scrubbing_a_clone_takes_the_credentials_out_of_its_config_and_its_reflogs() {
    let clone = clone_made_from("https://me:hunter2@example.com/acme/private.git");

    assert!(scrub(clone.path()).unwrap());

    let left = everything_in(clone.path());
    assert!(!left.contains("hunter2"), "{left}");
    assert!(
        left.contains("\turl = https://example.com/acme/private.git\n"),
        "{left}"
    );
    assert!(
        left.contains("\tclone: from https://example.com/acme/private.git\n"),
        "{left}"
    );
    // The rest of the config is left as it was.
    assert!(left.contains("fetch = +refs/heads/main:refs/remotes/origin/main"));
}

#[test]
fn scrubbing_a_clone_again_or_one_without_credentials_changes_nothing() {
    let clone = clone_made_from("https://me:hunter2@example.com/acme/private.git");
    scrub(clone.path()).unwrap();
    assert!(!scrub(clone.path()).unwrap());

    let clean = clone_made_from("git@example.com:acme/private.git");
    let before = everything_in(clean.path());
    assert!(!scrub(clean.path()).unwrap());
    assert_eq!(everything_in(clean.path()), before);

    // Nothing there at all is nothing to scrub.
    let gone = tempfile::TempDir::new().unwrap();
    assert!(!scrub(&gone.path().join("never-cloned")).unwrap());
}

#[test]
fn the_step_read_off_gix_progress_is_the_latest_under_way() {
    use gix::odb::pack::index::write::ProgressId as Pack;
    use gix::progress::Count as _;

    let root = tree::Root::new();
    let mut fetch = root.add_child("fetch");
    let mut snapshot = Vec::new();
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(reading(&snapshot), None, "nothing counted yet");

    let received = fetch.add_child_with_id("indexing", Pack::IndexObjects.into());
    received.init(Some(340), None);
    received.set(120);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::Receiving {
            done: 120,
            total: Some(340)
        })
    );

    // Resolving starts while the receiving count is still in the tree.
    let resolved = fetch.add_child_with_id("Resolving", Pack::ResolveObjects.into());
    resolved.init(Some(340), None);
    resolved.set(7);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::Resolving {
            done: 7,
            total: Some(340)
        })
    );
    drop((received, resolved));

    let files = root.add_child_with_id(
        "checkout",
        gix::clone::checkout::main_worktree::ProgressId::CheckoutFiles.into(),
    );
    files.init(Some(12), None);
    // Counted the way gix counts files: on the shared counter, without
    // telling the progress.
    files.counter().fetch_add(5, Ordering::Relaxed);
    root.sorted_snapshot(&mut snapshot);
    assert_eq!(
        reading(&snapshot),
        Some(Progress::CheckingOut {
            done: 5,
            total: Some(12)
        })
    );
}

/// A repository on the disk whose `skills/deploy` says `first` at its first
/// commit and `second` at its second, and both commits, oldest first.
fn moved_on() -> (tempfile::TempDir, Revision, Revision) {
    use crate::mother::GitRepositoryMother;

    let deploy = "skills/deploy/SKILL.md";
    let repository = GitRepositoryMother::create(&[(deploy, "first\n")]);
    let head = |dir: &Path| Revision::from(gix::open(dir).unwrap().head_id().unwrap().to_string());
    let first = head(repository.path());
    GitRepositoryMother::write(repository.path(), &[(deploy, "second\n")]);
    GitRepositoryMother::commit(repository.path());
    let second = head(repository.path());
    (repository, first, second)
}

fn deploy_in(cloned: &Cloned) -> String {
    fs::read_to_string(cloned.root.join("skills/deploy/SKILL.md")).unwrap()
}

#[test]
fn a_pin_is_fetched_by_itself_and_checked_out() {
    let (repository, first, _) = moved_on();
    let cache = tempfile::TempDir::new().unwrap();
    let url = repository.path().to_string_lossy();

    let cloned = Gix
        .fetch(
            &url,
            None,
            Some(&first),
            &cache.path().join("clone"),
            &Watch::new(),
        )
        .unwrap();

    assert_eq!(cloned.revision, Some(first));
    assert_eq!(deploy_in(&cloned), "first\n");
}

#[test]
fn a_pin_a_remote_will_not_hand_over_is_looked_for_in_its_references_history() {
    let (repository, first, second) = moved_on();
    let url = repository.path().to_string_lossy();
    let head = fs::read_to_string(repository.path().join(".git/HEAD")).unwrap();
    let branch = head
        .trim()
        .strip_prefix("ref: refs/heads/")
        .unwrap()
        .to_owned();

    for reference in [None, Some(branch.as_str())] {
        let cache = tempfile::TempDir::new().unwrap();
        let into = cache.path().join("clone");
        let cloned = reaching(&url, reference, &first, &into, &Watch::new()).unwrap();

        assert_eq!(cloned.revision.as_ref(), Some(&first), "{reference:?}");
        assert_eq!(deploy_in(&cloned), "first\n", "{reference:?}");
        // The tip it was found from is kept where the remote's branch is.
        let clone = gix::open(&into).unwrap();
        let tracked = clone
            .find_reference(format!("refs/remotes/origin/{branch}").as_str())
            .unwrap()
            .into_fully_peeled_id()
            .unwrap();
        assert_eq!(tracked.to_string(), second.as_str(), "{reference:?}");
    }
}

#[test]
fn a_pin_its_reference_does_not_reach_is_refused_and_leaves_no_clone() {
    let (repository, _, _) = moved_on();
    // A commit of another repository's, which this one does not hold: its
    // files are its own, since two repositories made alike commit alike.
    let elsewhere = crate::mother::GitRepositoryMother::create(&[("README.md", "elsewhere\n")]);
    let other = Revision::from(
        gix::open(elsewhere.path())
            .unwrap()
            .head_id()
            .unwrap()
            .to_string(),
    );
    drop(elsewhere);
    let url = repository.path().to_string_lossy();
    let cache = tempfile::TempDir::new().unwrap();
    let into = cache.path().join("clone");

    // Asked by itself first, which the remote refuses, holding no such
    // commit, and then looked for in the history of its HEAD.
    let refused = Gix
        .fetch(&url, None, Some(&other), &into, &Watch::new())
        .unwrap_err();

    assert!(
        matches!(&refused, GitError::PinUnreachable { pin, reference: None, .. } if *pin == other),
        "{refused}"
    );
    assert_eq!(refused.code(), "git_pin_unreachable");
    assert_eq!(
        refused.to_string(),
        format!(
            "{url}: commit {other} is neither handed over by itself nor in the history of the \
             remote's HEAD, so it cannot be gathered: name the branch or tag it is on with --ref"
        )
    );
    assert!(!into.exists());
}
