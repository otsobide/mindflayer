//! Unit tests for `plan.rs`: what a selection of boxes amounts to, each box
//! given as it stands, so no disk is read.

use serde_json::json;

use super::*;

/// The box of skill `name` from `origin` in `project`, standing so with
/// its name there, held from there or not, and asked to be held or not.
fn skill(
    project: &str,
    name: &str,
    origin: &str,
    standing: Standing,
    held: bool,
    wanted: bool,
) -> Asked {
    of(project, Kind::Skill, name, origin, standing, held, wanted)
}

fn of(
    project: &str,
    kind: Kind,
    name: &str,
    origin: &str,
    standing: Standing,
    held: bool,
    wanted: bool,
) -> Asked {
    Asked {
        cell: Cell::of(project, kind, name, origin, wanted),
        kind,
        name: name.to_owned(),
        standing,
        held,
    }
}

fn install(projects: &[&str], kind: Kind, name: &str, from: &str) -> Change {
    Change::Install {
        projects: projects.iter().map(|&project| project.to_owned()).collect(),
        kind,
        name: name.to_owned(),
        from: from.to_owned(),
    }
}

fn uninstall(projects: &[&str], kind: Kind, name: &str) -> Change {
    Change::Uninstall {
        projects: projects.iter().map(|&project| project.to_owned()).collect(),
        kind,
        name: name.to_owned(),
    }
}

/// The code of each conflict, and of each box locked, in order.
fn codes(plan: &Plan) -> (Vec<&str>, Vec<&str>) {
    (
        plan.conflicts
            .iter()
            .map(|conflict| conflict.why.code)
            .collect(),
        plan.locked.iter().map(|locked| locked.why.code).collect(),
    )
}

// ---------------------------------------------------------------------------
// What a selection amounts to
// ---------------------------------------------------------------------------

#[test]
fn an_offer_asked_for_is_installed_from_where_it_is_into_every_project_asking_it_on_one_line() {
    let plan = Plan::of(&[
        skill("beta", "deploy", "workspace", Standing::Absent, false, true),
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        skill(
            "alpha",
            "deploy",
            "load:../team",
            Standing::Absent,
            false,
            false,
        ),
    ]);

    assert_eq!(
        plan.changes,
        [install(
            &["beta", "alpha"],
            Kind::Skill,
            "deploy",
            "workspace"
        )]
    );
    assert_eq!(codes(&plan), (vec![], vec![]));
}

#[test]
fn a_name_held_and_asked_away_is_uninstalled_by_its_name() {
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        skill(
            "beta",
            "deploy",
            "https://example.com/acme",
            Standing::Outdated,
            true,
            false,
        ),
    ]);

    assert_eq!(
        plan.changes,
        [uninstall(&["alpha", "beta"], Kind::Skill, "deploy")]
    );
}

#[test]
fn a_box_asked_to_be_as_it_is_changes_nothing() {
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            true,
        ),
        skill(
            "alpha",
            "review",
            "workspace",
            Standing::Absent,
            false,
            false,
        ),
        // Every copy gone: no offer of it is held, so asking it away is
        // asking for what is so.
        skill(
            "alpha",
            "lint",
            "workspace",
            Standing::Missing,
            false,
            false,
        ),
        // Held from elsewhere, so not from here.
        skill(
            "beta",
            "deploy",
            "load:../team",
            Standing::Current,
            false,
            false,
        ),
    ]);

    assert_eq!(plan, Plan::default());
}

#[test]
fn an_installation_whose_copies_have_gone_is_put_back_by_asking_for_it() {
    let plan = Plan::of(&[skill(
        "alpha",
        "deploy",
        "workspace",
        Standing::Missing,
        false,
        true,
    )]);

    assert_eq!(
        plan.changes,
        [install(&["alpha"], Kind::Skill, "deploy", "workspace")]
    );
}

#[test]
fn an_offer_asked_for_where_another_of_its_name_is_held_replaces_it_rather_than_taking_it_out() {
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        skill(
            "alpha",
            "deploy",
            "load:../team",
            Standing::Current,
            false,
            true,
        ),
    ]);

    // Installing the one asked for writes over the one held, which is
    // Mindflayer's: taking it out first would leave the project without it
    // were the install to fail.
    assert_eq!(
        plan.changes,
        [install(&["alpha"], Kind::Skill, "deploy", "load:../team")]
    );
}

