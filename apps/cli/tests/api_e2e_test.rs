//! The server end to end: the HTTP API `mindweaver ui` serves, driven one use
//! case at a time, as a script on this machine drives it.
//!
//! The server (`apps/server`) is a library with no binary of its own: what
//! a person runs is `mindweaver ui`, which serves it over the CLI's backend, so
//! its use cases are run here, beside the binary. The other suites that
//! reach it stop short of a use case: the server's own run against a fake
//! backend, `backend_test` drives the CLI's backend with no port bound,
//! `ui_test` checks every route and guard over loopback on the suites'
//! common fixtures, and `api_guide_test` replays the guide's one
//! walk-through. Here each test is one thing a script or the panel's page
//! does, in three parts, as in the CLI's end-to-end suite (`e2e_test`):
//!
//! - the scenario it needs, built from nothing for that test alone
//!   ([`Scenario`], the CLI suite's own): folders and repositories made
//!   directly, Mindweaver's state through the commands (`given`), and, for
//!   what only the API leaves, such as what ran since the server started,
//!   through tasks run clean (`Served::given`);
//! - `mindweaver ui --no-open` typed in a folder of it, with the environment
//!   every command there gets, and one request, the input of the use case,
//!   sent over loopback as curl sends it;
//! - its status and its answer, word for word, as the guide shows them
//!   (`docs/api.md`) where it shows one, and what it left: on the disk, in
//!   what a command reads afterwards, or in what the server says next.
//!
//! Every path a request names is spelled under `/home`, and every path an
//! answer holds reads as `/home`, the home being `/home/you`. An answer to
//! a route the API describes is held to its OpenAPI document as well
//! (`contract`), and every event the stream tells to what the document
//! says an event holds.
//!
//! Unix alone: the reference's paths are a Unix home's, and the server is
//! stopped as a terminal stops it, by a SIGINT that `kill` sends.

// The panel is the `ui` feature's, which a build without it lacks.
#![cfg(all(unix, feature = "ui"))]

// The CLI's suite runs every step this one does not.
#[allow(dead_code)]
#[path = "e2e/scenario.rs"]
mod scenario;

// Shared with the suites that call the panel; this one holds answers and
// events to the document, and reads nothing else of it.
#[allow(dead_code)]
#[path = "common/contract.rs"]
mod contract;

#[path = "api_e2e/served.rs"]
mod served;

#[path = "api_e2e/access.rs"]
mod access;

#[path = "api_e2e/reading.rs"]
mod reading;

#[path = "api_e2e/changing.rs"]
mod changing;

#[path = "api_e2e/authoring.rs"]
mod authoring;

#[path = "api_e2e/managing.rs"]
mod managing;

#[path = "api_e2e/switching.rs"]
mod switching;

#[path = "api_e2e/following.rs"]
mod following;

#[path = "api_e2e/launching.rs"]
mod launching;

#[path = "api_e2e/mistakes.rs"]
mod mistakes;

pub use scenario::Scenario;
pub use served::{Answer, Ask, Served};

/// What the guide's skill of the workspace's own says of itself.
pub const REVIEW: &str = "Read a change before it lands";

/// The guide's workspace: `~/Projects` linking `api`, with one skill of its
/// own, `review`, and beside them `web`, a repository made a project and
/// not linked yet.
pub fn guide_workspace() -> Scenario {
    let scenario = Scenario::projects(&["api"]);
    scenario
        .given(
            "~/Projects",
            &format!("mindweaver add skill review '{REVIEW}'"),
        )
        .repository("~/Projects/web")
        .given("~/Projects/web", "mind init");
    scenario
}

/// `~/Projects` linking each of `projects`, with `review` of its own: the
/// guide's workspace once `web` is linked, or with more projects.
pub fn linking(projects: &[&str]) -> Scenario {
    let scenario = Scenario::projects(projects);
    scenario.given(
        "~/Projects",
        &format!("mindweaver add skill review '{REVIEW}'"),
    );
    scenario
}

/// The entry of each project `answer`, a list of projects, holds, in order.
pub fn entries(answer: &Answer) -> Vec<String> {
    answer
        .json()
        .as_array()
        .unwrap_or_else(|| panic!("not a list of projects:\n{answer}"))
        .iter()
        .map(|project| project["entry"].as_str().unwrap().to_owned())
        .collect()
}

/// The text of a skill's file as `add` writes it.
pub fn added(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
}
