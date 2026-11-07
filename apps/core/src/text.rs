//! Text somebody else wrote, made fit to be shown as it is, and a time said
//! as text.
//!
//! A name, a description or a line of a catalog comes from a file or an
//! answer somebody else wrote, and what is shown of it is the same rule
//! wherever it is shown: the CLI's rows, and whatever core makes of a
//! catalog's text before a front end is handed it. One function in core
//! rather than one in each front end, which would let a front end and what
//! core hands it disagree about what a name reads as.

use std::fmt;

/// `text` with every control character made a space, and the ends trimmed.
///
/// A carriage return in a name would let a row overwrite the row above it
/// on a terminal, hiding an entry or fabricating one that looks real, and an
/// escape sequence would change what the terminal shows after it. A space
/// rather than nothing, so two words a line break kept apart stay apart.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A time, as its date and its time of day in UTC.
///
/// In UTC rather than the local zone: telling the local one needs the
/// system's database of zones, which a dependency would read. The day is
/// counted from the epoch's by the rule the Gregorian calendar keeps (Howard
/// Hinnant's `civil_from_days`), so nothing is asked of the system but the
/// number. Here rather than in each front end, since core writes one too:
/// when a definition imported from a registry was fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Utc {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Utc {
    /// `seconds` since the Unix epoch, as the ledger and a clock keep a time.
    pub fn of(seconds: i64) -> Self {
        let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
        // Days since 0000-03-01, in eras of 400 years, which repeat exactly.
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let of_era = shifted.rem_euclid(146_097);
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_from_march = (5 * of_year + 2) / 153;
        let day = of_year - (153 * month_from_march + 2) / 5 + 1;
        let month = if month_from_march < 10 {
            month_from_march + 3
        } else {
            month_from_march - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        // Each below its bound by the arithmetic above, so none is cut.
        Utc {
            year,
            month: month as u32,
            day: day as u32,
            hour: (of_day / 3600) as u32,
            minute: (of_day % 3600 / 60) as u32,
            second: (of_day % 60) as u32,
        }
    }
}

/// As RFC 3339 writes a time in UTC, `2026-11-07T09:30:00Z`: what TOML reads
/// as a date and time, and what a registry writes its own in.
impl fmt::Display for Utc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
