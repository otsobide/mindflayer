use super::*;

/// A report's names are a contract, as every report's are: each field of
/// a project examined and of what was found, by its name, in its order.
#[test]
fn a_report_serializes_every_name_in_order() {
    let report = Report {
        projects: vec![Examined {
            entry: Some(String::from("app")),
            name: Some(String::from("app")),
            path: PathBuf::from("/work/app"),
            findings: vec![Symptom::CodexDistrusted {
                config: PathBuf::from("/home/you/.codex/config.toml"),
                project: PathBuf::from("/work/app"),
            }
            .finding()],
            failures: Vec::new(),
        }],
    };

    let json = serde_json::to_value(&report).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "projects": [{
                "entry": "app",
                "name": "app",
                "path": "/work/app",
                "findings": [{
                    "code": "doctor_codex_distrusted",
                    "agents": ["codex"],
                    "path": "/home/you/.codex/config.toml",
                    "message": "/home/you/.codex/config.toml: marks /work/app `untrusted`, so \
                                Codex reads neither its AGENTS.md nor its .codex/config.toml",
                    "fix": "set `trust_level = \"trusted\"` under `[projects.\"/work/app\"]` \
                            there, if Codex is to work in the project",
                }],
                "failures": [],
            }],
        })
    );
}

/// What a finding says is what its symptom says, by the symptom's code,
/// so a front end acting on the code acts on what the message says.
#[test]
fn a_finding_is_its_symptom_said() {
    let symptom = Symptom::CodexUntrusted {
        file: PathBuf::from("/work/app/.codex/config.toml"),
        project: PathBuf::from("/work/app"),
        config: None,
    };

    let finding = symptom.finding();

    assert_eq!(finding.code, symptom.code());
    assert_eq!(finding.message, symptom.to_string());
    assert_eq!(finding.path, PathBuf::from("/work/app/.codex/config.toml"));
    assert_eq!(finding.agents, vec![Agent::Codex]);
}
