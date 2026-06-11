//! GitHub Copilot CLI session provider.
//!
//! Copilot stores one directory per session under
//! `~/.copilot/session-state/<session-id>/`, where the directory name is the
//! session id passed to `copilot --resume=<id>`. The conversation is an
//! `events.jsonl` log of `{ type, id, parentId, timestamp, data }` records;
//! `user.message` and `assistant.message` carry their text in `data.content`
//! (a plain string), and other event types (tool, hook, turn, session) are
//! skipped. A sibling `workspace.yaml` holds the session name, the working
//! directory, and the created/updated timestamps.
//!
//! Parsing is defensive and streaming, like the other providers: the event log
//! is size-bounded and read line-by-line, malformed lines are skipped, and only
//! the last N conversational turns are kept for preview.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use directories::BaseDirs;
use serde::Deserialize;
use serde_json::Value;

use crate::config::{AppConfig, ProviderSettings};
use crate::errors::Result;
use crate::models::{MessagePreview, ProviderKind, Role, Session};
use crate::providers::Provider;
use crate::providers::util::{
    apply_working_dir, max_opt, min_opt, normalize_whitespace, parse_timestamp, truncate_chars,
};

/// Environment override for the scan root, used for tests and non-standard
/// installs. Takes effect only when the config does not set an explicit root.
const DIR_VAR: &str = "AURYN_COPILOT_DIR";

/// The native CLI invoked to resume a session.
const RESUME_BIN: &str = "copilot";

/// Upper bound on retained preview text per message.
const MAX_PREVIEW_CHARS: usize = 2000;

/// Discovers and resumes GitHub Copilot CLI sessions.
pub struct CopilotProvider {
    root: Option<PathBuf>,
}

impl CopilotProvider {
    /// Creates a provider scanning `root`, or nothing when `root` is `None`.
    pub fn new(root: Option<PathBuf>) -> Self {
        CopilotProvider { root }
    }

    /// Creates a provider with the root resolved from `settings`, the
    /// environment, then the platform default.
    pub fn from_settings(settings: &ProviderSettings) -> Self {
        CopilotProvider::new(resolve_root(settings))
    }
}

/// Resolves the scan root: an explicit config root wins, then `AURYN_COPILOT_DIR`,
/// then the default `~/.copilot/session-state`.
pub fn resolve_root(settings: &ProviderSettings) -> Option<PathBuf> {
    if let Some(root) = &settings.root {
        return Some(root.clone());
    }
    if let Some(dir) = std::env::var_os(DIR_VAR) {
        return Some(PathBuf::from(dir));
    }
    default_root()
}

/// The platform-default Copilot session-state directory.
fn default_root() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(".copilot").join("session-state"))
}

impl Provider for CopilotProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Copilot
    }

    fn display_name(&self) -> &'static str {
        "Copilot"
    }

    fn default_root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn scan(&self, config: &AppConfig) -> Result<Vec<Session>> {
        let root = match &self.root {
            Some(root) if root.is_dir() => root,
            _ => return Ok(Vec::new()),
        };
        let mut sessions = Vec::new();
        for dir in session_dirs(root) {
            if let Some(session) = parse_session(&dir, config) {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    fn resume_command(&self, session: &Session, _config: &AppConfig) -> Result<Command> {
        // Shell-free: `copilot --resume=<id>` with the project directory as cwd.
        let mut command = Command::new(RESUME_BIN);
        command.arg(format!("--resume={}", session.provider_session_id));
        apply_working_dir(&mut command, session.project_path.as_deref());
        Ok(command)
    }
}

/// Returns the per-session directories directly under the session-state root.
fn session_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path);
            }
        }
    }
    out
}

