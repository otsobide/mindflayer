//! Unit tests for `render.rs`, kept beside it rather than inside it.

use super::*;

#[test]
fn a_time_is_said_as_its_date_and_minute_in_utc() {
    for (seconds, said) in [
        (0, "1970-01-01 00:00 UTC"),
        (59, "1970-01-01 00:00 UTC"),
        (951_782_400, "2000-02-29 00:00 UTC"),
        (1_709_164_800, "2024-02-29 00:00 UTC"),
        (1_798_761_599, "2026-12-31 23:59 UTC"),
        (1_798_761_600, "2027-01-01 00:00 UTC"),
        (4_102_444_800, "2100-01-01 00:00 UTC"),
        (-1, "1969-12-31 23:59 UTC"),
    ] {
        assert_eq!(utc(seconds), said, "{seconds}");
    }
}

#[test]
fn every_day_follows_the_one_before_it() {
    // Day by day across four centuries, a leap one among them, each a day
    // after the last: no date is said twice, and none is skipped.
    let mut last = utc(0);
    for day in 1..146_097_i64 {
        let said = utc(day * 86_400);
        assert!(said > last, "{said} after {last}");
        let (date, _) = said.split_once(' ').unwrap();
        let parts: Vec<u32> = date.split('-').map(|part| part.parse().unwrap()).collect();
        assert!(
            (1..=12).contains(&parts[1]) && (1..=31).contains(&parts[2]),
            "{said}"
        );
        last = said;
    }
}
