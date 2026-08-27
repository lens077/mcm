use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Calendar date with day granularity and local semantics (spec Assumptions:
/// no timezone, no time-of-day).
pub type Date = NaiveDate;

/// Parses an ISO `YYYY-MM-DD` date. Returns `None` for any other shape so that
/// the parser can raise `P-003` with an exact location.
#[must_use]
pub fn parse_date(text: &str) -> Option<Date> {
    // `from_str` accepts some non-canonical spellings; require exact width.
    if text.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

#[must_use]
pub fn format_date(date: Date) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Inclusive calendar range used by explicit task schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DateRange {
    pub start: Date,
    pub end: Date,
}

impl DateRange {
    #[must_use]
    pub fn new(start: Date, end: Date) -> Self {
        Self { start, end }
    }

    /// True when `start <= end` (validated as `V-RANGE`).
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.start <= self.end
    }

    /// Smallest range covering both inputs — used to roll parent dates up from
    /// children (outline-grammar §时间推导 rule 3).
    #[must_use]
    pub fn envelope(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

#[must_use]
pub fn is_working_day(date: Date) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// First working day at or after `date`.
#[must_use]
pub fn next_working_day_on_or_after(date: Date) -> Date {
    let mut cursor = date;
    while !is_working_day(cursor) {
        cursor = cursor.succ_opt().unwrap_or(cursor);
    }
    cursor
}

/// First working day strictly after `date`.
#[must_use]
pub fn next_working_day_after(date: Date) -> Date {
    let mut cursor = date.succ_opt().unwrap_or(date);
    while !is_working_day(cursor) {
        cursor = cursor.succ_opt().unwrap_or(cursor);
    }
    cursor
}

/// End date of a task that starts on `start` and lasts `days` working days,
/// counting `start` itself as the first working day (`[1d]` ends the same day).
#[must_use]
pub fn working_day_end(start: Date, days: u32) -> Date {
    let mut cursor = next_working_day_on_or_after(start);
    let mut remaining = days.max(1);
    while remaining > 1 {
        cursor = next_working_day_after(cursor);
        remaining -= 1;
    }
    cursor
}

/// Number of working days in the inclusive range, used by timeline layout.
#[must_use]
pub fn working_days_between(range: DateRange) -> u32 {
    if !range.is_ordered() {
        return 0;
    }
    let mut count = 0;
    let mut cursor = range.start;
    while cursor <= range.end {
        if is_working_day(cursor) {
            count += 1;
        }
        let Some(next) = cursor.checked_add_days(Days::new(1)) else {
            break;
        };
        cursor = next;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Date {
        parse_date(text).expect("valid fixture date")
    }

    #[test]
    fn parses_only_canonical_iso_dates() {
        assert_eq!(
            parse_date("2026-09-01"),
            NaiveDate::from_ymd_opt(2026, 9, 1)
        );
        assert!(parse_date("2026-9-1").is_none());
        assert!(parse_date("2026-13-01").is_none());
        assert!(parse_date("not-a-date").is_none());
        assert!(parse_date("2026-09-011").is_none());
    }

    #[test]
    fn formats_round_trip() {
        let date = d("2026-02-28");
        assert_eq!(parse_date(&format_date(date)), Some(date));
    }

    #[test]
    fn weekend_detection_matches_monday_to_friday_rule() {
        assert!(is_working_day(d("2026-08-28"))); // Friday
        assert!(!is_working_day(d("2026-08-29"))); // Saturday
        assert!(!is_working_day(d("2026-08-30"))); // Sunday
        assert!(is_working_day(d("2026-08-31"))); // Monday
    }

    #[test]
    fn duration_of_one_day_ends_same_day() {
        assert_eq!(working_day_end(d("2026-09-01"), 1), d("2026-09-01"));
    }

    #[test]
    fn duration_skips_weekends() {
        // Thursday + 3 working days => Thu, Fri, Mon
        assert_eq!(working_day_end(d("2026-09-03"), 3), d("2026-09-07"));
    }

    #[test]
    fn duration_starting_on_weekend_shifts_to_monday() {
        assert_eq!(working_day_end(d("2026-08-29"), 1), d("2026-08-31"));
    }

    #[test]
    fn next_working_day_after_friday_is_monday() {
        assert_eq!(next_working_day_after(d("2026-08-28")), d("2026-08-31"));
    }

    #[test]
    fn envelope_covers_both_ranges() {
        let a = DateRange::new(d("2026-09-02"), d("2026-09-04"));
        let b = DateRange::new(d("2026-09-01"), d("2026-09-03"));
        assert_eq!(
            a.envelope(b),
            DateRange::new(d("2026-09-01"), d("2026-09-04"))
        );
    }

    #[test]
    fn containment_and_ordering() {
        let parent = DateRange::new(d("2026-09-01"), d("2026-09-30"));
        let child = DateRange::new(d("2026-09-05"), d("2026-09-10"));
        assert!(parent.contains(&child));
        assert!(!child.contains(&parent));
        assert!(parent.is_ordered());
        assert!(!DateRange::new(d("2026-09-10"), d("2026-09-01")).is_ordered());
    }

    #[test]
    fn counts_working_days_inclusive() {
        // Mon 2026-08-31 .. Sun 2026-09-06 => 5 working days
        assert_eq!(
            working_days_between(DateRange::new(d("2026-08-31"), d("2026-09-06"))),
            5
        );
        assert_eq!(
            working_days_between(DateRange::new(d("2026-09-10"), d("2026-09-01"))),
            0
        );
    }
}
