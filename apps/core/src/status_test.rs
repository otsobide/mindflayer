//! Unit tests for `status.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_state_is_serialized_as_its_slug() {
    let serialized = serde_json::to_string(&State::ALL).unwrap();

    assert_eq!(
        serialized,
        r#"["installed","partial","foreign","authored"]"#
    );
}

#[test]
fn every_state_has_a_slug_of_its_own() {
    let mut slugs: Vec<&str> = State::ALL.iter().map(|state| state.slug()).collect();
    slugs.sort_unstable();
    slugs.dedup();

    assert_eq!(slugs.len(), State::ALL.len());
}

/// What a report serializes to is a contract from the first report on: each
/// field by its snake_case name, in the order a reader meets them.
#[test]
fn a_report_serializes_every_field_by_the_name_it_is_held_to() {
    let status = Status {
        projects: vec![
            ProjectStatus {
                name: Some(String::from("collapse")),
                entry: Some(String::from("collapse")),
                path: PathBuf::from("/work/collapse"),
                marker: Some(MarkerFile::SettingsJson),
                agents: vec![Agent::Claude, Agent::OpenCode],
                stale: None,
                unreached: vec![Unreached {
                    kind: Kind::Skill,
                    targets: vec![String::from(".skills")],
                    agents: vec![Agent::Claude],
                }],
                missed: vec![Missed {
                    kind: Kind::Rule,
                    name: String::from("rust/errors"),
                    agents: vec![Agent::OpenCode],
                }],
                artifacts: vec![
                    ArtifactStatus {
                        kind: Kind::Skill,
                        name: String::from("deploy"),
                        state: State::Partial,
                        agents: vec![Agent::OpenCode],
                        origin: Some(String::from("workspace")),
                    },
                    ArtifactStatus {
                        kind: Kind::Rule,
                        name: String::from("rust/errors"),
                        state: State::Installed,
                        agents: vec![Agent::Claude],
                        origin: Some(String::from("workspace")),
                    },
                ],
            },
            ProjectStatus {
                name: None,
                entry: Some(String::from("../old-api")),
                path: PathBuf::from("/old-api"),
                marker: None,
                agents: Vec::new(),
                stale: Some(String::from("gone")),
                unreached: Vec::new(),
                missed: Vec::new(),
                artifacts: Vec::new(),
            },
        ],
    };

    let serialized = serde_json::to_string(&status).unwrap();

    assert_eq!(
        serialized,
        concat!(
            r#"{"projects":["#,
            r#"{"name":"collapse","entry":"collapse","path":"/work/collapse","#,
            r#""marker":"settings.json","agents":["claude","opencode"],"stale":null,"#,
            r#""unreached":[{"kind":"skill","targets":[".skills"],"agents":["claude"]}],"#,
            r#""missed":[{"kind":"rule","name":"rust/errors","agents":["opencode"]}],"#,
            r#""artifacts":["#,
            r#"{"kind":"skill","name":"deploy","state":"partial","#,
            r#""agents":["opencode"],"origin":"workspace"},"#,
            r#"{"kind":"rule","name":"rust/errors","state":"installed","#,
            r#""agents":["claude"],"origin":"workspace"}]},"#,
            r#"{"name":null,"entry":"../old-api","path":"/old-api","marker":null,"#,
            r#""agents":[],"stale":"gone","unreached":[],"missed":[],"artifacts":[]}]}"#,
        )
    );
}
