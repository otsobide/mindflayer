//! Who is let in: the guide's "The token" and "Versions", and what keeps
//! other pages out (`docs/architecture.md`, "What keeps other pages out").

use serde_json::json;

use crate::{entries, guide_workspace, Ask, Scenario};

/// A task that would link `web`: a change, which is what the checks a
/// script never meets are about.
fn link_web() -> serde_json::Value {
    json!({"type": "link", "path": "/home/you/Projects/web"})
}

#[test]
fn the_link_trades_its_token_for_a_cookie_and_sends_the_browser_to_the_panel() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");
    let link = format!("/?token={}", served.token);

    let answer = served.send(Ask::at("GET", &link).without_token());

    answer.is(303);
    assert_eq!(answer.header("location"), Some("/"));
    assert_eq!(
        answer.header("set-cookie"),
        Some(&*format!(
            "mw_{}={}; Path=/; HttpOnly; SameSite=Strict",
            served.address.port(),
            served.token
        ))
    );
    assert_eq!(answer.header("cache-control"), Some("no-store"));
}

#[test]
fn the_cookie_the_link_set_lets_the_panels_page_read() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let answer = served.send(Ask::get("/projects").as_the_page());

    answer.is(200);
    assert_eq!(entries(&answer), ["api"]);
}

#[test]
fn the_panels_page_runs_a_change_with_its_cookie_as_a_script_does_with_the_token() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .send(Ask::post("/tasks", link_web()).as_the_page())
        .is(200)
        .answers_at("/outcome/runs/0/stdout", json!("linked web as web\n"));
    assert_eq!(entries(&served.get("/projects")), ["api", "web"]);
}

#[test]
fn a_link_with_the_token_of_a_launch_since_stopped_opens_nothing() {
    let scenario = Scenario::projects(&["api"]);
    let before = scenario.serve("~/Projects");
    let old = before.token.clone();
    before.stop();
    let served = scenario.serve("~/Projects");

    let answer = served.send(Ask::at("GET", &format!("/?token={old}")).without_token());

    answer.is(401);
    assert_eq!(
        answer.body,
        "This link's token is not this server's: open the link printed when it started.\n"
    );
    assert_eq!(answer.header("set-cookie"), None);
}

#[test]
fn a_request_without_the_token_is_refused() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served.send(Ask::get("/snapshot").without_token()).refused(
        401,
        "token_required",
        "this needs the launch token, as the cookie its link sets or as a Bearer token",
    );
}

#[test]
fn a_token_of_a_launch_since_stopped_opens_nothing() {
    let scenario = Scenario::projects(&["api"]);
    let before = scenario.serve("~/Projects");
    let old = before.token.clone();
    before.stop();
    let served = scenario.serve("~/Projects");

    served
        .send(
            Ask::get("/snapshot")
                .without_token()
                .header(&format!("Authorization: Bearer {old}")),
        )
        .refused(
            401,
            "token_required",
            "this needs the launch token, as the cookie its link sets or as a Bearer token",
        );
}

#[test]
fn a_page_of_another_site_naming_the_server_by_its_own_host_is_refused() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");
    let port = served.address.port();

    // What a page of evil.example reaches once its name is made to lead to
    // 127.0.0.1: the server, named as evil.example, with the token or not.
    served
        .send(Ask::get("/projects").header("Host: evil.example:{port}"))
        .refused(
            403,
            "unknown_host",
            &format!(
                "this server answers only as 127.0.0.1:{port}, localhost:{port} or [::1]:{port}"
            ),
        );
}

#[test]
fn the_server_answers_when_named_as_localhost() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let answer = served.send(Ask::get("/projects").header("Host: localhost:{port}"));

    answer.is(200);
    assert_eq!(entries(&answer), ["api"]);
}

#[test]
fn a_change_sent_with_the_cookie_from_another_site_is_refused_and_runs_nothing() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    let address = served.address;

    served
        .send(
            Ask::post("/tasks", link_web())
                .as_the_page()
                .header("Origin: http://evil.example"),
        )
        .refused(
            403,
            "foreign_origin",
            &format!("this server takes requests only from its own pages, at http://{address}"),
        );
    assert_eq!(entries(&served.get("/projects")), ["api"]);
}

#[test]
fn a_change_sent_with_the_cookie_from_no_page_of_the_servers_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .send(
            Ask::post("/tasks", link_web())
                .as_the_page()
                .header("Sec-Fetch-Site: cross-site"),
        )
        .refused(
            403,
            "same_origin_required",
            "a change sent with the cookie must come from the panel's own page \
             (Sec-Fetch-Site: same-origin); a script sends the token as Bearer instead",
        );
    assert_eq!(entries(&served.get("/projects")), ["api"]);
}

#[test]
fn a_change_sent_with_the_cookie_without_the_panels_header_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .send(
            Ask::post("/tasks", link_web())
                .as_the_page()
                .without("X-Mindweaver"),
        )
        .refused(
            403,
            "x_mindflayer_required",
            "a change sent with the cookie must carry X-Mindweaver: 1, as the panel's do",
        );
    assert_eq!(entries(&served.get("/projects")), ["api"]);
}

#[test]
fn a_version_the_server_does_not_serve_is_no_route() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served.send(Ask::at("GET", "/api/v2/snapshot")).refused(
        404,
        "unknown_route",
        "no such endpoint: GET /api/v2/snapshot",
    );
}

#[test]
fn an_answer_is_kept_by_no_browser_and_runs_nothing_it_holds() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let answer = served.get("/projects");

    answer.is(200);
    assert_eq!(answer.header("cache-control"), Some("no-store"));
    assert_eq!(answer.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        answer.header("content-security-policy"),
        Some("default-src 'self'; frame-ancestors 'none'")
    );
}
