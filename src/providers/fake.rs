//! Synthetic provider used to validate Auryn's architecture end-to-end before
//! any real provider scanner exists (spec Phase 1).
//!
//! It serves two purposes. With no configuration it returns a small set of
//! built-in sessions so `auryn list` produces output immediately. Pointed at a
//! directory of JSON session files (via `AURYN_FAKE_DIR` or [`FakeProvider::new`])
//! it exercises the real scanning path: bounded file reads, tolerant parsing,
//! and preview truncation. It never represents real user data and is gated
//! behind the `AURYN_FAKE` environment variable.

use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::errors::Result;
use crate::models::{MessagePreview, ProviderKind, Role, Session};

/// Environment variable that enables the fake provider.
const ENABLE_VAR: &str = "AURYN_FAKE";

/// Environment variable pointing at a directory of JSON session fixtures.
const DIR_VAR: &str = "AURYN_FAKE_DIR";

/// Returns true when the fake provider should be registered.
pub fn fake_enabled() -> bool {
    matches!(
        env::var(ENABLE_VAR).ok().as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

/// Provider that yields synthetic sessions for architecture validation.
pub struct FakeProvider {
    /// Optional directory of JSON session fixtures. When `None` or absent the
    /// provider falls back to its built-in sample sessions.
    root: Option<PathBuf>,
}

impl FakeProvider {
    /// Creates a provider that reads from `root` if given, else uses built-ins.
    pub fn new(root: Option<PathBuf>) -> Self {
        FakeProvider { root }
    }

    /// Creates a provider configured from `AURYN_FAKE_DIR`.
    pub fn from_env() -> Self {
        FakeProvider::new(env::var_os(DIR_VAR).map(PathBuf::from))
    }

    /// Reads and normalizes every JSON session file under `dir`, skipping any
    /// file that is too large or fails to parse so a single corrupt fixture
    /// cannot break discovery.
    fn scan_dir(&self, dir: &PathBuf, config: &AppConfig) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.len() > config.max_file_bytes {
                // Bound file sizes: skip pathological inputs rather than read them.
                continue;
            }
            let raw = match std::fs::read_to_string(&path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let parsed: FakeSessionFile = match serde_json::from_str(&raw) {
                Ok(p) => p,
                Err(_) => continue, // Ignore malformed content safely.
            };
            sessions.push(parsed.into_session(path, config.preview_turns));
        }
        Ok(sessions)
    }
}

impl crate::providers::Provider for FakeProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Fake
    }

    fn display_name(&self) -> &'static str {
        "Fake"
    }

    fn default_root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn scan(&self, config: &AppConfig) -> Result<Vec<Session>> {
        match &self.root {
            Some(dir) if dir.is_dir() => self.scan_dir(dir, config),
            _ => Ok(builtin_sessions(config.preview_turns)),
        }
    }

    fn resume_command(&self, session: &Session, _config: &AppConfig) -> Result<Command> {
        // The fake provider cannot resume anything real, so it builds a benign,
        // shell-free command that simply echoes the target session id. This
        // exercises the launcher plumbing without side effects.
        let mut cmd = Command::new("echo");
        cmd.arg(format!("resume {}", session.id));
        Ok(cmd)
    }
}

/// On-disk JSON shape for a fake session fixture.
#[derive(Debug, Deserialize)]
struct FakeSessionFile {
    id: String,
    name: String,
    #[serde(default)]
    project_path: Option<PathBuf>,
    #[serde(default)]
    date_began: Option<DateTime<Utc>>,
    #[serde(default)]
    date_last_used: Option<DateTime<Utc>>,
    #[serde(default)]
    message_count: Option<usize>,
    #[serde(default)]
    messages: Vec<FakeMessage>,
}

#[derive(Debug, Deserialize)]
struct FakeMessage {
    role: Role,
    content: String,
    #[serde(default)]
    timestamp: Option<DateTime<Utc>>,
}

impl FakeSessionFile {
    fn into_session(self, source_path: PathBuf, preview_turns: usize) -> Session {
        let message_count = self.message_count.unwrap_or(self.messages.len());
        let preview_messages = preview_from(self.messages, preview_turns);
        Session {
            id: Session::make_id(ProviderKind::Fake, &self.id),
            provider: ProviderKind::Fake,
            provider_session_id: self.id,
            session_name: self.name,
            project_path: self.project_path,
            date_began: self.date_began,
            date_last_used: self.date_last_used,
            message_count,
            preview_messages,
            source_path,
        }
    }
}

