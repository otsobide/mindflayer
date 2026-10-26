use super::*;

#[test]
fn a_token_is_256_bits_of_hex() {
    let token = Token::issue();
    assert_eq!(token.as_str().len(), 64);
    assert!(token.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn every_launch_gets_its_own_token() {
    assert_ne!(Token::issue().as_str(), Token::issue().as_str());
}

#[test]
fn a_token_matches_only_itself() {
    let token = Token::issue();
    let copy = token.as_str().to_owned();
    assert!(token.matches(&copy));

    let mut wrong = copy.clone().into_bytes();
    wrong[63] = if wrong[63] == b'0' { b'1' } else { b'0' };
    assert!(!token.matches(std::str::from_utf8(&wrong).unwrap()));
    assert!(!token.matches(&copy[..63]));
    assert!(!token.matches(&format!("{copy}0")));
    assert!(!token.matches(""));
}

#[test]
fn debugging_a_token_does_not_print_it() {
    let token = Token::issue();
    let shown = format!("{token:?}");
    assert!(!shown.contains(token.as_str()), "{shown}");
}
