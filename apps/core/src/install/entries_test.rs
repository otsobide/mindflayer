//! Unit tests for `entries.rs`, kept beside it rather than inside it.

use super::*;

use crate::mother::ContentHashMother;

#[test]
fn an_entry_stands_by_its_record_what_its_file_holds_and_what_is_offered() {
    let written = ContentHashMother::random();
    let other = ContentHashMother::random();
    let offering = |offered: &[Option<&ContentHash>]| {
        Offered::Entries(offered.iter().map(|hash| hash.cloned()).collect())
    };
    let current = offering(&[Some(&written)]);

    for (recorded, held, offered, standing) in [
        (None, None, &current, Standing::Absent),
        (None, Some(&written), &current, Standing::Foreign),
        (Some(&written), Some(&written), &current, Standing::Current),
        (Some(&written), Some(&other), &current, Standing::Edited),
        (Some(&written), None, &current, Standing::Missing),
        (
            Some(&written),
            Some(&written),
            &offering(&[Some(&other)]),
            Standing::Outdated,
        ),
        // An offer the agent would be given no entry of is no longer what
        // was written.
        (
            Some(&written),
            Some(&written),
            &offering(&[None]),
            Standing::Outdated,
        ),
        // One place of several giving what was written is enough.
        (
            Some(&written),
            Some(&written),
            &offering(&[Some(&other), Some(&written)]),
            Standing::Current,
        ),
        (
            Some(&written),
            Some(&written),
            &Offered::Nothing,
            Standing::Orphaned,
        ),
        // Edited over orphaned, and orphaned over missing, as for a copy.
        (
            Some(&written),
            Some(&other),
            &Offered::Nothing,
            Standing::Edited,
        ),
        (Some(&written), None, &Offered::Nothing, Standing::Orphaned),
        (
            Some(&written),
            None,
            &offering(&[Some(&other)]),
            Standing::Missing,
        ),
    ] {
        assert_eq!(
            of_entry(recorded, held, offered),
            standing,
            "{recorded:?}, {held:?}"
        );
    }
}

#[test]
fn an_installation_is_said_as_the_first_of_edited_orphaned_missing_and_outdated() {
    use Standing::{Current, Edited, Missing, Orphaned, Outdated};

    for (entries, offered, standing) in [
        (&[Current, Current, Current][..], true, Current),
        (&[Current, Outdated, Current], true, Outdated),
        (&[Outdated, Missing, Current], true, Missing),
        (&[Missing, Edited, Outdated], true, Edited),
        (&[Orphaned, Orphaned], false, Orphaned),
        (&[Orphaned, Edited], false, Edited),
        // One recording no entry has nothing to fall behind, or to miss.
        (&[], true, Current),
        (&[], false, Orphaned),
    ] {
        assert_eq!(
            of_installation(entries, offered),
            standing,
            "{entries:?}, offered: {offered}"
        );
    }
}