/// Keeps only the last `turns` messages for preview, preserving order.
fn preview_from(messages: Vec<FakeMessage>, turns: usize) -> Vec<MessagePreview> {
    let start = messages.len().saturating_sub(turns);
    messages
        .into_iter()
        .skip(start)
        .map(|m| MessagePreview {
            role: m.role,
            content: m.content,
            timestamp: m.timestamp,
        })
        .collect()
}

/// Built-in sample sessions mirroring the spec's UI mockup, used when no
/// fixture directory is configured. They are constructed through the same
/// [`FakeSessionFile::into_session`] path as on-disk fixtures.
fn builtin_sessions(preview_turns: usize) -> Vec<Session> {
    [
        FakeSessionFile {
            id: "alpha-notes-0001".to_string(),
            name: "Alpha Notes".to_string(),
            project_path: Some(PathBuf::from("/home/dev/projects/alpha")),
            date_began: Some(at(2026, 6, 2, 9, 15)),
            date_last_used: Some(at(2026, 6, 10, 11, 55)),
            message_count: Some(87),
            messages: turns(&[
                (Role::User, "Can you outline the next steps for this project?"),
                (
                    Role::Assistant,
                    "Here are the next steps, grouped by priority so we can tackle them in order.",
                ),
                (Role::User, "Which item should we start with first?"),
                (
                    Role::Assistant,
                    "I recommend starting with the highest-priority item before the rest.",
                ),
            ]),
        },
        FakeSessionFile {
            id: "service-setup-0002".to_string(),
            name: "Service Setup".to_string(),
            project_path: Some(PathBuf::from("/home/dev/projects/service")),
            date_began: Some(at(2026, 6, 5, 14, 0)),
            date_last_used: Some(at(2026, 6, 10, 11, 0)),
            message_count: Some(22),
            messages: turns(&[
                (Role::User, "How should the service handle its configuration?"),
                (
                    Role::Assistant,
                    "Store configuration in a single file with sensible defaults for each setting.",
                ),
            ]),
        },
        FakeSessionFile {
            id: "article-outline-0003".to_string(),
            name: "Article Outline".to_string(),
            project_path: Some(PathBuf::from("/home/dev/writing/article")),
            date_began: Some(at(2026, 6, 7, 8, 30)),
            date_last_used: Some(at(2026, 6, 9, 12, 0)),
            message_count: Some(14),
            messages: turns(&[
                (Role::User, "Draft an outline for an article on developer tools."),
                (
                    Role::Assistant,
                    "Here is a five-section outline to start from, beginning with the introduction.",
                ),
            ]),
        },
    ]
    .into_iter()
    .map(|file| {
        let source = PathBuf::from(format!("<builtin>/{}.json", file.id));
        file.into_session(source, preview_turns)
    })
    .collect()
}

/// Fixed UTC timestamp helper for built-in sample data.
fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
}

/// Builds preview-less [`FakeMessage`] turns from role/content pairs.
fn turns(pairs: &[(Role, &str)]) -> Vec<FakeMessage> {
    pairs
        .iter()
        .map(|(role, content)| FakeMessage {
            role: *role,
            content: content.to_string(),
            timestamp: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Provider;

    #[test]
    fn builtin_scan_returns_sessions_when_no_root() {
        let provider = FakeProvider::new(None);
        let sessions = provider.scan(&AppConfig::default()).unwrap();
        assert_eq!(sessions.len(), 3);
        assert!(sessions.iter().all(|s| s.provider == ProviderKind::Fake));
    }

    #[test]
    fn preview_is_truncated_to_configured_turns() {
        let config = AppConfig {
            preview_turns: 2,
            ..AppConfig::default()
        };
        let provider = FakeProvider::new(None);
        let sessions = provider.scan(&config).unwrap();
        let alpha = sessions
            .iter()
            .find(|s| s.session_name == "Alpha Notes")
            .unwrap();
        // The Alpha sample has 4 turns; only the last 2 should survive.
        assert_eq!(alpha.preview_messages.len(), 2);
        assert_eq!(alpha.preview_messages[0].role, Role::User);
        assert!(
            alpha.preview_messages[0]
                .content
                .contains("Which item should we start with")
        );
    }

    #[test]
    fn resume_command_is_shell_free_and_references_session() {
        let provider = FakeProvider::new(None);
        let session = provider.scan(&AppConfig::default()).unwrap().remove(0);
        let cmd = provider
            .resume_command(&session, &AppConfig::default())
            .unwrap();
        assert_eq!(cmd.get_program(), "echo");
        let args: Vec<_> = cmd.get_args().collect();
        assert!(args[0].to_string_lossy().contains(&session.id));
    }
}
