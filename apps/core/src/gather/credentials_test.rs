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

#[test]
fn a_credential_no_helper_has_is_an_error_and_never_a_prompt() {
    let mut credentials = never_prompting(no_helper_and_a_prompt_that_panics());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
}

#[test]
fn the_configured_helpers_have_nothing_when_git_config_names_none() {
    let dir = TempDir::new().unwrap();
    let mut credentials = never_prompting(configured(isolated_repository(&dir)));

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
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
