//! What the CLI is built from. Without its `ui` feature, nothing of the
//! panel's: a build that leaves the panel out (D-17) is spared compiling its
//! server, and its binaries cannot serve anything.

use std::collections::BTreeSet;
use std::process::Command;

/// What only the panel brings: its server, the framework the server is
/// written in, the embed that carries the panel's files, and what opens a
/// browser on it. Not hyper or tokio, which gix's HTTP transport brings to
/// core already.
const PANEL: [&str; 4] = ["mindflayer-server", "axum", "rust-embed", "webbrowser"];

#[test]
fn without_the_ui_feature_nothing_of_the_panels_is_built() {
    let lean = packages(&["--no-default-features"]);
    let default = packages(&[]);

    for name in PANEL {
        assert!(!lean.contains(name), "built without the panel: {name}");
        // Or the check above would pass for a name the tree never holds.
        assert!(default.contains(name), "not built with the panel: {name}");
    }
}

/// Every package the CLI's binaries are built from, with `features` given to
/// cargo, by name: what `cargo tree` lists, without the tests' own.
///
/// `--offline`, so nothing is resolved or fetched afresh: building this test
/// has already put every package of the default build on the disk, and a
/// build with fewer features needs no package that one lacks.
fn packages(features: &[&str]) -> BTreeSet<String> {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["tree", "--offline", "--manifest-path", manifest])
        .args(["-p", "mindflayer-cli", "-e", "normal"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(features)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect()
}
