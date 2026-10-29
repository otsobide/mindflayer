use super::*;

use axum::http::HeaderValue;

/// Headers holding one Host, or none for `None`.
fn host(host: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(host) = host {
        headers.insert(HOST, HeaderValue::from_str(host).unwrap());
    }
    headers
}

#[test]
fn the_server_answers_to_loopbacks_three_names_on_its_port() {
    let hosts = Hosts::new(4000);

    for name in ["127.0.0.1:4000", "localhost:4000", "[::1]:4000"] {
        assert_eq!(hosts.named(&host(Some(name))), Some(name));
    }
}

#[test]
fn a_name_is_the_same_name_in_any_case() {
    let hosts = Hosts::new(4000);

    assert_eq!(
        hosts.named(&host(Some("LocalHost:4000"))),
        Some("localhost:4000")
    );
}

#[test]
fn any_other_name_or_port_is_not_the_servers() {
    let hosts = Hosts::new(4000);

    for other in [
        "attacker.example:4000",
        "127.0.0.1:4001",
        "127.0.0.1:40000",
        "127.0.0.1",
        "localhost",
        "localhost.:4000",
        "127.0.0.2:4000",
        "0.0.0.0:4000",
        "[::ffff:127.0.0.1]:4000",
        "127.0.0.1:4000.attacker.example",
        "",
    ] {
        assert_eq!(hosts.named(&host(Some(other))), None, "{other:?}");
    }
    assert_eq!(hosts.named(&host(None)), None);
}

#[test]
fn a_request_naming_two_hosts_names_neither() {
    let hosts = Hosts::new(4000);
    let mut headers = host(Some("127.0.0.1:4000"));
    headers.append(HOST, HeaderValue::from_static("127.0.0.1:4000"));

    assert_eq!(hosts.named(&headers), None);
}

#[test]
fn a_refusal_lists_the_names_the_server_answers_as() {
    assert_eq!(
        Hosts::new(4000).listed(),
        "127.0.0.1:4000, localhost:4000 or [::1]:4000"
    );
}

/// Headers holding these Origins.
fn origins(origins: &[&str]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for origin in origins {
        headers.append(ORIGIN, HeaderValue::from_str(origin).unwrap());
    }
    headers
}

#[test]
fn a_request_without_an_origin_is_not_held_to_one() {
    assert!(from_itself("127.0.0.1:4000", &origins(&[])));
}

#[test]
fn an_origin_must_be_the_one_the_request_was_sent_to() {
    assert!(from_itself(
        "127.0.0.1:4000",
        &origins(&["http://127.0.0.1:4000"])
    ));
    assert!(from_itself("[::1]:4000", &origins(&["http://[::1]:4000"])));
    assert!(from_itself(
        "localhost:4000",
        &origins(&["HTTP://LOCALHOST:4000"])
    ));

    for other in [
        "https://attacker.example",
        "http://attacker.example:4000",
        "http://127.0.0.1:4001",
        "http://localhost:4000",
        "https://127.0.0.1:4000",
        "http://127.0.0.1:4000/",
        "127.0.0.1:4000",
        "null",
        "",
    ] {
        assert!(
            !from_itself("127.0.0.1:4000", &origins(&[other])),
            "{other:?}"
        );
    }
}

#[test]
fn a_request_with_two_origins_is_from_neither() {
    let both = origins(&["http://127.0.0.1:4000", "http://127.0.0.1:4000"]);

    assert!(!from_itself("127.0.0.1:4000", &both));
}

