//! Unit tests for `url.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn redacting_drops_the_userinfo_before_the_host() {
    assert_eq!(
        redacted("https://x-access-token:ghp_secret@github.com/acme/private"),
        "https://github.com/acme/private"
    );
    // A password that kept a raw `@` still ends at the last one before the
    // host.
    assert_eq!(
        redacted("https://me:p@ss@example.com/x"),
        "https://example.com/x"
    );
    // A host with no path after it.
    assert_eq!(
        redacted("https://me:hunter2@example.com"),
        "https://example.com"
    );
}

#[test]
fn a_user_name_alone_goes_too_where_a_token_can_stand_in_for_one() {
    // GitHub takes a token as the user name of an https URL.
    assert_eq!(
        redacted("https://ghp_secret@github.com/acme/private"),
        "https://github.com/acme/private"
    );
    assert_eq!(
        redacted("git://me@example.com/acme/skills"),
        "git://example.com/acme/skills"
    );
}

#[test]
fn the_user_ssh_logs_in_as_is_kept_and_only_its_password_goes() {
    for url in [
        "ssh://git@example.com:2222/acme/private.git",
        "git+ssh://git@example.com/acme/private.git",
        "SSH://git@example.com/acme/private.git",
    ] {
        assert_eq!(redacted(url), url);
    }
    assert_eq!(
        redacted("ssh://git:hunter2@example.com/acme/private.git"),
        "ssh://git@example.com/acme/private.git"
    );
    // A password with no user before it leaves nothing to log in as.
    assert_eq!(
        redacted("ssh://:hunter2@example.com/acme/private.git"),
        "ssh://example.com/acme/private.git"
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
        assert!(matches!(redacted(url), Cow::Borrowed(_)), "{url}");
    }
}

#[test]
fn every_url_in_a_message_is_redacted_and_the_words_around_them_kept() {
    assert_eq!(
        redacted(
            "URL \"https://me:hunter2@example.com\" can not be parsed, \
             and ssh://git:hunter2@example.com/x refused it"
        ),
        "URL \"https://example.com\" can not be parsed, and ssh://git@example.com/x refused it"
    );
    // Whitespace ends a URL, so an `@` in the words after one is left alone.
    assert_eq!(
        redacted("https://example.com failed for me@example.com"),
        "https://example.com failed for me@example.com"
    );
    // Text before the scheme that is not ASCII is stepped over, not cut.
    assert_eq!(
        redacted("échec: https://me:hunter2@example.com/x"),
        "échec: https://example.com/x"
    );
}

#[test]
fn a_url_carries_credentials_when_redacting_would_change_it() {
    assert!(carries_credentials("https://u:t@example.com/x"));
    assert!(carries_credentials("https://t@example.com/x"));
    assert!(carries_credentials("ssh://git:t@example.com/x"));
    assert!(!carries_credentials("ssh://git@example.com/x"));
    assert!(!carries_credentials("git@example.com:acme/x.git"));
    assert!(!carries_credentials("https://example.com/x"));
}
