//! Unit tests for `port.rs`, kept beside it rather than inside it.
//!
//! A port's error is what every adapter of it says in core's words, so its
//! codes and its messages are the same whichever adapter met the failure:
//! pinned here, beside the promise that a message says only what its fields
//! hold, which is what keeps a credential the adapter took out of a URL from
//! coming back in the words around it.

use super::*;

use std::io;

use crate::mother::random;
use crate::mother::{Credentialed, UrlMother};
use crate::url::redacted;

/// One of each variant with a code of its own, built from `url` and
/// `detail`, beside the code it gives.
fn every_own_error(url: &str, detail: &str) -> Vec<(GitError, &'static str)> {
    vec![
        (
            GitError::Url {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_url_invalid",
        ),
        (
            GitError::Reference {
                reference: String::from("main"),
                detail: detail.to_owned(),
            },
            "git_reference_invalid",
        ),
        (
            GitError::Cache {
                path: PathBuf::from("/w/.mindweaver/cache/acme"),
                source: io::Error::other(detail.to_owned()),
            },
            "git_cache_failed",
        ),
        (
            GitError::Fetch {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_fetch_failed",
        ),
        (
            GitError::Credentials {
                url: url.to_owned(),
            },
            "git_credentials_needed",
        ),
        (
            GitError::Checkout {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_checkout_failed",
        ),
        (
            GitError::PinUnreachable {
                url: url.to_owned(),
                pin: Revision::from("4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3"),
                reference: Some(String::from("main")),
            },
            "git_pin_unreachable",
        ),
        (GitError::Cancelled, "git_clone_cancelled"),
    ]
}

/// A failure of `url` as an adapter hands it over: the URL and the words
/// quoting it both redacted at the edge, as the gix adapter redacts them.
fn as_an_adapter_says(typed: &Credentialed) -> Vec<GitError> {
    let quoted = UrlMother::quoted(typed);
    every_own_error(&redacted(&typed.typed), &redacted(&quoted.typed))
        .into_iter()
        .map(|(error, _)| error)
        .collect()
}

#[test]
fn each_git_error_of_its_own_gives_its_own_code() {
    for (error, code) in every_own_error("https://example.com/acme/skills.git", "refused") {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

#[test]
fn the_codes_a_git_error_gives_of_its_own_are_listed_in_order() {
    let given: Vec<&str> = every_own_error("u", "d")
        .into_iter()
        .map(|(_, code)| code)
        .collect();

    assert_eq!(GitError::CODES, given.as_slice());
}

/// A clone whose file could not be scrubbed failed for the workspace's
/// reason, and says the workspace's code, never one of the port's own.
#[test]
fn a_git_error_carrying_a_workspace_s_gives_the_workspace_s_code() {
    let carried = [
        WorkspaceError::NotAWorkspace {
            path: PathBuf::from("/w"),
        },
        WorkspaceError::Read {
            path: PathBuf::from("/w/.mindweaver/cache/acme/.git/config"),
            source: io::Error::other("denied"),
        },
        WorkspaceError::Malformed {
            path: PathBuf::from("/w/.mindweaver/cache/acme/.git/config"),
            detail: String::from("not a config"),
        },
    ];
    for workspace in carried {
        let code = workspace.code();
        let said = workspace.to_string();
        let error = GitError::from(workspace);

        assert_eq!(error.code(), code, "{error:?}");
        assert!(!GitError::CODES.contains(&code), "{code}");
        assert_eq!(error.to_string(), said, "said as the workspace says it");
    }
}

#[test]
fn a_url_git_does_not_understand_is_named_with_why() {
    let error = GitError::Url {
        url: String::from("htps//acme"),
        detail: String::from("no scheme"),
    };

    assert_eq!(
        error.to_string(),
        "htps//acme: not a URL git understands: no scheme"
    );
}

#[test]
fn a_reference_that_is_no_name_is_quoted_in_backticks() {
    let error = GitError::Reference {
        reference: String::from("main..dev"),
        detail: String::from("a ref cannot hold two dots"),
    };

    assert_eq!(
        error.to_string(),
        "`main..dev` is not a usable branch or tag name: a ref cannot hold two dots"
    );
}

#[test]
fn a_cache_that_cannot_be_prepared_names_its_folder_and_why() {
    let error = GitError::Cache {
        path: PathBuf::from("/w/.mindweaver/cache/acme"),
        source: io::Error::other("read-only file system"),
    };

    assert_eq!(
        error.to_string(),
        "/w/.mindweaver/cache/acme: the cache directory cannot be prepared: read-only file system"
    );
}

#[test]
fn a_fetch_that_failed_names_the_url_and_what_the_remote_said() {
    let error = GitError::Fetch {
        url: String::from("https://example.com/acme/skills.git"),
        detail: String::from("connection refused"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills.git: cannot be fetched: connection refused"
    );
}

/// What to do about it is the message's: a gather never prompts, so the
/// person reading it is told where a credential is looked for instead.
#[test]
fn a_remote_that_asks_who_is_fetching_says_where_a_credential_is_looked_for() {
    let error = GitError::Credentials {
        url: String::from("https://example.com/acme/private.git"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: needs credentials, and gathering never \
         prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL"
    );
}

#[test]
fn a_pin_a_remote_will_not_hand_over_says_the_history_it_was_looked_for_in() {
    let pin = Revision::from("4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3");
    let at = |reference: Option<&str>| GitError::PinUnreachable {
        url: String::from("https://example.com/acme/skills.git"),
        pin: pin.clone(),
        reference: reference.map(str::to_owned),
    };

    assert_eq!(
        at(Some("v2")).to_string(),
        "https://example.com/acme/skills.git: commit 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3 is \
         neither handed over by itself nor in the history of `v2`, so it cannot be gathered: \
         name the branch or tag it is on with --ref"
    );
    assert!(
        at(None)
            .to_string()
            .contains("in the history of the remote's HEAD,"),
        "{}",
        at(None)
    );
}

#[test]
fn a_checkout_that_failed_says_the_clone_was_made() {
    let error = GitError::Checkout {
        url: String::from("https://example.com/acme/skills.git"),
        detail: String::from("path too long"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills.git: cloned, but its files could not be checked out: \
         path too long"
    );
}

#[test]
fn a_cancelled_clone_says_it_was_cancelled() {
    assert_eq!(
        GitError::Cancelled.to_string(),
        "the clone was cancelled before it was finished"
    );
}

/// The adapter redacts at the edge, and the message holds what its fields
/// hold and nothing besides: no variant's words quote the URL again, nor
/// anything an adapter did not hand over, so no form of credential comes
/// back into what the gather log and the screen are given.
#[test]
fn no_git_error_built_at_the_edge_repeats_a_credential_however_it_was_typed() {
    for typed in UrlMother::every_credentialed() {
        for error in as_an_adapter_says(&typed) {
            let said = error.to_string();

            assert!(!said.contains(&typed.secret), "{typed:?} gave {said}");
            assert!(!format!("{error:?}").contains(&typed.secret), "{error:?}");
        }
    }
}

#[test]
fn a_git_error_about_a_url_names_it_as_the_redaction_left_it() {
    for typed in UrlMother::every_credentialed() {
        for error in as_an_adapter_says(&typed) {
            if matches!(
                error,
                GitError::Url { .. }
                    | GitError::Fetch { .. }
                    | GitError::Credentials { .. }
                    | GitError::Checkout { .. }
            ) {
                let said = error.to_string();

                assert!(
                    said.starts_with(&format!("{}: ", typed.clean)),
                    "{typed:?} gave {said}"
                );
            }
        }
    }
}

/// Each URL is said once: a message that named it twice would say it in a
/// spelling of its own the second time, which nothing redacted.
#[test]
fn a_git_error_says_its_url_once() {
    for _ in 0..50 {
        let url = UrlMother::https();
        let detail = random::word();
        for (error, _) in every_own_error(&url, &detail) {
            let said = error.to_string();

            assert!(said.matches(url.as_str()).count() <= 1, "{said}");
        }
    }
}

#[test]
fn a_clone_is_equal_to_one_at_the_same_place_and_revision() {
    let clone = Cloned {
        root: PathBuf::from("/w/.mindweaver/cache/acme"),
        revision: Some(Revision::from("4f9c2a7")),
    };

    assert_eq!(clone.clone(), clone);
    assert_ne!(
        Cloned {
            revision: None,
            ..clone.clone()
        },
        clone
    );
}
