use super::*;

#[test]
fn the_token_parameter_is_redacted_and_the_rest_of_the_query_kept() {
    assert_eq!(redact("token=abc"), "token=<redacted>");
    assert_eq!(
        redact("view=skills&token=abc&sort=name"),
        "view=skills&token=<redacted>&sort=name"
    );
    assert_eq!(
        redact("token=a&token=b"),
        "token=<redacted>&token=<redacted>"
    );
    assert_eq!(redact("tokens=abc&a_token=b"), "tokens=abc&a_token=b");
}

#[test]
fn an_encoded_name_is_redacted_as_the_exchange_would_read_it() {
    assert_eq!(redact("%74oken=abc"), "%74oken=<redacted>");
    assert_eq!(redact("%74%6F%6B%65%6E=abc"), "%74%6F%6B%65%6E=<redacted>");
}

#[test]
fn a_bare_token_parameter_is_still_redacted() {
    assert_eq!(redact("token"), "token=<redacted>");
}

#[test]
fn the_token_is_scrubbed_wherever_else_it_turns_up() {
    let token = "0123456789abcdef";
    assert_eq!(
        scrub("GET /0123456789abcdef/x?t=0123456789ABCDEF 404", token),
        "GET /<redacted>/x?t=<redacted> 404"
    );
    assert_eq!(
        scrub("GET /api/transcript 200", token),
        "GET /api/transcript 200"
    );
}
