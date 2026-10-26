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
