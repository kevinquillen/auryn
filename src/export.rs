//! Rendering a fully-read session to a portable Markdown or JSON document.
//!
//! [`Transcript`] bundles a session's metadata with its complete, untruncated
//! conversation (as read by [`crate::providers::Provider::read_messages`]) and
//! renders it independently of which provider produced it. The output is a
//! portable record for archiving or seeding a new session in another tool; it
//! never resumes or writes provider state.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::errors::{AurynError, Result};
use crate::models::{MessagePreview, Session};

/// The format an export is rendered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable Markdown with a metadata header and one heading per turn.
    Markdown,
    /// Machine-readable JSON with metadata and a messages array.
    Json,
}

/// A fully-read session ready to render: its metadata plus every readable turn,
/// untruncated and in chronological order.
pub struct Transcript {
    pub session: Session,
    pub messages: Vec<MessagePreview>,
}

impl Transcript {
    /// Renders the transcript in the requested format.
    pub fn render(&self, format: Format) -> Result<String> {
        match format {
            Format::Markdown => Ok(self.to_markdown()),
            Format::Json => self.to_json(),
        }
    }

    /// Renders Markdown: a metadata header followed by a `## Role` heading and
    /// the content for each turn. Turns are separated by headings rather than
    /// horizontal rules so the document stays clean when re-parsed.
    fn to_markdown(&self) -> String {
        let s = &self.session;
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", s.session_name));

        out.push_str(&format!("- Provider: {}\n", s.provider.display_name()));
        out.push_str(&format!("- Session: {}\n", s.id));
        if let Some(path) = &s.project_path {
            out.push_str(&format!("- Project: {}\n", path.display()));
        }
        if let Some(when) = s.date_began {
            out.push_str(&format!("- Began: {}\n", stamp(when)));
        }
        if let Some(when) = s.date_last_used {
            out.push_str(&format!("- Last used: {}\n", stamp(when)));
        }
        out.push_str(&format!("- Messages: {}\n", self.messages.len()));

        for message in &self.messages {
            out.push_str(&format!("\n## {}\n\n", message.role.label()));
            out.push_str(message.content.trim_end());
            out.push('\n');
        }
        out
    }

    /// Renders JSON: session metadata plus the full messages array. Optional
    /// metadata fields are omitted when absent rather than emitted as null.
    fn to_json(&self) -> Result<String> {
        let s = &self.session;
        let doc = JsonDocument {
            id: &s.id,
            provider: s.provider.as_str(),
            session_name: &s.session_name,
            project_path: s.project_path.as_ref().map(|p| p.display().to_string()),
            date_began: s.date_began.map(stamp_rfc3339),
            date_last_used: s.date_last_used.map(stamp_rfc3339),
            message_count: self.messages.len(),
            messages: &self.messages,
        };
        serde_json::to_string_pretty(&doc).map_err(AurynError::from)
    }
}

/// The on-disk JSON shape an export produces.
#[derive(Serialize)]
struct JsonDocument<'a> {
    id: &'a str,
    provider: &'a str,
    session_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_began: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_last_used: Option<String>,
    message_count: usize,
    messages: &'a [MessagePreview],
}

/// Formats a timestamp for the Markdown header in a compact, unambiguous form.
fn stamp(when: DateTime<Utc>) -> String {
    when.format("%Y-%m-%d %H:%M UTC").to_string()
}

/// Formats a timestamp as RFC 3339 for machine-readable JSON output.
fn stamp_rfc3339(when: DateTime<Utc>) -> String {
    when.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MessagePreview, ProviderKind, Role, Session};
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn transcript() -> Transcript {
        let session = Session {
            id: "claude:abc".to_string(),
            provider: ProviderKind::Claude,
            provider_session_id: "abc".to_string(),
            session_name: "Open Tasks Review".to_string(),
            project_path: Some(PathBuf::from("/home/dev/projects/alpha")),
            date_began: Some(Utc.with_ymd_and_hms(2026, 6, 1, 10, 0, 0).unwrap()),
            date_last_used: Some(Utc.with_ymd_and_hms(2026, 6, 8, 16, 30, 0).unwrap()),
            message_count: 2,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/abc.jsonl"),
        };
        let messages = vec![
            MessagePreview {
                role: Role::User,
                content: "Can you review the open tasks?".to_string(),
                timestamp: None,
            },
            MessagePreview {
                role: Role::Assistant,
                content: "I grouped them by priority.".to_string(),
                timestamp: None,
            },
        ];
        Transcript { session, messages }
    }

    #[test]
    fn markdown_has_header_and_one_heading_per_turn() {
        let md = transcript().render(Format::Markdown).unwrap();
        assert!(md.contains("# Open Tasks Review"));
        assert!(md.contains("- Provider: Claude"));
        assert!(md.contains("- Session: claude:abc"));
        assert!(md.contains("- Project: /home/dev/projects/alpha"));
        assert!(md.contains("- Messages: 2"));
        assert!(md.contains("## User"));
        assert!(md.contains("Can you review the open tasks?"));
        assert!(md.contains("## Assistant"));
        // No horizontal rules are used to separate turns.
        assert!(!md.contains("\n---\n"));
    }

    #[test]
    fn json_is_parseable_with_metadata_and_messages() {
        let json = transcript().render(Format::Json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["id"], "claude:abc");
        assert_eq!(value["provider"], "claude");
        assert_eq!(value["session_name"], "Open Tasks Review");
        assert_eq!(value["message_count"], 2);
        assert_eq!(value["date_began"], "2026-06-01T10:00:00Z");
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Can you review the open tasks?");
    }

    #[test]
    fn json_omits_absent_optional_metadata() {
        let mut t = transcript();
        t.session.project_path = None;
        t.session.date_began = None;
        let json = t.render(Format::Json).unwrap();
        assert!(!json.contains("project_path"));
        assert!(!json.contains("date_began"));
    }
}
