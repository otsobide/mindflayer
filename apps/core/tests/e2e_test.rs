//! Core end to end: the engine driven one use case at a time, through the
//! public calls every front end makes.
//!
//! Core has no binary and prints nothing: what it is to the CLI, the TUI
//! and the panel is its public API, whose answers they render, and whose
//! read types they print as JSON. Its other suites hold a module each, or
//! a flow through several, built by its mothers over values drawn afresh
//! (`flows_test`). Here each test is one thing a front end does, in three
//! parts, as in the CLI's and the server's end-to-end suites:
//!
//! - the world it needs, built from nothing for that test alone
//!   ([`World`]): folders and git repositories made directly, and
//!   Mindflayer's own state only through core's public calls, each the one
//!   a command makes for that step, with what a front end hands it (the
//!   home, the agents' own folders under it), and held to go cleanly;
//! - one call, the input of the use case, as a front end makes it;
//! - what it answered and left: a read type as the JSON a front end is
//!   handed, every path under the world reading as `/home`, the home being
//!   `/home/you`; a refusal by its code and its words; and the disk, or
//!   what the snapshot every front end reads says next.
//!
//! No test shares a world with another, so any one runs alone (`cargo test
//! -p mindflayer-core --test e2e_test <name>`).
//!
//! Unix alone: the reference's paths are a Unix home's, which a Windows
//! path never reads as, and a link is made as Unix makes one.

#![cfg(unix)]

#[path = "e2e/world.rs"]
mod world;

#[path = "e2e/workspaces.rs"]
mod workspaces;

#[path = "e2e/projects.rs"]
mod projects;

#[path = "e2e/authoring.rs"]
mod authoring;

#[path = "e2e/installing.rs"]
mod installing;

#[path = "e2e/gathering.rs"]
mod gathering;

#[path = "e2e/updating.rs"]
mod updating;

#[path = "e2e/reading.rs"]
mod reading;

#[path = "e2e/settings.rs"]
mod settings;

#[path = "e2e/upgrading.rs"]
mod upgrading;

#[path = "e2e/adopting.rs"]
mod adopting;

pub use world::{at, World};
