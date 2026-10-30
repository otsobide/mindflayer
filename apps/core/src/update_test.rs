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

#[test]
fn each_step_is_said_by_a_word_of_its_own() {
    let path = PathBuf::from(".agents/skills/deploy");

    let words: Vec<serde_json::Value> = [
        Step::Keep,
        Step::Refresh,
        Step::PutBack(vec![path.clone()]),
        Step::WriteOver(vec![path.clone()]),
        Step::Leave(vec![path]),
        Step::Orphaned,
        Step::Unsure(vec![String::from("workspace")]),
    ]
    .iter()
    .map(|step| serde_json::to_value(step.does()).unwrap())
    .collect();

    assert_eq!(
        words,
        [
            "keep",
            "refresh",
            "put_back",
            "write_over",
            "leave",
            "orphaned",
            "unsure"
        ]
    );
}

/// An installation in `collapse` at `/work/collapse`, standing as given,
/// from the workspace's own.
fn update(standing: Standing, step: Step) -> Update {
    Update {
        kind: Kind::Skill,
        name: String::from("deploy"),
        origin: Some(String::from("workspace")),
        standing,
        step,
        done: None,
        candidate: None,
    }
}

/// Why an update leaves what it leaves, in core's words and by a code: an
/// edit by the code an install refuses it with, the rest by codes of their
/// own, and nothing for what it writes or has nothing to do for.
#[test]
fn an_update_says_why_it_leaves_an_installation_and_nothing_of_one_it_writes() {
    let root = Path::new("/work/collapse");
    let left = |standing, step: Step| {
        let update = update(standing, step.clone());
        step.left(&update, "collapse", root)
            .map(|left| (left.code(), left.to_string()))
    };

    assert_eq!(
        left(
            Standing::Edited,
            Step::Leave(vec![root.join(".claude/skills/deploy/SKILL.md")])
        ),
        Some((
            "edited",
            String::from(
                "skill `deploy` in collapse was changed since it was installed, at \
                 .claude/skills/deploy/SKILL.md, so an update leaves it alone, every copy of \
                 it; forcing the update writes over the change"
            )
        ))
    );
    assert_eq!(
        left(Standing::Orphaned, Step::Orphaned),
        Some((
            "no_longer_offered",
            String::from(
                "skill `deploy` in collapse is no longer offered by workspace, so an update \
                 leaves it as it is; uninstalling it takes it out"
            )
        ))
    );
    assert_eq!(
        left(Standing::Edited, Step::Orphaned).map(|(_, said)| said),
        Some(String::from(
            "skill `deploy` in collapse is no longer offered by workspace, so an update leaves \
             it as it is; it was changed since it was installed, so only a forced uninstall \
             takes it out"
        ))
    );
    assert_eq!(
        left(
            Standing::Outdated,
            Step::Unsure(vec![
                String::from("workspace"),
                String::from("load:../team")
            ])
        ),
        Some((
            "origin_unknown",
            String::from(
                "skill `deploy` in collapse is offered by workspace, load:../team, and was \
                 installed before Mindflayer kept where from, so an update leaves it as it is; \
                 `install --from` says which"
            )
        ))
    );
    for (standing, step) in [
        (Standing::Current, Step::Keep),
        (Standing::Outdated, Step::Refresh),
        (Standing::Missing, Step::PutBack(Vec::new())),
        (Standing::Edited, Step::WriteOver(Vec::new())),
    ] {
        assert_eq!(left(standing, step.clone()), None, "{step:?}");
    }
}

/// One condition, one code: an edit left alone is what an install refuses
/// an edited copy for.
#[test]
fn every_code_an_update_echoes_is_one_core_gives() {
    let core: Vec<&str> = crate::code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .collect();

    assert_eq!(Left::ECHOED, [Refused::Edited.code()]);
    assert!(Left::ECHOED.iter().all(|code| core.contains(code)));
}
