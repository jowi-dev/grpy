//! Time windows for event queries.

use chrono::{DateTime, TimeZone, Utc};

/// A half-open window of time, `[start, end)`, used to ask a provider for
/// upcoming events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateRange {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl DateRange {
    /// Builds the window `[start, end)`. Returns `None` if `end` is before
    /// `start`; `start == end` is allowed and gives an empty window.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    /// First instant in the window.
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// First instant after the window.
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }

    /// Whether `instant` falls in the window, whatever its time zone.
    pub fn contains<Tz: TimeZone>(&self, instant: &DateTime<Tz>) -> bool {
        let instant = instant.to_utc();
        self.start <= instant && instant < self.end
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    fn october() -> DateRange {
        DateRange::new(utc("2026-10-01T00:00:00Z"), utc("2026-11-01T00:00:00Z")).unwrap()
    }

    #[test]
    fn end_before_start_is_rejected() {
        assert_eq!(
            DateRange::new(utc("2026-11-01T00:00:00Z"), utc("2026-10-01T00:00:00Z")),
            None
        );
    }

    #[test]
    fn start_is_inclusive_and_end_is_exclusive() {
        let range = october();
        assert!(range.contains(&range.start()));
        assert!(!range.contains(&range.end()));
    }

    #[test]
    fn empty_window_contains_nothing() {
        let t = utc("2026-10-01T00:00:00Z");
        let range = DateRange::new(t, t).unwrap();
        assert!(!range.contains(&t));
    }

    #[test]
    fn contains_compares_instants_across_offsets() {
        // 9pm Eastern on Oct 31 is 1am UTC on Nov 1, outside the window.
        let late_show = DateTime::parse_from_rfc3339("2026-10-31T21:00:00-04:00").unwrap();
        assert!(!october().contains(&late_show));

        // 7pm Eastern on Sep 30 is 11pm UTC, still before the window.
        let early_show = DateTime::parse_from_rfc3339("2026-09-30T19:00:00-04:00").unwrap();
        assert!(!october().contains(&early_show));

        let mid_month = DateTime::parse_from_rfc3339("2026-10-17T20:00:00-04:00").unwrap();
        assert!(october().contains(&mid_month));
    }
}