/// The dev server's origin, exactly, as the front end named it; and the
/// server's own still, which the panel's built page is served from.
#[test]
fn the_dev_servers_origin_is_taken_when_named_and_nothing_like_it() {
    let hosts = Hosts::new(4000).dev_server(Some(String::from("http://127.0.0.1:5173")));

    for origin in [
        "http://127.0.0.1:5173",
        "HTTP://127.0.0.1:5173",
        "http://127.0.0.1:4000",
    ] {
        assert!(
            hosts.sent_from("127.0.0.1:4000", &origins(&[origin])),
            "{origin:?}"
        );
    }
    assert!(hosts.sent_from("127.0.0.1:4000", &origins(&[])));

    for other in [
        "http://127.0.0.1:5174",
        "http://localhost:5173",
        "https://127.0.0.1:5173",
        "http://127.0.0.1:5173/",
        "http://127.0.0.1:51730",
        "null",
        "",
    ] {
        assert!(
            !hosts.sent_from("127.0.0.1:4000", &origins(&[other])),
            "{other:?}"
        );
    }
    let twice = origins(&["http://127.0.0.1:5173", "http://127.0.0.1:5173"]);
    assert!(!hosts.sent_from("127.0.0.1:4000", &twice));
}

#[test]
fn without_a_dev_server_its_origin_is_any_other_page() {
    let hosts = Hosts::new(4000);

    assert!(!hosts.sent_from("127.0.0.1:4000", &origins(&["http://127.0.0.1:5173"])));
    assert_eq!(hosts.pages("127.0.0.1:4000"), "http://127.0.0.1:4000");
    assert_eq!(
        hosts
            .dev_server(Some(String::from("http://127.0.0.1:5173")))
            .pages("127.0.0.1:4000"),
        "http://127.0.0.1:4000 or http://127.0.0.1:5173"
    );
}

/// The headers the panel's page sends with a change, each of `changes` set
/// to its value or, for `None`, taken out.
fn panel(changes: &[(&'static str, Option<&str>)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    headers.insert("x-mindflayer", HeaderValue::from_static("1"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    for (name, value) in changes {
        match value {
            Some(value) => headers.insert(*name, HeaderValue::from_str(value).unwrap()),
            None => headers.remove(*name),
        };
    }
    headers
}

#[test]
fn a_change_from_the_panels_own_page_is_taken() {
    assert!(from_the_panel(&Method::POST, &panel(&[])).is_ok());
    let charset = panel(&[("content-type", Some("application/json; charset=utf-8"))]);
    assert!(from_the_panel(&Method::POST, &charset).is_ok());
}

#[test]
fn a_change_from_any_other_page_or_from_no_browser_is_not() {
    for site in [
        Some("same-site"),
        Some("cross-site"),
        Some("none"),
        Some(""),
        None,
    ] {
        for method in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH] {
            let headers = panel(&[("sec-fetch-site", site)]);
            let (status, _) = from_the_panel(&method, &headers).unwrap_err();
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {site:?}");
        }
    }

    let mut twice = panel(&[]);
    twice.append("sec-fetch-site", HeaderValue::from_static("same-origin"));
    assert!(from_the_panel(&Method::POST, &twice).is_err());
}

#[test]
fn a_change_without_the_panels_own_header_is_refused() {
    for value in [None, Some("0"), Some("true"), Some("")] {
        let headers = panel(&[("x-mindflayer", value)]);
        let (status, message) = from_the_panel(&Method::POST, &headers).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN, "{value:?}");
        assert!(message.contains("X-Mindflayer: 1"), "{message}");
    }
}

#[test]
fn a_change_that_is_not_sent_as_json_is_refused() {
    for value in [
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        Some("multipart/form-data; boundary=x"),
        Some("application/jsonp"),
        None,
    ] {
        let headers = panel(&[("content-type", value)]);
        let (status, _) = from_the_panel(&Method::POST, &headers).unwrap_err();
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{value:?}");
    }
}

#[test]
fn a_read_is_not_held_to_where_it_came_from() {
    for method in [Method::GET, Method::HEAD] {
        assert!(from_the_panel(&method, &HeaderMap::new()).is_ok());
        let elsewhere = panel(&[("sec-fetch-site", Some("same-site"))]);
        assert!(from_the_panel(&method, &elsewhere).is_ok());
    }
}
