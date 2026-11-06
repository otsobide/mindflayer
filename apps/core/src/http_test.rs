//! Unit tests for `http.rs`: the rules a URL and a request are held to,
//! which need no transport. What a client does with an answer is tested
//! through a fake in `tests/http_test.rs`.

use super::*;

#[test]
fn an_https_url_is_fetchable() {
    let parsed = fetchable("https://registry.example/v0/servers?limit=100").unwrap();

    assert_eq!(parsed.host_str(), Some("registry.example"));
}

/// This machine is where a test's server and a catalog of one's own are,
/// and the way there is nobody else's.
#[test]
fn plain_http_is_fetchable_only_on_this_machine() {
    for url in [
        "http://127.0.0.1:8080/catalog.json",
        "http://127.1/catalog.json",
        "http://[::1]:9000/catalog.json",
        "http://localhost/catalog.json",
        "http://LOCALHOST:80/catalog.json",
    ] {
        assert!(fetchable(url).is_ok(), "{url}");
    }

    for url in [
        "http://registry.example/v0/servers",
        "http://10.0.0.1/catalog.json",
        "http://localhost.example/catalog.json",
        "http://127.0.0.1.example/catalog.json",
    ] {
        let error = fetchable(url).unwrap_err();
        assert_eq!(error.code(), "http_url_refused", "{url}");
    }
}

#[test]
fn a_scheme_other_than_http_is_refused() {
    for url in [
        "ftp://127.0.0.1/catalog.json",
        "file:///etc/passwd",
        "git://example.com/x",
    ] {
        assert_eq!(
            fetchable(url).unwrap_err().code(),
            "http_url_refused",
            "{url}"
        );
    }
}

#[test]
fn what_is_not_a_url_is_said_so() {
    let error = fetchable("registry.example/v0/servers").unwrap_err();

    assert_eq!(error.code(), "http_url_invalid");
}

/// Refused rather than sent and dropped from what is shown, and the error
/// names the URL without the credential it refuses.
#[test]
fn a_url_carrying_credentials_is_refused_without_repeating_them() {
    for url in [
        "https://me:s3cret@registry.example/v0/servers",
        "https://ghp_token@registry.example/v0/servers",
    ] {
        let error = fetchable(url).unwrap_err();

        assert_eq!(error.code(), "http_credentials_in_url", "{url}");
        let said = error.to_string();
        assert!(
            !said.contains("s3cret") && !said.contains("ghp_token"),
            "{said}"
        );
    }
}

#[test]
fn an_authorization_header_is_a_credential_however_it_is_added() {
    let request = Request::get("https://api.example/search")
        .header("Accept", "application/json")
        .header("authorization", "Bearer ghp_token")
        .credential("X-Api-Key", "k3y");

    let credentials: Vec<&str> = request
        .headers()
        .iter()
        .filter(|header| header.is_credential())
        .map(Header::name)
        .collect();
    assert_eq!(credentials, ["authorization", "X-Api-Key"]);
    assert!(request.carries_credential());
    assert!(!Request::get("https://api.example/")
        .header("Accept", "application/json")
        .carries_credential());
}

/// What a request is debugged as goes into logs, so a credential is never
/// in it, nor one a URL carries.
#[test]
fn a_request_is_debugged_without_its_credentials() {
    let request = Request::get("https://me:s3cret@api.example/search")
        .header("Authorization", "Bearer ghp_token")
        .credential("X-Api-Key", "k3y")
        .header("Accept", "application/json");

    let shown = format!("{request:?}");

    assert!(!shown.contains("s3cret"), "{shown}");
    assert!(!shown.contains("ghp_token"), "{shown}");
    assert!(!shown.contains("k3y"), "{shown}");
    assert!(shown.contains("Accept: application/json"), "{shown}");
}

#[test]
fn a_header_given_twice_is_sent_once_with_the_last_value() {
    let request = Request::get("https://api.example/")
        .header("Accept", "text/plain")
        .header("accept", "application/json");

    assert_eq!(request.headers().len(), 1);
    assert_eq!(request.header_value("ACCEPT"), Some("application/json"));
}

#[test]
fn an_answers_headers_are_read_whatever_their_case() {
    let headers = Headers::new()
        .with("Content-Type", "application/json")
        .with("ETag", "\"v1\"");

    assert_eq!(headers.get("content-type"), Some("application/json"));
    assert_eq!(headers.get("etag"), Some("\"v1\""));
    assert_eq!(headers.get("link"), None);
}

#[test]
fn the_user_agent_names_mindweaver_and_its_version() {
    assert_eq!(
        USER_AGENT,
        format!("mindweaver/{}", env!("CARGO_PKG_VERSION"))
    );
}
