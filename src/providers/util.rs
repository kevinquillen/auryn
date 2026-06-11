//! Shared parsing helpers for file-based providers.
//!
//! Providers that read line-delimited session logs (Claude, Codex, and later
//! Gemini) need the same small primitives: collapsing whitespace for one-line
//! display, bounding preview text, parsing RFC 3339 timestamps, and tracking a
//! min/max timestamp range. They live here so each provider stays focused on
//! its own on-disk shape.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};

/// Sets `command`'s working directory to `project_path`, but only when it is an
/// existing directory that canonicalizes successfully. The project path comes
/// from untrusted session files, so a missing, non-directory, or unresolvable
/// path is ignored rather than used; the provider CLI then launches in the
/// inherited working directory instead of an attacker-influenced one.
pub(crate) fn apply_working_dir(command: &mut Command, project_path: Option<&Path>) {
    if let Some(path) = project_path
        && let Ok(canonical) = std::fs::canonicalize(path)
        && canonical.is_dir()
    {
        command.current_dir(canonical);
    }
}

/// Collapses all runs of whitespace (including newlines) into single spaces so
/// a value can be shown on one table row.
pub(crate) fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncates `text` to at most `max` characters, appending an ellipsis when cut.
pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('\u{2026}');
    out
}

/// Parses an RFC 3339 timestamp into UTC, tolerating fractional seconds and a
/// trailing `Z`.
pub(crate) fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Returns the earlier of an existing optional timestamp and a candidate.
pub(crate) fn min_opt(current: Option<DateTime<Utc>>, candidate: DateTime<Utc>) -> DateTime<Utc> {
    match current {
        Some(existing) if existing <= candidate => existing,
        _ => candidate,
    }
}

/// Returns the later of an existing optional timestamp and a candidate.
pub(crate) fn max_opt(current: Option<DateTime<Utc>>, candidate: DateTime<Utc>) -> DateTime<Utc> {
    match current {
        Some(existing) if existing >= candidate => existing,
        _ => candidate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_working_dir_uses_an_existing_directory() {
        let mut command = Command::new("true");
        apply_working_dir(&mut command, Some(Path::new(env!("CARGO_MANIFEST_DIR"))));
        assert!(command.get_current_dir().is_some());
    }

    #[test]
    fn apply_working_dir_ignores_missing_or_nonexistent_paths() {
        let mut command = Command::new("true");
        apply_working_dir(&mut command, None);
        assert!(command.get_current_dir().is_none());
        apply_working_dir(&mut command, Some(Path::new("/no/such/auryn/dir")));
        assert!(command.get_current_dir().is_none());
    }

    #[test]
    fn normalize_collapses_newlines_and_runs() {
        assert_eq!(normalize_whitespace("a\n  b\tc  "), "a b c");
    }

    #[test]
    fn truncate_appends_ellipsis_only_when_needed() {
        assert_eq!(truncate_chars("short", 10), "short");
        assert_eq!(truncate_chars("abcdef", 3), "abc\u{2026}");
    }

    #[test]
    fn timestamp_parsing_handles_fractional_seconds() {
        let ts = parse_timestamp("2026-06-10T13:38:10.261Z").unwrap();
        assert_eq!(
            ts.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-06-10T13:38:10.261Z"
        );
        assert!(parse_timestamp("not a date").is_none());
    }

    #[test]
    fn min_and_max_track_the_range() {
        let earlier = parse_timestamp("2026-06-01T00:00:00Z").unwrap();
        let later = parse_timestamp("2026-06-09T00:00:00Z").unwrap();
        assert_eq!(min_opt(None, later), later);
        assert_eq!(min_opt(Some(later), earlier), earlier);
        assert_eq!(max_opt(Some(earlier), later), later);
        assert_eq!(max_opt(Some(later), earlier), later);
    }
}
