//! The draws every mother is made of: a mother is only as good as the range
//! it draws from, so a draw that stopped covering its range would let every
//! test built on it pass on fewer cases than it says.

use super::*;

const DRAWS: usize = 2000;

#[test]
fn a_number_between_two_bounds_stays_inside_them_and_reaches_both() {
    let drawn: Vec<usize> = (0..DRAWS).map(|_| between(3, 7)).collect();

    assert!(drawn.iter().all(|n| (3..=7).contains(n)), "{drawn:?}");
    for bound in [3, 7] {
        assert!(drawn.contains(&bound), "{bound} never drawn");
    }
    assert_eq!(between(4, 4), 4);
}

#[test]
fn two_numbers_drawn_one_after_the_other_differ() {
    let drawn: std::collections::BTreeSet<u64> = (0..DRAWS).map(|_| number()).collect();

    assert_eq!(drawn.len(), DRAWS);
}

#[test]
fn a_chance_comes_up_both_ways() {
    let drawn: Vec<bool> = (0..DRAWS).map(|_| chance()).collect();

    assert!(drawn.contains(&true));
    assert!(drawn.contains(&false));
}

#[test]
fn a_pick_is_one_of_what_it_was_given_and_reaches_each() {
    let items = ["a", "b", "c"];
    let drawn: Vec<&str> = (0..DRAWS).map(|_| pick(&items)).collect();

    for item in items {
        assert!(drawn.contains(&item), "{item} never picked");
    }
}

#[test]
#[should_panic(expected = "nothing to pick from")]
fn nothing_is_picked_from_nothing() {
    pick::<u8>(&[]);
}

#[test]
fn some_of_a_list_keeps_its_order_and_may_be_none_or_all() {
    let items = [1, 2, 3];
    let drawn: Vec<Vec<i32>> = (0..DRAWS).map(|_| some_of(&items)).collect();

    assert!(drawn
        .iter()
        .all(|some| some.windows(2).all(|pair| pair[0] < pair[1])));
    assert!(drawn.iter().any(Vec::is_empty));
    assert!(drawn.iter().any(|some| some.len() == items.len()));
}

#[test]
fn hex_digits_are_lowercase_and_as_many_as_asked() {
    for length in [0, 1, 32, 64] {
        let digits = hex(length);

        assert_eq!(digits.len(), length);
        assert!(
            digits.bytes().all(|b| HEX.as_bytes().contains(&b)),
            "{digits}"
        );
    }
}

#[test]
fn a_kebab_name_is_held_to_the_rule_a_skills_name_is() {
    for _ in 0..DRAWS {
        let name = kebab();

        assert!(
            crate::artifact::name_issues(&name).is_empty(),
            "{name:?} breaks the name rule"
        );
    }
}

#[test]
fn any_text_reaches_beyond_ascii_and_stays_under_its_bound() {
    let drawn: Vec<String> = (0..DRAWS).map(|_| text(12)).collect();

    assert!(drawn.iter().all(|text| text.chars().count() <= 12));
    assert!(drawn.iter().any(|text| !text.is_ascii()));
    assert!(drawn.iter().any(String::is_empty));
}

#[test]
fn bytes_reach_beyond_utf8() {
    let drawn = bytes(4096);

    assert_eq!(drawn.len(), 4096);
    assert!(std::str::from_utf8(&drawn).is_err());
}
