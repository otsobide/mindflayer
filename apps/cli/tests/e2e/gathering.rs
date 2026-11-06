//! Gathering skills from a git repository onto the workspace's shelf, and
//! installing from there: the reference's "Gathering skills from
//! elsewhere".
//!
//! A source is named by its path from the workspace, as the reference names
//! one by its URL: a table pads every row to its longest source, which a
//! temporary folder's path would make as long as the system's says.

use crate::{skill, Scenario};

const SOURCE: &str = "~/src/acme-skills";

/// `~/Projects` linking `collapse`, beside `~/src/acme-skills`, a repository
/// offering two skills.
fn acme() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                "skills/ddd-reviewer/SKILL.md",
                &skill(
                    "ddd-reviewer",
                    "Review a change against the DDD layering rules",
                ),
            ),
        ],
    );
    scenario
}

#[test]
fn gathering_a_repository_puts_its_skills_on_the_shelf_and_says_each() {
    let scenario = acme();

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 2 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert!(
        !scenario.exists("~/Projects/collapse/.claude"),
        "nothing is written into a project"
    );
}

#[test]
fn a_dry_run_says_what_a_gather_would_take_from_every_layout_and_takes_nothing() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                ".claude/skills/ddd-reviewer/SKILL.md",
                &skill(
                    "ddd-reviewer",
                    "Review a change against the DDD layering rules",
                ),
            ),
            (
                "plugins/ops/skills/deploy/SKILL.md",
                &skill("deploy", "Ship the service"),
            ),
        ],
    );
    let before = scenario.files("~/Projects/.mindweaver");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --path plugins/ops --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "../src/acme-skills at {}\n\
             \x20 deploy  plugins/ops/skills/deploy\n\
             \n\
             1 skill to gather; nothing gathered: run it again without --dry-run to gather them\n",
            scenario.head(SOURCE)
        ))
        .quiet();
    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "../src/acme-skills at {}\n\
             \x20 ddd-reviewer  .claude/skills/ddd-reviewer\n\
             \x20 deploy        plugins/ops/skills/deploy\n\
             \x20 commit-style  skills/commit-style\n\
             \n\
             3 skills to gather; nothing gathered: run it again without --dry-run to gather them\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert_eq!(scenario.files("~/Projects/.mindweaver"), before);
}

#[test]
fn an_address_naming_another_branch_than_its_ref_is_refused() {
    let scenario = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git acme/skills@v2 --ref v3",
        )
        .exits(1)
        .prints("")
        .says(
            "error: acme/skills@v2 names the branch or tag `v2`, and --ref names `v3`: give one\n",
        );
}

#[test]
fn gathering_again_says_what_moved() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[(
                "skills/commit-style/SKILL.md",
                &format!(
                    "{}Keep the subject short.\n",
                    skill("commit-style", "How this repo writes commit messages")
                ),
            )],
        );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 updated    commit-style  How this repo writes commit messages\n\
             \x20 unchanged  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 0 added, 1 updated, 1 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
}

#[test]
fn a_skill_breaking_the_format_is_gathered_with_a_warning() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1)
        .prints(&format!(
            "src-other-rules: ../src/other-rules at {}\n\
             \x20 added  pdf--forms  Fill in PDF forms\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/other-rules")
        ))
        .says(
            "warning: /home/you/Projects/.mindweaver/skills/..-src-other-rules/pdf--forms/SKILL.md: \
             `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, \
             joined by single hyphens\n",
        );
}

#[test]
fn gather_list_says_what_the_shelf_holds_and_from_where() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  src-acme-skills  unknown  How this repo writes commit messages\n",
            "ddd-reviewer  src-acme-skills  unknown  Review a change against the DDD layering rules\n",
        ))
        .quiet();
}

#[test]
fn gather_list_says_each_skills_licence_and_the_commit_its_source_is_pinned_to() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                "skills/ddd-reviewer/SKILL.md",
                "---\nname: ddd-reviewer\ndescription: Review a change against the DDD layering \
                 rules\nlicense: Apache-2.0\n---\n",
            ),
        ],
    );
    let pinned = scenario.commit(SOURCE);
    // A licence file the pinned commit does not hold yet.
    scenario.source(SOURCE, &[("skills/commit-style/LICENSE", "MIT License\n")]);
    scenario.source(
        "~/src/other-rules",
        &[
            (
                "skills/pdf-forms/SKILL.md",
                &skill("pdf-forms", "Fill in PDF forms"),
            ),
            (
                "skills/pdf-forms/LICENSE.md",
                "# Licence\n\nSPDX-License-Identifier: MIT\n",
            ),
        ],
    );
    scenario
        .given(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pinned}"),
        )
        .given("~/Projects", "mindweaver gather git ../src/other-rules");

    let short = &pinned[..7];
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(&format!(
            "commit-style  src-acme-skills  pinned {short}  unknown     How this repo writes \
             commit messages\n\
             ddd-reviewer  src-acme-skills  pinned {short}  Apache-2.0  Review a change against \
             the DDD layering rules\n\
             pdf-forms     src-other-rules                  MIT         Fill in PDF forms\n"
        ))
        .quiet();
}

