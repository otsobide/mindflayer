use std::io;
use std::path::PathBuf;

use super::*;

#[test]
fn a_scan_is_serialized_field_by_field_without_its_failures() {
    let scan = Scan {
        roots: vec![PathBuf::from("/work")],
        depth: 3,
        found: vec![
            Found {
                path: PathBuf::from("/work/alpha"),
                route: String::from("alpha"),
                id: ProjectId::parse("4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"),
                mind: true,
                linked: true,
                moved: None,
                reason: Reason::Id,
                agent_files: vec![String::from("AGENTS.md")],
            },
            Found {
                path: PathBuf::from("/work/beta"),
                route: String::from("beta"),
                id: None,
                mind: false,
                linked: false,
                moved: Some(String::from("gamma")),
                reason: Reason::Git,
                agent_files: Vec::new(),
            },
        ],
        untraced: vec![String::from("delta")],
        failures: vec![ScanFailure::Unlisted {
            path: PathBuf::from("/work/locked"),
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        }],
        truncated: false,
    };

    let serialized = serde_json::to_string(&scan).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"roots":["/work"],"depth":3,"found":["#,
            r#"{"path":"/work/alpha","route":"alpha","#,
            r#""id":"4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5","mind":true,"linked":true,"#,
            r#""moved":null,"reason":"id","agent_files":["AGENTS.md"]},"#,
            r#"{"path":"/work/beta","route":"beta","id":null,"mind":false,"linked":false,"#,
            r#""moved":"gamma","reason":"git","agent_files":[]}"#,
            r#"],"untraced":["delta"],"truncated":false}"#,
        )
    );
}

#[test]
fn a_reason_is_serialized_as_the_word_a_report_names_it_by() {
    let reasons = [Reason::Id, Reason::Mind, Reason::Agents, Reason::Git];

    let serialized = serde_json::to_string(&reasons).unwrap();

    assert_eq!(serialized, r#"["id","mind","agents","git"]"#);
}

#[test]
fn reasons_rank_a_project_above_a_repository_and_an_id_above_none() {
    let mut reasons = vec![Reason::Git, Reason::Agents, Reason::Mind, Reason::Id];

    reasons.sort();

    assert_eq!(
        reasons,
        [Reason::Id, Reason::Mind, Reason::Agents, Reason::Git]
    );
}
