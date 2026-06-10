//! Human-friendly date and time rendering.
//!
//! Formatting is parameterized on an explicit `now` so the relative-time logic
//! is deterministic and unit-testable without mocking the system clock.

use chrono::{DateTime, Datelike, Utc};

/// Renders an absolute calendar date like `Jun 2, 2026`.
pub fn absolute_date(when: DateTime<Utc>) -> String {
    // `%-d` drops the leading zero on the day, matching the spec's mockup.
    when.format("%b %-d, %Y").to_string()
}

/// Renders a timestamp relative to `now` using coarse, human buckets:
/// `just now`, `N minutes ago`, `N hours ago`, `Yesterday`, `N days ago`,
/// and finally an absolute date once it is more than a week old.
pub fn relative_time(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(when);
    let seconds = delta.num_seconds();

    if seconds < 0 {
        // Clock skew or a future timestamp; fall back to the absolute date.
        return absolute_date(when);
    }
    if seconds < 60 {
        return "just now".to_string();
    }

    let minutes = delta.num_minutes();
    if minutes < 60 {
        return plural(minutes, "minute");
    }

    let hours = delta.num_hours();
    if hours < 24 {
        return plural(hours, "hour");
    }

    let days = delta.num_days();
    if days == 1 {
        return "Yesterday".to_string();
    }
    if days < 7 {
        return plural(days, "day");
    }

    absolute_date(when)
}

/// Renders an optional timestamp, using a stable placeholder when absent.
pub fn relative_time_opt(when: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    match when {
        Some(w) => relative_time(w, now),
        None => "-".to_string(),
    }
}

/// Renders an optional absolute date, using a stable placeholder when absent.
pub fn absolute_date_opt(when: Option<DateTime<Utc>>) -> String {
    match when {
        Some(w) => absolute_date(w),
        None => "-".to_string(),
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}

/// True when `when` falls on the same calendar day (UTC) as `now`.
pub fn is_same_day(when: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    when.year() == now.year() && when.ordinal() == now.ordinal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn absolute_date_drops_leading_zero() {
        assert_eq!(absolute_date(utc(2026, 6, 2, 10, 0)), "Jun 2, 2026");
        assert_eq!(absolute_date(utc(2026, 12, 25, 0, 0)), "Dec 25, 2026");
    }

    #[test]
    fn relative_time_buckets() {
        let now = utc(2026, 6, 10, 12, 0);
        assert_eq!(relative_time(now, now), "just now");
        assert_eq!(
            relative_time(utc(2026, 6, 10, 11, 55), now),
            "5 minutes ago"
        );
        assert_eq!(relative_time(utc(2026, 6, 10, 11, 0), now), "1 hour ago");
        assert_eq!(relative_time(utc(2026, 6, 10, 9, 0), now), "3 hours ago");
        assert_eq!(relative_time(utc(2026, 6, 9, 12, 0), now), "Yesterday");
        assert_eq!(relative_time(utc(2026, 6, 7, 12, 0), now), "3 days ago");
    }

    #[test]
    fn relative_time_falls_back_to_absolute_when_old() {
        let now = utc(2026, 6, 10, 12, 0);
        assert_eq!(relative_time(utc(2026, 6, 1, 12, 0), now), "Jun 1, 2026");
    }

    #[test]
    fn future_timestamps_render_as_absolute_date() {
        let now = utc(2026, 6, 10, 12, 0);
        assert_eq!(relative_time(utc(2026, 6, 11, 12, 0), now), "Jun 11, 2026");
    }

    #[test]
    fn singular_units_are_not_pluralized() {
        let now = utc(2026, 6, 10, 12, 0);
        assert_eq!(
            relative_time(utc(2026, 6, 10, 11, 58), now),
            "2 minutes ago"
        );
        assert_eq!(relative_time(utc(2026, 6, 10, 11, 59), now), "1 minute ago");
    }
}
