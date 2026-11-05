//! Reading what is where: the reference's "What each project has",
//! "Machine-readable output" and "What a skill looks like".

use crate::{skill, Scenario};

/// `~/Projects` linking `collapse`, which holds one of each state a project
/// shows most: `deploy`, its own; `commit-style`, installed from the
/// library; and `notes`, kept by hand where Claude and OpenCode read.
fn collapse_holding_three() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        )
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .file(
            "~/Projects/collapse/.claude/skills/notes/SKILL.md",
            &skill("notes", "My own notes"),
        );
    scenario
}

#[test]
fn status_says_what_a_project_holds_who_reads_it_and_where_it_came_from() {
    let scenario = collapse_holding_three();

    scenario
        .run("~/Projects/collapse", "mind status")
        .succeeds()
        .prints(concat!(
            "commit-style  current   claude,codex,opencode  workspace\n",
            "deploy        authored  -\n",
            "notes         foreign   claude,opencode\n",
        ))
        .quiet();
}

#[test]
fn status_names_each_project_when_there_is_more_than_one() {
    let scenario = Scenario::projects(&["collapse", "tanukeys"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given(
            "~/Projects",
            "mindweaver install --all-projects commit-style",
        );

    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints(concat!(
            "collapse  commit-style  current  claude,codex,opencode  workspace\n",
            "tanukeys  commit-style  current  claude,codex,opencode  workspace\n",
        ))
        .quiet();
}

#[test]
fn status_json_prints_the_same_report_in_the_envelope() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style");

    scenario
        .run("~/Projects", "mindweaver status -p collapse --json")
        .succeeds()
        .prints(concat!(
            "{\n",
            "  \"version\": 2,\n",
            "  \"data\": {\n",
            "    \"projects\": [\n",
            "      {\n",
            "        \"name\": \"collapse\",\n",
            "        \"entry\": \"collapse\",\n",
            "        \"path\": \"/home/you/Projects/collapse\",\n",
            "        \"marker\": \"settings.toml\",\n",
            "        \"agents\": [\n",
            "          \"claude\",\n",
            "          \"codex\",\n",
            "          \"opencode\"\n",
            "        ],\n",
            "        \"stale\": null,\n",
            "        \"unreached\": [],\n",
            "        \"missed\": [],\n",
            "        \"artifacts\": [\n",
            "          {\n",
            "            \"kind\": \"skill\",\n",
            "            \"name\": \"commit-style\",\n",
            "            \"state\": \"current\",\n",
            "            \"agents\": [\n",
            "              \"claude\",\n",
            "              \"codex\",\n",
            "              \"opencode\"\n",
            "            ],\n",
            "            \"origin\": \"workspace\"\n",
            "          }\n",
            "        ]\n",
            "      }\n",
            "    ]\n",
            "  },\n",
            "  \"warnings\": [],\n",
            "  \"coded_warnings\": []\n",
            "}\n",
        ))
        .quiet();
}

#[test]
fn a_warning_reaches_the_envelope_with_its_code_and_is_said_on_stderr_too() {
    let scenario = Scenario::projects(&["collapse"]);
    std::fs::rename(
        scenario.at("~/Projects/collapse"),
        scenario.at("~/Projects/moved"),
    )
    .expect("move the project by hand");
    let message = "/home/you/Projects/collapse: not a mind project \
                   (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)";

    let ran = scenario.run("~/Projects", "mindweaver status --json");

    ran.exits(1).says(&format!("warning: {message}\n"));
    let envelope: serde_json::Value =
        serde_json::from_str(&ran.stdout).expect("--json prints JSON even when it warns");
    // The line stderr says, without its prefix, and beside it the code a
    // script tells the condition by without reading the words.
    assert_eq!(
        envelope,
        serde_json::json!({
            "version": 2,
            "data": {
                "projects": [{
                    "name": null,
                    "entry": "collapse",
                    "path": "/home/you/Projects/collapse",
                    "marker": null,
                    "agents": [],
                    "stale": message,
                    "unreached": [],
                    "missed": [],
                    "artifacts": [],
                }],
            },
            "warnings": [message],
            "coded_warnings": [{"code": "not_a_project", "message": message}],
        })
    );
}

#[test]
fn show_prints_an_artifact_and_where_it_is() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.given(
        "~/Projects/collapse",
        "mind add skill review \"Review a change\"",
    );

    scenario
        .run("~/Projects/collapse", "mind show review")
        .succeeds()
        .prints(concat!(
            "review\n",
            "/home/you/Projects/collapse/skills/review/SKILL.md\n",
            "\n",
            "Review a change\n",
            "\n",
            "# review\n",
        ))
        .quiet();
}

#[test]
fn validate_says_everything_wrong_with_a_skill_and_fails() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects/collapse",
            "mind add skill review \"Review a change\"",
        )
        .file(
            "~/Projects/collapse/skills/review/SKILL.md",
            &skill("Review_It", "Review a change"),
        );

    scenario
        .run("~/Projects/collapse", "mind validate")
        .exits(1)
        .prints(concat!(
            "Review_It: 2 problems\n",
            "  - `Review_It` is not usable as a name: it has to be lowercase letters and digits, \
             joined by single hyphens\n",
            "  - `name` is `Review_It` but the directory is `review`; they have to match\n",
            "\n",
            "1 skill checked, 1 invalid\n",
        ))
        .quiet();
}
