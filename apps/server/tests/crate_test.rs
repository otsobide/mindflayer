//! What the crate may depend on: nothing of mindflayer's. The CLI implements
//! `Backend` and depends on the server, so a dependency the other way would be
//! a cycle, and one on core would be the server starting to hold rules.

use std::process::Command;

use serde_json::Value;

#[test]
fn the_server_depends_on_no_other_mindflayer_crate() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    // `--no-deps --offline`: only what the workspace's own manifests declare,
    // so nothing is resolved or fetched. Direct dependencies are the ones to
    // check, since every crate past them comes from crates.io.
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .args(["--manifest-path", manifest])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();

    let server = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "mindflayer-server")
        .unwrap();
    let ours: Vec<&str> = server["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|dependency| dependency["name"].as_str())
        .filter(|name| name.starts_with("mindflayer"))
        .collect();

    assert!(ours.is_empty(), "the server depends on {ours:?}");
}
