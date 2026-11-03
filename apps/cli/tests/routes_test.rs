//! The reads the panel's backend serves under `/api/v1`, as the table it
//! hands the server: each route pinned by its method, path, parameters and
//! answer, which are a contract once served (D-24), and each making of what
//! a request sends the very query the panel sends for that read. No port is
//! bound: `ui_test` calls every route over loopback, and the server's own
//! suites cover reading a query string by a table.

// The backend is the panel's, which a build without the `ui` feature lacks.
#![cfg(feature = "ui")]

use mindflayer_cli::backend::{PanelBackend, Query};
use mindflayer_server::{Arguments, Backend, Method, Parameter, Route, Shape};
use serde_json::{json, Map, Value};

/// The route served at `path`.
fn route(path: &str) -> Route<Query> {
    PanelBackend::routes()
        .into_iter()
        .find(|route| route.path == path)
        .unwrap_or_else(|| panic!("no route {path}"))
}

/// The query the route at `path` makes of `sent`, as the server hands it
/// what a request sent.
fn routed(path: &str, sent: Value) -> Result<Query, String> {
    (route(path).query)(Arguments::new(sent))
}

/// What the panel sends for a read, read as the backend reads it.
fn query(sent: Value) -> Query {
    serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
}

/// A parameter as the table below spells it: its name, `?` when it may be
/// left out, and its shape.
fn spelled(parameter: &Parameter) -> String {
    let optional = if parameter.required { "" } else { "?" };
    format!("{}{optional}: {:?}", parameter.name, parameter.shape)
}

/// Every route, as a description of the API would list it: so one renamed,
/// dropped or holding something else fails here, and moves the API to
/// `/api/v2` rather than reach a script that was written against it.
#[test]
fn every_route_is_pinned_by_its_method_path_parameters_body_and_answer() {
    let table: Vec<Value> = PanelBackend::routes()
        .iter()
        .map(|route| {
            json!({
                "route": format!("{} {}", route.method, route.path),
                "parameters": route.parameters.iter().map(spelled).collect::<Vec<_>>(),
                "body": route.body,
                "answers": route.answers,
            })
        })
        .collect();

    let none: [&str; 0] = [];
    assert_eq!(
        table,
        [
            json!({"route": "GET /snapshot", "parameters": none, "body": null, "answers": "Snapshot"}),
            json!({"route": "GET /projects", "parameters": none, "body": null, "answers": "Project[]"}),
            json!({"route": "GET /project", "parameters": ["entry: Text"], "body": null, "answers": "Project"}),
            json!({"route": "GET /library", "parameters": none, "body": null, "answers": "Library"}),
            json!({
                "route": "GET /files", "parameters": ["reference: Text", "project?: Text"],
                "body": null, "answers": "Files",
            }),
            json!({
                "route": "GET /contents",
                "parameters": ["reference: Text", "file: Text", "project?: Text"],
                "body": null, "answers": "Contents",
            }),
            json!({
                "route": "GET /scan", "parameters": ["root?: List", "depth?: Number"],
                "body": null, "answers": "Scanned",
            }),
            json!({
                "route": "GET /folders", "parameters": ["path?: Text", "hidden?: Flag"],
                "body": null, "answers": "Folders",
            }),
            json!({"route": "GET /updates", "parameters": none, "body": null, "answers": "Updates"}),
            json!({"route": "GET /workspaces", "parameters": none, "body": null, "answers": "Switchable[]"}),
            json!({"route": "POST /plan", "parameters": none, "body": "Selected", "answers": "Planned"}),
            json!({"route": "POST /validate", "parameters": none, "body": "Drafted", "answers": "Judged"}),
        ]
    );
    for route in PanelBackend::routes() {
        assert!(!route.about.is_empty(), "{}", route.path);
        for parameter in &route.parameters {
            assert!(
                !parameter.about.is_empty(),
                "{} {}",
                route.path,
                parameter.name
            );
        }
    }
}

