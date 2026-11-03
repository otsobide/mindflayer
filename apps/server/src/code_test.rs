use super::*;

/// A code reaches a client without the place that found it, so two
/// conditions given one code are two a script cannot tell apart. The
/// CLI holds them apart from core's and its own as well (`error_test.rs`).
#[test]
fn no_code_the_server_gives_is_given_twice() {
    let twice: Vec<&str> = Condition::CODES
        .iter()
        .enumerate()
        .filter(|(at, code)| Condition::CODES[..*at].contains(code))
        .map(|(_, code)| *code)
        .collect();

    assert!(twice.is_empty(), "codes given twice: {twice:?}");
}

#[test]
fn every_code_the_server_gives_is_snake_case() {
    let unfit: Vec<&str> = Condition::CODES
        .iter()
        .copied()
        .filter(|code| {
            !code.split('_').all(|word| {
                !word.is_empty()
                    && word
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        })
        .collect();

    assert!(unfit.is_empty(), "codes not in snake_case: {unfit:?}");
}

/// What a description of the API lists of each condition, its code and its
/// status, read from the one list, in the one order.
#[test]
fn every_condition_is_listed_once_with_its_code_beside_it() {
    let listed: Vec<&str> = Condition::ALL
        .iter()
        .map(|condition| condition.code())
        .collect();

    assert_eq!(listed, Condition::CODES);
    assert_eq!(Condition::TokenRequired.code(), "token_required");
    assert_eq!(Condition::UnknownRoute.code(), "unknown_route");
}

/// None is a success, nor a redirect: each is an answer a client is to
/// read as what went wrong.
#[test]
fn every_condition_is_answered_with_an_error_status() {
    for condition in Condition::ALL {
        let status = condition.status();
        assert!(
            status.is_client_error() || status.is_server_error(),
            "{condition:?}: {status}"
        );
    }
    assert_eq!(Condition::TokenRequired.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        Condition::JsonRequired.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        Condition::InvalidBody.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        Condition::WriterStopped.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
