//! Unit tests for `git.rs`, kept beside it rather than inside it.

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
        "Failed to obtain credentials".to_owned(),
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
        "not accepted by the remote".to_owned(),
    );
    assert!(matches!(error, GitError::Credentials { .. }), "{error}");

    // The reverse rewrite: typed as ssh, fetched over https, and the server
    // turned down the credential a helper gave.
    let error = fetch_failed(
        "git@example.com:acme/private.git",
        refused_at("https://example.com/acme/private.git"),
        "not accepted by the remote".to_owned(),
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
        "not accepted by the remote".to_owned(),
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
        "connection refused".to_owned(),
    );

    assert!(matches!(error, GitError::Fetch { .. }), "{error}");
}

#[test]
fn redacting_drops_the_userinfo_before_the_host() {
    assert_eq!(
        redacted("https://x-access-token:ghp_secret@github.com/acme/private"),
        "https://github.com/acme/private"
    );
    assert_eq!(
        redacted("ssh://git@example.com:2222/acme/private.git"),
        "ssh://example.com:2222/acme/private.git"
    );
    // A password that kept a raw `@` still ends at the last one before the
    // host.
    assert_eq!(
        redacted("https://me:p@ss@example.com/x"),
        "https://example.com/x"
    );
}

#[test]
fn redacting_leaves_a_url_without_userinfo_as_typed() {
    for url in [
        "https://github.com/acme/skills",
        "HTTPS://github.com:443/acme/skills",
        // An `@` after the host is part of the path, not a user.
        "file:///srv/git/team@work/skills",
        "https://example.com/acme/skills?ref=a@b",
        // The scp-like form has no room for a password.
        "git@github.com:acme/skills.git",
        "/srv/git/skills",
    ] {
        assert_eq!(redacted(url), url);
    }
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
