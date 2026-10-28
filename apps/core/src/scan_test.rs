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
            },
            Found {
                path: PathBuf::from("/work/beta"),
                route: String::from("beta"),
                id: None,
                mind: false,
                linked: false,
                moved: Some(String::from("gamma")),
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
            r#""moved":null},"#,
            r#"{"path":"/work/beta","route":"beta","id":null,"mind":false,"linked":false,"#,
            r#""moved":"gamma"}"#,
            r#"],"untraced":["delta"],"truncated":false}"#,
        )
    );
}