#[test]
fn a_pinned_gather_takes_off_the_shelf_what_its_commit_lacks() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[(
            "skills/commit-style/SKILL.md",
            &skill("commit-style", "How this repo writes commit messages"),
        )],
    );
    let pinned = scenario.commit(SOURCE);
    scenario.change(
        SOURCE,
        &[(
            "skills/ddd-reviewer/SKILL.md",
            &skill(
                "ddd-reviewer",
                "Review a change against the DDD layering rules",
            ),
        )],
    );
    // Gathered at the branch first, so the shelf holds a skill the pinned
    // commit does not.
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pinned}"),
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 removed    ddd-reviewer  Review a change against the DDD layering rules\n\
             \x20 unchanged  commit-style  How this repo writes commit messages\n\
             \n\
             1 skill: 0 added, 0 updated, 1 unchanged, 1 removed\n",
            &pinned[..7]
        ))
        .quiet();

    assert!(!scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/ddd-reviewer"));
}

#[test]
fn gather_list_says_a_pin_only_of_a_skill_taken_at_it() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            ("skills/ddd-reviewer/SKILL.md", "no front matter yet\n"),
        ],
    );
    let pinned = scenario.commit(SOURCE);
    scenario.change(
        SOURCE,
        &[(
            "skills/ddd-reviewer/SKILL.md",
            &skill(
                "ddd-reviewer",
                "Review a change against the DDD layering rules",
            ),
        )],
    );
    // Gathered at the branch first, so the shelf holds a skill whose folder
    // the pinned commit holds and cannot be read from, which the pinned
    // gather leaves as it was taken.
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");
    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pinned}"),
        )
        .exits(1);

    let short = &pinned[..7];
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(&format!(
            "commit-style  src-acme-skills  pinned {short}  unknown  How this repo writes commit \
             messages\n\
             ddd-reviewer  src-acme-skills                  unknown  Review a change against the \
             DDD layering rules\n"
        ))
        .quiet();
}

/// When `gather sources --json` says each source was last gathered, in the
/// order it lists them: what no run makes the same, read back to be spelled
/// in what the text says.
fn gathered_times(scenario: &Scenario) -> Vec<String> {
    let ran = scenario.run("~/Projects", "mindweaver gather sources --json");
    let envelope: serde_json::Value = serde_json::from_str(&ran.stdout).expect("JSON");
    envelope["data"]
        .as_array()
        .expect("a list of sources")
        .iter()
        .map(|source| utc(source["gathered_at"].as_i64().expect("gathered")))
        .collect()
}

/// `seconds` since the epoch as the text says a time: its date and minute
/// in UTC, worked out here from the calendar rather than by the CLI's own.
fn utc(seconds: i64) -> String {
    let (mut days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let mut year = 1970;
    let leap = |year: i64| (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= lengths[month] {
        days -= lengths[month];
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02} UTC",
        month + 1,
        days + 1,
        of_day / 3600,
        of_day % 3600 / 60
    )
}

#[test]
fn gather_sources_lists_each_source_by_its_name_with_what_it_holds() {
    let scenario = acme();
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf-forms/SKILL.md",
            &skill("pdf-forms", "Fill in PDF forms"),
        )],
    );
    let pinned = scenario.commit(SOURCE);
    scenario
        .given(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pinned}"),
        )
        .given("~/Projects", "mindweaver gather git ../src/other-rules");
    let at = gathered_times(&scenario);

    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints(&format!(
            "src-acme-skills  ../src/acme-skills  pinned {}  2 skills  gathered {}\n\
             src-other-rules  ../src/other-rules                  1 skill   gathered {}\n",
            &pinned[..7],
            at[0],
            at[1]
        ))
        .quiet();
}

#[test]
fn gather_rename_calls_a_source_another_name_from_then_on() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather rename src-acme-skills team-skills",
        )
        .succeeds()
        .prints("renamed source src-acme-skills to team-skills, gathered from ../src/acme-skills\n")
        .quiet();

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  team-skills  unknown  How this repo writes commit messages\n",
            "ddd-reviewer  team-skills  unknown  Review a change against the DDD layering rules\n",
        ))
        .quiet();
}