/// The fields Auryn reads from a session's `workspace.yaml`.
#[derive(Default, Deserialize)]
struct Workspace {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

/// Reads `workspace.yaml`, returning empty metadata on any read or parse error.
fn read_workspace(path: &Path) -> Workspace {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_yaml_ng::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Accumulates conversational data across a session's event log.
#[derive(Default)]
struct Builder {
    first_user_text: Option<String>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    message_count: usize,
    preview: VecDeque<MessagePreview>,
}

/// Parses one session directory into a [`Session`], or `None` if the event log
/// is too large, unreadable, or has no conversational messages.
fn parse_session(dir: &Path, config: &AppConfig) -> Option<Session> {
    let id = dir.file_name()?.to_string_lossy().into_owned();
    let workspace = read_workspace(&dir.join("workspace.yaml"));

    let events = dir.join("events.jsonl");
    let metadata = std::fs::metadata(&events).ok()?;
    if metadata.len() > config.max_file_bytes {
        return None;
    }

    let reader = BufReader::new(File::open(&events).ok()?);
    let mut builder = Builder::default();
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => continue,
        };
        absorb_event(&mut builder, &record, config.preview_turns);
    }

    if builder.message_count == 0 {
        return None;
    }

    let session_name = workspace
        .name
        .filter(|n| !n.trim().is_empty())
        .map(|n| normalize_whitespace(&n))
        .or(builder.first_user_text)
        .unwrap_or_else(|| id.clone());

    let date_began = workspace
        .created_at
        .as_deref()
        .and_then(parse_timestamp)
        .or(builder.first_ts);
    let date_last_used = workspace
        .updated_at
        .as_deref()
        .and_then(parse_timestamp)
        .or(builder.last_ts)
        .or_else(|| metadata.modified().ok().map(DateTime::<Utc>::from));

    Some(Session {
        id: Session::make_id(ProviderKind::Copilot, &id),
        provider: ProviderKind::Copilot,
        provider_session_id: id,
        session_name,
        project_path: workspace.cwd.map(PathBuf::from),
        date_began,
        date_last_used,
        message_count: builder.message_count,
        preview_messages: builder.preview.into_iter().collect(),
        source_path: events,
    })
}

/// Folds a single event into the builder when it is a `user.message` or
/// `assistant.message` with readable text.
fn absorb_event(builder: &mut Builder, record: &Value, preview_turns: usize) {
    let timestamp = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    if let Some(ts) = timestamp {
        builder.first_ts = Some(min_opt(builder.first_ts, ts));
        builder.last_ts = Some(max_opt(builder.last_ts, ts));
    }

    let role = match record.get("type").and_then(Value::as_str) {
        Some("user.message") => Role::User,
        Some("assistant.message") => Role::Assistant,
        _ => return,
    };
    let content = record
        .get("data")
        .and_then(|data| data.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if content.trim().is_empty() {
        return;
    }

    if role == Role::User && builder.first_user_text.is_none() {
        builder.first_user_text = Some(truncate_chars(&normalize_whitespace(content), 80));
    }

    builder.message_count += 1;
    if preview_turns > 0 {
        if builder.preview.len() == preview_turns {
            builder.preview.pop_front();
        }
        builder.preview.push_back(MessagePreview {
            role,
            content: truncate_chars(content, MAX_PREVIEW_CHARS),
            timestamp,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_yaml_parses_the_fields_we_use() {
        let raw = "id: abc\ncwd: /home/dev/projects/alpha\nname: Review Code\nuser_named: false\ncreated_at: 2026-06-11T15:52:06.798Z\nupdated_at: 2026-06-11T15:52:36.388Z\n";
        let ws: Workspace = serde_yaml_ng::from_str(raw).unwrap();
        assert_eq!(ws.cwd.as_deref(), Some("/home/dev/projects/alpha"));
        assert_eq!(ws.name.as_deref(), Some("Review Code"));
        assert!(ws.created_at.is_some());
    }

    #[test]
    fn resume_command_is_copilot_resume_equals_id() {
        let provider = CopilotProvider::new(None);
        let session = Session {
            id: "copilot:abc".to_string(),
            provider: ProviderKind::Copilot,
            provider_session_id: "abc".to_string(),
            session_name: "x".to_string(),
            project_path: None,
            date_began: None,
            date_last_used: None,
            message_count: 1,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/events.jsonl"),
        };
        let command = provider
            .resume_command(&session, &AppConfig::default())
            .unwrap();
        assert_eq!(command.get_program(), "copilot");
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--resume=abc"]);
    }
}
