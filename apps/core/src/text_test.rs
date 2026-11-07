//! Unit tests for `text.rs`.

use super::*;

#[test]
fn a_control_character_is_shown_as_a_space() {
    assert_eq!(printable("one\rtwo\nthree\u{1b}[2J"), "one two three [2J");
}

#[test]
fn the_ends_are_trimmed_and_the_rest_kept() {
    assert_eq!(printable("\t  a name, as it is \n"), "a name, as it is");
    assert_eq!(printable("ünïcode stays"), "ünïcode stays");
    assert_eq!(printable(""), "");
}