/// Each route makes of what a request sends the query the panel sends for
/// the same read: a project named by its entry in `project`, and the
/// workspace's own by none; roots one `root` each; a parameter left out
/// read as the panel's query leaves it.
#[test]
fn each_route_makes_the_query_the_panel_sends_for_the_same_read() {
    let workspace = json!({"type": "workspace"});
    let alpha = json!({"type": "project", "entry": "alpha"});
    let cell = json!({"project": "alpha", "reference": "skill:deploy", "origin": "workspace", "wanted": true});
    let draft = json!({"type": "edit", "reference": "skill:deploy", "text": "---\n"});

    for (path, sent, asked) in [
        ("/snapshot", json!({}), json!({"type": "snapshot"})),
        ("/projects", json!({}), json!({"type": "projects"})),
        (
            "/project",
            json!({"entry": "a/b"}),
            json!({"type": "project", "entry": "a/b"}),
        ),
        ("/library", json!({}), json!({"type": "library"})),
        (
            "/files",
            json!({"reference": "skill:deploy"}),
            json!({"type": "files", "holder": workspace, "reference": "skill:deploy"}),
        ),
        (
            "/files",
            json!({"reference": "skill:deploy", "project": "alpha"}),
            json!({"type": "files", "holder": alpha, "reference": "skill:deploy"}),
        ),
        (
            "/contents",
            json!({"reference": "rule:git/commits", "file": "commits.md"}),
            json!({
                "type": "contents", "holder": workspace, "reference": "rule:git/commits",
                "file": "commits.md",
            }),
        ),
        (
            "/contents",
            json!({"reference": "skill:deploy", "file": "SKILL.md", "project": "alpha"}),
            json!({
                "type": "contents", "holder": alpha, "reference": "skill:deploy",
                "file": "SKILL.md",
            }),
        ),
        (
            "/scan",
            json!({}),
            json!({"type": "scan", "roots": [], "depth": null}),
        ),
        (
            "/scan",
            json!({"root": ["/home/you/Work", "/srv"], "depth": 2}),
            json!({"type": "scan", "roots": ["/home/you/Work", "/srv"], "depth": 2}),
        ),
        (
            "/folders",
            json!({}),
            json!({"type": "folders", "path": null, "hidden": false}),
        ),
        (
            "/folders",
            json!({"path": "/home/you", "hidden": true}),
            json!({"type": "folders", "path": "/home/you", "hidden": true}),
        ),
        ("/updates", json!({}), json!({"type": "updates"})),
        ("/workspaces", json!({}), json!({"type": "workspaces"})),
        (
            "/plan",
            json!({"selection": [cell]}),
            json!({"type": "plan", "selection": [cell]}),
        ),
        (
            "/validate",
            json!({"holder": alpha, "draft": draft}),
            json!({"type": "validate", "holder": alpha, "draft": draft}),
        ),
    ] {
        assert_eq!(
            routed(path, sent.clone()),
            Ok(query(asked)),
            "{path} {sent}"
        );
    }
}

/// What the server has read a request's parameters into, each at a value
/// of its shape: every one the table names, or only those it needs. A
/// route that does not read one it names, or needs one it says may be left
/// out, fails here rather than on a request.
#[test]
fn each_route_reads_every_parameter_its_table_names_and_needs_no_other() {
    let sample = |parameter: &Parameter| match parameter.shape {
        Shape::Text => json!("x"),
        Shape::Number => json!(1),
        Shape::Flag => json!(true),
        Shape::List => json!(["x"]),
    };
    for route in PanelBackend::routes() {
        if route.method == Method::Post {
            continue;
        }
        let read = |needed: bool| -> Map<String, Value> {
            route
                .parameters
                .iter()
                .filter(|parameter| parameter.required || !needed)
                .map(|parameter| (parameter.name.to_owned(), sample(parameter)))
                .collect()
        };
        for sent in [read(false), read(true)] {
            let made = (route.query)(Arguments::new(Value::Object(sent.clone())));
            assert!(made.is_ok(), "{} {sent:?}: {made:?}", route.path);
        }
    }
}

/// A body is read as a task's is: a field its route does not name refused,
/// the query's own `type` among them, and one it needs left out refused.
#[test]
fn a_body_holding_what_its_route_does_not_read_is_refused() {
    let cell = json!({"project": "alpha", "reference": "skill:deploy", "origin": "workspace", "wanted": true});
    for (path, sent) in [
        ("/plan", json!({"type": "plan", "selection": [cell]})),
        ("/plan", json!({"selection": [cell], "home": "/etc"})),
        ("/plan", json!({})),
        ("/plan", json!([cell])),
        ("/validate", json!({"holder": {"type": "workspace"}})),
        (
            "/validate",
            json!({"holder": {"type": "workspace"}, "draft": {"type": "edit", "reference": "x", "text": ""}, "raw": true}),
        ),
    ] {
        assert!(routed(path, sent.clone()).is_err(), "{path} {sent}");
    }
}
