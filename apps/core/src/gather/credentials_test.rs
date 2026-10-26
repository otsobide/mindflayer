//! Unit tests for `credentials.rs`, kept beside it rather than inside it.

use super::*;

use tempfile::TempDir;

const PRIVATE: &str = "https://example.com/acme/private.git";

/// A cascade with no helper configured, whose prompt panics.
///
/// `gix`'s cascade hands its options to the prompt when no helper had a
/// credential, and asks a person unless the mode forbids it. So this panics on
/// any options that would let it ask, or that name an askpass program at all,
/// and otherwise runs the real, empty cascade.
fn no_helper_and_a_prompt_that_panics() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, options| {
        if options.mode != prompt::Mode::Disable || options.askpass.is_some() {
            panic!("the prompt was reached for {action:?}");
        }
        Cascade::default().invoke(action, options)
    }
}

/// A repository that reads no config but its own, so no helper or askpass of
/// the machine running the tests can answer.
fn isolated_repository(dir: &TempDir) -> gix::Repository {
    gix::ThreadSafeRepository::init_opts(
        dir.path(),
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .expect("initialize a repository")
    .to_thread_local()
}

/// A helper that has a credential for every URL.
fn a_helper_that_answers() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, _| match action {
        Action::Get(context) => Ok(Some(protocol::Outcome {
            identity: gix::sec::identity::Account {
                username: "me".into(),
                password: "hunter2".into(),
                oauth_refresh_token: None,
            },
            next: context.into(),
        })),
        Action::Store(_) | Action::Erase(_) => Ok(None),
    }
}

#[test]
fn a_credential_no_helper_has_is_an_error_and_never_a_prompt() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(no_helper_and_a_prompt_that_panics(), shortfalls.clone());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn a_credential_the_server_takes_leaves_no_shortfall() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the helper's credential was not passed on");
    };
    assert_eq!(answer.identity.username, "me");
    credentials(answer.next.store()).unwrap();

    assert_eq!(shortfalls.last(), None);
}

#[test]
fn a_credential_the_server_refuses_is_a_shortfall() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the helper's credential was not passed on");
    };
    credentials(answer.next.erase()).unwrap();

    assert_eq!(shortfalls.last(), Some(Shortfall::Refused));
}

#[test]
fn a_login_ssh_refuses_is_a_missing_credential() {
    for url in [
        "git@example.com:acme/private.git",
        "ssh://git@example.com/acme/private.git",
    ] {
        let shortfalls = Shortfalls::default();
        let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

        let Ok(Some(answer)) = credentials(Action::get_for_url(url)) else {
            panic!("{url}: the helper's user name was not passed on");
        };
        credentials(answer.next.erase()).unwrap();

        assert_eq!(shortfalls.last(), Some(Shortfall::Missing), "{url}");
    }
}

#[test]
fn the_configured_helpers_have_nothing_when_git_config_names_none() {
    let dir = TempDir::new().unwrap();
    let shortfalls = Shortfalls::default();
    let mut credentials =
        never_prompting(configured(isolated_repository(&dir)), shortfalls.clone());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn a_report_on_an_answer_nobody_gave_is_ignored() {
    let dir = TempDir::new().unwrap();
    let mut helpers = configured(isolated_repository(&dir));

    let answer = helpers(
        Action::Erase("url=https://example.com\n".into()),
        prompt::Options {
            askpass: None,
            mode: prompt::Mode::Disable,
        },
    );

    assert!(matches!(answer, Ok(None)), "{answer:?}");
}
