//! Unit tests for `paging.rs`: reading where a next page is, which needs no
//! transport. A listing read through a fake is `tests/http_test.rs`'s.

use super::*;

#[test]
fn the_next_link_is_found_among_others() {
    let github = concat!(
        r#"<https://api.github.com/search/code?q=skill&page=2>; rel="next", "#,
        r#"<https://api.github.com/search/code?q=skill&page=34>; rel="last""#,
    );

    assert_eq!(
        next_in(github),
        Some("https://api.github.com/search/code?q=skill&page=2")
    );
}

#[test]
fn a_next_link_is_found_wherever_it_stands_and_however_rel_is_written() {
    for (value, next) in [
        (r#"<a>; rel="prev", <b>; rel="next""#, Some("b")),
        ("<a>; rel=next", Some("a")),
        (r#"<a>; title="x"; REL="Next""#, Some("a")),
        (r#"<a>; rel="next last""#, Some("a")),
        (r#"<a>; rel="prev", <b>; rel="last""#, None),
        (r#"<a>; rel="nextpage""#, None),
        ("", None),
        ("<unclosed; rel=next", None),
    ] {
        assert_eq!(next_in(value), next, "{value}");
    }
}

#[test]
fn a_page_number_is_one_more_than_the_urls_own() {
    let next = numbered(
        "https://catalog.example/v0/servers?limit=100&page=4",
        "page",
    )
    .unwrap();

    assert_eq!(
        next.as_str(),
        "https://catalog.example/v0/servers?limit=100&page=5"
    );
}

#[test]
fn a_url_with_no_page_number_was_the_first_page() {
    let next = numbered("https://catalog.example/v0/servers?limit=100", "page").unwrap();

    assert_eq!(
        next.as_str(),
        "https://catalog.example/v0/servers?limit=100&page=2"
    );
}

#[test]
fn a_page_number_that_is_no_number_is_said_so() {
    let error = numbered("https://catalog.example/v0/servers?page=last", "page").unwrap_err();

    assert_eq!(error.code(), "http_url_invalid");
    assert_eq!(
        error.to_string(),
        "https://catalog.example/v0/servers?page=last: not a URL that can be fetched: `page` \
         is not a page number"
    );
}

#[test]
fn a_cursor_is_asked_for_on_the_first_pages_url_in_place_of_any_it_had() {
    let next = at_cursor(
        "https://registry.example/v0.1/servers?search=git&cursor=old&limit=100",
        "cursor",
        "io.github.acme/b:1.0.0",
    )
    .unwrap();

    assert_eq!(
        next.as_str(),
        "https://registry.example/v0.1/servers?search=git&limit=100&cursor=io.github.acme%2Fb%3A1.0.0"
    );
}

/// What a page says is somebody else's text: put in as a value, it adds no
/// parameter of its own.
#[test]
fn a_cursor_is_escaped_as_the_value_it_is() {
    let next = at_cursor(
        "https://registry.example/v0.1/servers?search=git",
        "cursor",
        "x&search=other#top",
    )
    .unwrap();

    let pairs: Vec<(String, String)> = next.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [
            (String::from("search"), String::from("git")),
            (String::from("cursor"), String::from("x&search=other#top")),
        ]
    );
    assert_eq!(next.fragment(), None);
}
