use super::*;

fn looked(found: &[Found]) -> Vec<Looked> {
    found
        .iter()
        .enumerate()
        .map(|(index, found)| Looked {
            path: PathBuf::from(format!("copy-{index}")),
            found: *found,
        })
        .collect()
}

#[test]
fn an_edit_is_said_over_everything_else_true_of_an_installation() {
    let copies = looked(&[Found::Missing, Found::Edited, Found::Untouched]);
    assert_eq!(standing(&copies, false, |_| false), Standing::Edited);
    assert_eq!(standing(&copies, true, |_| true), Standing::Edited);
}

#[test]
fn an_offer_gone_is_said_over_a_copy_gone_and_one_behind() {
    let copies = looked(&[Found::Missing, Found::Untouched]);
    assert_eq!(standing(&copies, false, |_| false), Standing::Orphaned);
    assert_eq!(standing(&copies, true, |_| false), Standing::Missing);
    assert_eq!(
        standing(&looked(&[Found::Missing]), true, |_| true),
        Standing::Missing,
        "every copy gone is missing, not absent: the ledger still records it"
    );
}

#[test]
fn untouched_copies_are_current_or_behind_by_what_is_offered_now() {
    let copies = looked(&[Found::Untouched, Found::Untouched]);
    assert_eq!(standing(&copies, true, |_| true), Standing::Current);
    assert_eq!(standing(&copies, true, |_| false), Standing::Outdated);
}

#[test]
fn an_installation_recording_no_copy_has_nothing_to_fall_behind() {
    assert_eq!(standing(&[], true, |_| false), Standing::Current);
    assert_eq!(standing(&[], false, |_| false), Standing::Orphaned);
}

#[test]
fn only_somebodys_work_and_an_edit_are_not_mindflayers_to_change() {
    for standing in [
        Standing::Current,
        Standing::Outdated,
        Standing::Missing,
        Standing::Orphaned,
        Standing::Absent,
    ] {
        assert!(standing.ours(), "{standing:?}");
    }
    assert!(!Standing::Edited.ours());
    assert!(!Standing::Foreign.ours());
    assert!(!Standing::Absent.present());
}

#[test]
fn what_is_not_mindflayers_to_change_is_refused_by_the_code_a_change_reaching_it_gives() {
    assert_eq!(Standing::Foreign.refused(), Some(Refused::Foreign));
    assert_eq!(Standing::Edited.refused(), Some(Refused::Edited));
    for standing in [
        Standing::Absent,
        Standing::Current,
        Standing::Outdated,
        Standing::Missing,
        Standing::Orphaned,
    ] {
        assert_eq!(standing.refused(), None, "{standing:?}");
    }
}

#[test]
fn an_installation_with_a_copy_gone_is_not_shown_as_held() {
    // Its box opens empty, for marking it to put every copy back.
    assert!(!Standing::Missing.present());
    for standing in [
        Standing::Current,
        Standing::Outdated,
        Standing::Edited,
        Standing::Orphaned,
        Standing::Foreign,
    ] {
        assert!(standing.present(), "{standing:?}");
    }
}

#[test]
fn a_standing_is_said_in_the_word_status_says_it_in() {
    use crate::status::State;

    for (standing, state) in [
        (Standing::Current, State::Current),
        (Standing::Outdated, State::Outdated),
        (Standing::Edited, State::Edited),
        (Standing::Missing, State::Missing),
        (Standing::Orphaned, State::Orphaned),
        (Standing::Foreign, State::Foreign),
    ] {
        assert_eq!(standing.slug(), state.slug());
    }
    assert_eq!(Standing::Absent.slug(), "absent");
}
