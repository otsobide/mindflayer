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

#[test]
fn a_time_is_said_in_utc_as_rfc_3339_writes_it() {
    for (seconds, said) in [
        (0, "1970-01-01T00:00:00Z"),
        (59, "1970-01-01T00:00:59Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_709_164_800, "2024-02-29T00:00:00Z"),
        (1_798_761_599, "2026-12-31T23:59:59Z"),
        (1_798_761_600, "2027-01-01T00:00:00Z"),
        (4_102_444_800, "2100-01-01T00:00:00Z"),
        (-1, "1969-12-31T23:59:59Z"),
    ] {
        assert_eq!(Utc::of(seconds).to_string(), said, "{seconds}");
    }
}

#[test]
fn every_day_follows_the_one_before_it() {
    // Day by day across four centuries, a leap one among them, each a day
    // after the last: no date is said twice, and none is skipped.
    let mut last = Utc::of(0);
    for day in 1..146_097_i64 {
        let said = Utc::of(day * 86_400);
        assert!(said > last, "{said} after {last}");
        assert!(
            (1..=12).contains(&said.month) && (1..=31).contains(&said.day),
            "{said}"
        );
        last = said;
    }
}

#[test]
fn what_is_said_reads_as_a_toml_date_and_time() {
    let said = Utc::of(1_762_507_800).to_string();

    let read: toml::value::Datetime = said.parse().unwrap();

    assert_eq!(read.to_string(), said);
}
