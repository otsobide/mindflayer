//! Unit tests for `git.rs`, kept beside it rather than inside it.

use super::*;

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
fn a_login_ssh_refused_is_a_missing_credential() {
    for url in [
        "git@example.com:acme/private.git",
        "ssh://git@example.com/acme/private.git",
    ] {
        let error = fetch_failed(url, Some(Shortfall::Refused), "refused".to_owned());

        assert!(
            matches!(error, GitError::Credentials { .. }),
            "{url}: {error}"
        );
    }
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
