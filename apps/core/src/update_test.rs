//! Unit tests for `update.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn only_an_outdated_a_missing_or_a_forced_installation_is_written() {
    let path = PathBuf::from(".agents/skills/deploy");

    let writes: Vec<bool> = [
        Step::Keep,
        Step::Refresh,
        Step::PutBack(vec![path.clone()]),
        Step::WriteOver(vec![path.clone()]),
        Step::Leave(vec![path]),
        Step::Orphaned,
        Step::Unsure(vec![String::from("workspace")]),
    ]
    .iter()
    .map(Step::writes)
    .collect();

    assert_eq!(writes, [false, true, true, true, false, false, false]);
}

#[test]
fn only_an_installation_left_as_edited_is_refused_and_by_an_install_s_code() {
    let path = PathBuf::from(".agents/skills/deploy");

    let refused: Vec<Option<&str>> = [
        Step::Keep,
        Step::Refresh,
        Step::PutBack(vec![path.clone()]),
        Step::WriteOver(vec![path.clone()]),
        Step::Leave(vec![path]),
        Step::Orphaned,
        Step::Unsure(vec![String::from("workspace")]),
    ]
    .iter()
    .map(|step| step.refused().map(|refused| refused.code()))
    .collect();

    assert_eq!(
        refused,
        [None, None, None, None, Some("edited"), None, None]
    );
}

#[test]
fn a_plan_that_writes_nothing_is_empty_whatever_it_says() {
    let update = |step| Update {
        kind: Kind::Skill,
        name: String::from("deploy"),
        origin: Some(String::from("workspace")),
        standing: Standing::Orphaned,
        step,
        done: None,
        candidate: None,
    };
    let plan = |step| Plan {
        projects: vec![ProjectPlan {
            root: PathBuf::from("/work/collapse"),
            name: String::from("collapse"),
            installations: vec![update(step)],
            rendered: None,
            left: Vec::new(),
        }],
        unmatched: vec![String::from("typo")],
    };

    assert!(plan(Step::Orphaned).is_empty());
    assert!(plan(Step::Leave(Vec::new())).is_empty());
    assert!(!plan(Step::Refresh).is_empty());
}
