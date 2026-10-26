//! The panel: its files, `index.html` for the routes it draws itself, and a
//! page that says how to build it when this build has none.

mod common;

use common::*;
use mindflayer_server::Panel;
use rust_embed::RustEmbed;

/// A panel as `make web/build` would leave one, small enough to read.
#[derive(RustEmbed)]
#[folder = "tests/panel"]
struct Built;

/// A build of the server where nothing built the panel.
#[derive(RustEmbed)]
#[folder = "tests/no-panel-here"]
#[allow_missing = true]
struct Unbuilt;

fn serving(panel: Panel) -> Listening {
    Server::new(Fake::default()).panel(panel).bind().unwrap()
}

const INDEX: &str = include_str!("panel/index.html");

#[tokio::test]
async fn a_client_route_serves_index_html_without_a_token() {
    let server = serving(Panel::embedded::<Built>());

    for path in [
        "/",
        "/projects/beta",
        "/projects/beta/",
        "/skills/a:b",
        "/skills?kind=rule",
    ] {
        let reply = get(&server, path, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
        assert_eq!(reply.header("content-type"), Some("text/html"));
        assert_eq!(reply.header("cache-control"), Some("no-cache"));
        assert_eq!(reply.text(), INDEX);
    }
}

#[tokio::test]
async fn a_hashed_asset_is_served_as_immutable() {
    let server = serving(Panel::embedded::<Built>());

    let reply = get(&server, "/assets/index-4f2a9c1b.js", Auth::Nothing).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-type"), Some("text/javascript"));
    assert_eq!(
        reply.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert!(reply.text().contains("a panel built for the tests"));
}

#[tokio::test]
async fn a_file_outside_the_hashed_assets_is_asked_about_again() {
    let server = serving(Panel::embedded::<Built>());

    let reply = get(&server, "/robots.txt", Auth::Nothing).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-type"), Some("text/plain"));
    assert_eq!(reply.header("cache-control"), Some("no-cache"));
}

#[tokio::test]
async fn a_missing_hashed_asset_is_a_404_rather_than_the_page() {
    let server = serving(Panel::embedded::<Built>());

    let reply = get(&server, "/assets/index-00000000.js", Auth::Nothing).await;

    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_ne!(reply.text(), INDEX);
}

#[tokio::test]
async fn a_path_that_climbs_names_nothing() {
    let server = serving(Panel::embedded::<Built>());

    for path in [
        "/assets/../index.html",
        "/assets/..\\index.html",
        "/./robots.txt",
    ] {
        let reply = get(&server, path, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
    }
}

/// A debug build's embed reads the panel's folder from disk, where an
/// absolute path replaces the folder and a symlink is followed wherever it
/// leads.
#[cfg(unix)]
#[tokio::test]
async fn a_symlink_outside_the_panel_is_never_read() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret"), "not the panel's").unwrap();
    let link = outside.path().join("link");
    std::os::unix::fs::symlink(outside.path().join("secret"), &link).unwrap();
    let link = link.to_str().unwrap().replace('/', "\\");
    let server = serving(Panel::embedded::<Built>());

    for path in [
        format!("/{link}"),
        format!("/assets{}{link}", "\\..".repeat(64)),
    ] {
        let reply = get(&server, &path, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert!(!reply.text().contains("not the panel's"), "{path}");
    }
}

#[tokio::test]
async fn an_unknown_api_path_gets_json_and_not_the_page() {
    let server = serving(Panel::embedded::<Built>());

    let reply = get(&server, "/api/projects", Auth::Bearer(server.token())).await;

    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(
        reply.json(),
        json!({ "error": "no such endpoint: GET /api/projects" })
    );
}

#[tokio::test]
async fn the_panel_answers_only_get() {
    let server = serving(Panel::embedded::<Built>());

    let reply = send(&server, Method::POST, "/projects", Auth::Nothing, None).await;

    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(reply.header("allow"), Some("GET, HEAD"));
}

#[tokio::test]
async fn a_build_without_the_panel_says_how_to_build_it() {
    let server = serving(Panel::embedded::<Unbuilt>());

    for path in ["/", "/projects/beta"] {
        let reply = get(&server, path, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(
            reply.header("content-type"),
            Some("text/html; charset=utf-8")
        );
        assert!(reply
            .text()
            .contains("panel not built: run <code>make web/build</code>"));
    }

    // The API does not need the panel.
    let reply = get(&server, "/api/transcript", Auth::Bearer(server.token())).await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn the_launch_link_still_trades_its_token_with_a_panel_at_the_root() {
    let server = serving(Panel::embedded::<Built>());

    let reply = get(
        &server,
        &format!("/?token={}", server.token()),
        Auth::Nothing,
    )
    .await;

    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.header("location"), Some("/"));
}