#[test]
fn an_offer_held_beside_another_held_one_asked_away_is_installed_again_in_its_place() {
    // An installation nothing offers any more where it came from, asked
    // away, beside the offer of its name that ticks its box.
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "load:../gone",
            Standing::Orphaned,
            true,
            false,
        ),
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Orphaned,
            true,
            true,
        ),
    ]);

    assert_eq!(
        plan.changes,
        [install(&["alpha"], Kind::Skill, "deploy", "workspace")]
    );
}

#[test]
fn changes_go_kind_by_kind_each_where_the_first_box_asking_it_is() {
    let plan = Plan::of(&[
        of(
            "alpha",
            Kind::Rule,
            "git/commits",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        skill(
            "alpha",
            "review",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        // Held from the workspace, the box asked away first: the install
        // replacing it goes where the box asked for is.
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        skill("alpha", "lint", "workspace", Standing::Absent, false, true),
        skill(
            "alpha",
            "deploy",
            "load:../team",
            Standing::Current,
            false,
            true,
        ),
        skill(
            "beta",
            "review",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        skill("beta", "lint", "workspace", Standing::Absent, false, true),
    ]);

    assert_eq!(
        plan.changes,
        [
            uninstall(&["alpha", "beta"], Kind::Skill, "review"),
            install(&["alpha", "beta"], Kind::Skill, "lint", "workspace"),
            install(&["alpha"], Kind::Skill, "deploy", "load:../team"),
            install(&["alpha"], Kind::Rule, "git/commits", "workspace"),
        ]
    );
}

#[test]
fn a_skill_and_a_rule_of_one_name_are_two_things() {
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        of(
            "alpha",
            Kind::Rule,
            "deploy",
            "load:../team",
            Standing::Absent,
            false,
            true,
        ),
    ]);

    assert_eq!(
        plan.changes,
        [
            install(&["alpha"], Kind::Skill, "deploy", "workspace"),
            install(&["alpha"], Kind::Rule, "deploy", "load:../team"),
        ]
    );
    assert!(plan.conflicts.is_empty());
}

#[test]
fn a_box_asked_twice_alike_is_asked_once() {
    let plan = Plan::of(&[
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
    ]);

    assert_eq!(
        plan.changes,
        [install(&["alpha"], Kind::Skill, "deploy", "workspace")]
    );
}

// ---------------------------------------------------------------------------
// Each conflict
// ---------------------------------------------------------------------------

#[test]
fn one_name_asked_for_from_two_places_in_one_project_clashes_and_nothing_of_it_is_planned_there() {
    let asked = [
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            true,
        ),
        skill(
            "alpha",
            "deploy",
            "load:../team",
            Standing::Current,
            false,
            true,
        ),
        skill(
            "beta",
            "deploy",
            "load:../team",
            Standing::Absent,
            false,
            true,
        ),
    ];

    let plan = Plan::of(&asked);

    // The other project goes ahead.
    assert_eq!(
        plan.changes,
        [install(&["beta"], Kind::Skill, "deploy", "load:../team")]
    );
    assert_eq!(plan.locked, []);
    assert_eq!(
        plan.conflicts,
        [Conflict {
            project: String::from("alpha"),
            kind: Kind::Skill,
            name: String::from("deploy"),
            cells: vec![asked[0].cell.clone(), asked[1].cell.clone()],
            why: Said {
                code: "wanted_twice",
                message: String::from(
                    "`deploy` is asked of alpha from more than one place (workspace, \
                     load:../team), and a project holds one skill of a name: ask for one"
                ),
            },
        }]
    );
}

#[test]
fn one_box_asked_both_ways_clashes_and_nothing_of_its_name_is_planned_there() {
    let asked = [
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            false,
        ),
        skill(
            "alpha",
            "review",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
    ];

    let plan = Plan::of(&asked);

    assert_eq!(
        plan.changes,
        [install(&["alpha"], Kind::Skill, "review", "workspace")]
    );
    assert_eq!(
        plan.conflicts,
        [Conflict {
            project: String::from("alpha"),
            kind: Kind::Skill,
            name: String::from("deploy"),
            cells: vec![asked[0].cell.clone(), asked[1].cell.clone()],
            why: Said {
                code: "asked_both_ways",
                message: String::from(
                    "`deploy` from workspace is asked both to be in alpha and not to be"
                ),
            },
        }]
    );
}

// ---------------------------------------------------------------------------
// Each lock
// ---------------------------------------------------------------------------