#[test]
fn gather_update_gathers_a_source_again_and_status_then_says_what_moved_and_what_went() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .given("~/Projects", "mindweaver install -p collapse ddd-reviewer")
        .change(
            SOURCE,
            &[(
                "skills/commit-style/SKILL.md",
                &format!(
                    "{}Keep the subject short.\n",
                    skill("commit-style", "How this repo writes commit messages")
                ),
            )],
        );
    std::fs::remove_dir_all(scenario.at("~/src/acme-skills/skills/ddd-reviewer")).unwrap();
    scenario.change(SOURCE, &[]);

    scenario
        .run("~/Projects", "mindweaver gather update src-acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 updated  commit-style  How this repo writes commit messages\n\
             \x20 removed  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             1 skill: 0 added, 1 updated, 0 unchanged, 1 removed\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints(
            "commit-style  outdated  claude,codex,opencode  ../src/acme-skills\n\
             ddd-reviewer  orphaned  claude,codex,opencode  ../src/acme-skills\n",
        )
        .quiet();
}

#[test]
fn gather_remove_says_what_would_go_and_what_it_would_leave_orphaned() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver install -p collapse commit-style");

    scenario
        .run("~/Projects", "mindweaver gather remove src-acme-skills")
        .succeeds()
        .prints(
            "would remove the source src-acme-skills, gathered from ../src/acme-skills, and the \
             2 skills it holds on the shelf\n\
             would delete /home/you/Projects/.mindweaver/cache/..-src-acme-skills\n\
             would delete /home/you/Projects/.mindweaver/skills/..-src-acme-skills\n\
             would leave commit-style in collapse, installed from it, orphaned\n\
             nothing removed: add --yes to go ahead\n",
        )
        .quiet();

    assert!(scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/commit-style"));
}

#[test]
fn gather_remove_yes_takes_the_source_and_leaves_its_copies_orphaned() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver install -p collapse commit-style");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather remove src-acme-skills --yes",
        )
        .succeeds()
        .prints(
            "removed the source src-acme-skills, gathered from ../src/acme-skills, and the 2 \
             skills it held on the shelf\n\
             deleted /home/you/Projects/.mindweaver/cache/..-src-acme-skills\n\
             deleted /home/you/Projects/.mindweaver/skills/..-src-acme-skills\n\
             left commit-style in collapse, installed from it, orphaned\n",
        )
        .says(
            "note: nothing offers what was installed from it any more; `mindweaver uninstall \
             --orphaned --all-projects` takes out what nothing offers\n",
        );

    assert!(!scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills"));
    assert!(!scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-skills"));
    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints("commit-style  orphaned  claude,codex,opencode  ../src/acme-skills\n")
        .quiet();
}

#[test]
fn a_gathered_skill_installs_from_the_source_it_came_from() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver install -p collapse commit-style")
        .succeeds()
        .prints("installed commit-style into collapse, from ../src/acme-skills\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        scenario.read("~/src/acme-skills/skills/commit-style/SKILL.md")
    );
}

#[test]
fn a_gathered_skill_the_format_refuses_is_not_installed() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );
    // Gathered with its warning, which a step building the scenario would
    // refuse: the warning is the use case before this one.
    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1);

    scenario
        .run("~/Projects", "mindweaver install -p collapse pdf--forms")
        .exits(1)
        .prints("")
        .says(
            "error: `pdf--forms` cannot be installed: `pdf--forms` is not usable as a name: \
             it has to be lowercase letters and digits, joined by single hyphens\n",
        );

    assert!(!scenario.exists("~/Projects/collapse/.claude"));
}

#[test]
fn a_url_carrying_a_token_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git https://me:ghp_t0ken@github.com/acme/private-skills",
        )
        .exits(1)
        .prints("")
        .says(
            "error: https://github.com/acme/private-skills: a password, a token or, over \
             http(s), a user name in the URL is refused, because gathering stores and shows the \
             URL: gather it without one, and configure a git credential helper that has it\n",
        );

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
}

#[test]
fn mindweaver_validate_reads_the_shelf_beside_the_library_and_the_projects() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver validate")
        .succeeds()
        .prints(concat!(
            "commit-style: ok\n",
            "ddd-reviewer: ok\n",
            "\n",
            "2 skills checked, 0 invalid\n",
        ))
        .quiet();
}
