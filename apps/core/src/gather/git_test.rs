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
        None,
        &cache.path().join("clone"),
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