#[test]
fn a_change_to_somebodys_copy_is_locked_as_foreign_and_one_asking_for_it_is_no_change() {
    let asked = [
        // Somebody's, so every offer of its name is held: asking for one is
        // asking for what is there.
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Foreign,
            true,
            true,
        ),
        skill(
            "beta",
            "deploy",
            "workspace",
            Standing::Foreign,
            true,
            false,
        ),
        skill(
            "gamma",
            "deploy",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
    ];

    let plan = Plan::of(&asked);

    assert_eq!(
        plan.changes,
        [install(&["gamma"], Kind::Skill, "deploy", "workspace")]
    );
    assert_eq!(
        plan.locked,
        [Locked {
            cell: asked[1].cell.clone(),
            why: Said {
                code: "foreign",
                message: String::from(
                    "`deploy` is in beta, and Mindflayer did not put it there, so it is left alone"
                ),
            },
        }]
    );
}

#[test]
fn a_change_to_a_copy_edited_since_it_was_installed_is_locked_as_edited_either_way() {
    let asked = [
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Edited,
            true,
            false,
        ),
        // Another offer of the name in its place: the install writing over
        // the edit is held back, and so is the box it would replace.
        skill("beta", "deploy", "workspace", Standing::Edited, true, false),
        skill(
            "beta",
            "deploy",
            "load:../team",
            Standing::Edited,
            false,
            true,
        ),
    ];

    let plan = Plan::of(&asked);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec![], vec!["edited", "edited", "edited"]));
    assert_eq!(
        plan.locked
            .iter()
            .map(|locked| locked.cell.clone())
            .collect::<Vec<_>>(),
        [
            asked[0].cell.clone(),
            asked[1].cell.clone(),
            asked[2].cell.clone()
        ]
    );
    assert_eq!(
        plan.locked[0].why.message,
        "`deploy` in alpha was changed since it was installed, so it is left alone unless a \
         change to it is forced"
    );
}

#[test]
fn a_lock_is_the_code_the_command_refuses_the_change_with() {
    for (standing, refused) in [
        (Standing::Foreign, Refused::Foreign),
        (Standing::Edited, Refused::Edited),
    ] {
        let plan = Plan::of(&[skill("alpha", "deploy", "workspace", standing, true, false)]);
        assert_eq!(codes(&plan), (vec![], vec![refused.code()]), "{standing:?}");
    }
}

// ---------------------------------------------------------------------------
// What it serializes to
// ---------------------------------------------------------------------------

#[test]
fn a_plan_serializes_each_field_by_its_name() {
    let foreign = skill("alpha", "lint", "workspace", Standing::Foreign, true, false);
    let plan = Plan::of(&[
        of(
            "alpha",
            Kind::Rule,
            "git/commits",
            "workspace",
            Standing::Absent,
            false,
            true,
        ),
        skill(
            "alpha",
            "deploy",
            "workspace",
            Standing::Current,
            true,
            false,
        ),
        foreign,
        skill("beta", "review", "workspace", Standing::Absent, false, true),
        skill(
            "beta",
            "review",
            "load:../team",
            Standing::Absent,
            false,
            true,
        ),
    ]);

    assert_eq!(
        serde_json::to_value(&plan).unwrap(),
        json!({
            "changes": [
                {"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "deploy"},
                {
                    "type": "install",
                    "projects": ["alpha"],
                    "kind": "rule",
                    "name": "git/commits",
                    "from": "workspace",
                },
            ],
            "conflicts": [{
                "project": "beta",
                "kind": "skill",
                "name": "review",
                "cells": [
                    {"project": "beta", "reference": "skill:review", "origin": "workspace", "wanted": true},
                    {"project": "beta", "reference": "skill:review", "origin": "load:../team", "wanted": true},
                ],
                "why": {
                    "code": "wanted_twice",
                    "message": "`review` is asked of beta from more than one place (workspace, \
                                load:../team), and a project holds one skill of a name: ask for one",
                },
            }],
            "locked": [{
                "cell": {"project": "alpha", "reference": "skill:lint", "origin": "workspace", "wanted": false},
                "why": {
                    "code": "foreign",
                    "message": "`lint` is in alpha, and Mindflayer did not put it there, so it is left alone",
                },
            }],
        })
    );
}

#[test]
fn a_cell_is_read_by_its_names_and_refuses_one_it_does_not_have() {
    let cell: Cell = serde_json::from_value(json!({
        "project": "alpha",
        "reference": "rule:git/commits",
        "origin": "load:../team",
        "wanted": false,
    }))
    .unwrap();
    assert_eq!(
        cell,
        Cell::of("alpha", Kind::Rule, "git/commits", "load:../team", false)
    );

    let misspelled = serde_json::from_value::<Cell>(json!({
        "project": "alpha",
        "reference": "skill:deploy",
        "origin": "workspace",
        "wanted": true,
        "force": true,
    }));
    assert!(misspelled.is_err(), "{misspelled:?}");
}
