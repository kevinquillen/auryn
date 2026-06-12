//! Claude Code session provider.
//!
//! Claude Code stores one JSONL file per session at
//! `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`, where the file stem is
//! the session id passed to `claude --resume`. Each line is a record with a
//! `type`: `user`/`assistant` lines carry messages, an `ai-title` line carries
//! the session name, and other lines are metadata.
//!
//! Parsing is defensive: files are size-bounded, read line-by-line so memory
//! stays bounded regardless of conversation length, malformed lines are skipped,
//! and only the last N conversational turns are retained for preview. Records
//! flagged `isMeta`/`isSidechain`, and messages with no readable text (e.g. pure
//! tool calls), are excluded from the preview and message count.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use directories::BaseDirs;
use serde_json::Value;

use crate::config::{AppConfig, ProviderSettings};
use crate::errors::{AurynError, Result};
use crate::models::{MessagePreview, ProviderKind, Role, Session};
use crate::providers::Provider;
use crate::providers::util::{
    apply_working_dir, max_opt, min_opt, normalize_whitespace, parse_timestamp, truncate_chars,
};

/// Environment override for the scan root, used for tests and non-standard
/// installs. Takes effect only when the config does not set an explicit root.
const DIR_VAR: &str = "AURYN_CLAUDE_DIR";

/// The native CLI invoked to resume a session.
const RESUME_BIN: &str = "claude";

/// Upper bound on retained preview text per message, to bound memory and keep
/// previews readable.
const MAX_PREVIEW_CHARS: usize = 2000;

/// Discovers and resumes Claude Code sessions.
pub struct ClaudeProvider {
    root: Option<PathBuf>,
}

impl ClaudeProvider {
    /// Creates a provider scanning `root`, or nothing when `root` is `None`.
    pub fn new(root: Option<PathBuf>) -> Self {
        ClaudeProvider { root }
    }

    /// Creates a provider with the root resolved from `settings`, the
    /// environment, then the platform default.
    pub fn from_settings(settings: &ProviderSettings) -> Self {
        ClaudeProvider::new(resolve_root(settings))
    }
}

/// Resolves the scan root: an explicit config root wins, then `AURYN_CLAUDE_DIR`,
/// then the default `~/.claude/projects`.
pub fn resolve_root(settings: &ProviderSettings) -> Option<PathBuf> {
    if let Some(root) = &settings.root {
        return Some(root.clone());
    }
    if let Some(dir) = std::env::var_os(DIR_VAR) {
        return Some(PathBuf::from(dir));
    }
    default_root()
}

/// The platform-default Claude Code projects directory.
fn default_root() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(".claude").join("projects"))
}

impl Provider for ClaudeProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Claude
    }

    fn display_name(&self) -> &'static str {
        "Claude"
    }

    fn default_root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn scan(&self, config: &AppConfig) -> Result<Vec<Session>> {
        let root = match &self.root {
            Some(root) if root.is_dir() => root,
            // A missing root is normal (e.g. Claude Code not installed); it is
            // not an error, just an empty result.
            _ => return Ok(Vec::new()),
        };

        let mut sessions = Vec::new();
        for project in read_subdirs(root) {
            for file in session_files(&project) {
                if let Some(session) = parse_session(&file, config) {
                    sessions.push(session);
                }
            }
        }
        Ok(sessions)
    }

    fn resume_command(&self, session: &Session, _config: &AppConfig) -> Result<Command> {
        // Shell-free: the binary and arguments are passed explicitly, and the
        // session id originates from the file name, never from file content.
        let mut command = Command::new(RESUME_BIN);
        command.arg("--resume").arg(&session.provider_session_id);
        apply_working_dir(&mut command, session.project_path.as_deref());
        Ok(command)
    }

    fn read_messages(&self, session: &Session, config: &AppConfig) -> Result<Vec<MessagePreview>> {
        collect_messages(&session.source_path, config)
    }
}

/// Reads every readable user/assistant turn from a session file in file order,
/// untruncated, for export. Applies the same size bound and tolerant line
/// parsing as [`parse_session`]; a file over the limit is an error rather than
/// a silent truncation.
fn collect_messages(path: &Path, config: &AppConfig) -> Result<Vec<MessagePreview>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > config.max_file_bytes {
        return Err(AurynError::FileTooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
        });
    }

    let reader = BufReader::new(File::open(path)?);
    let mut messages = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some((role, text, timestamp)) = message_from_record(&record) {
            messages.push(MessagePreview {
                role,
                content: text,
                timestamp,
            });
        }
    }
    Ok(messages)
}

/// Returns the immediate subdirectories of `root`, tolerating IO errors.
fn read_subdirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
    }
    dirs
}

/// Returns the `.jsonl` files directly inside a project directory.
fn session_files(project: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(project) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(OsStr::new("jsonl")) {
                files.push(path);
            }
        }
    }
    files
}

/// Accumulates session data across a file's records.
#[derive(Default)]
struct Builder {
    session_id: Option<String>,
    project_path: Option<PathBuf>,
    ai_title: Option<String>,
    first_user_text: Option<String>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    message_count: usize,
    preview: VecDeque<MessagePreview>,
}

/// Parses one session file into a [`Session`], or `None` if it is too large,
/// unreadable, or contains no conversational messages.
fn parse_session(path: &Path, config: &AppConfig) -> Option<Session> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > config.max_file_bytes {
        return None;
    }

    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
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
            Err(_) => continue, // Skip malformed lines.
        };
        absorb_record(&mut builder, &record, config.preview_turns);
    }

    if builder.message_count == 0 {
        return None;
    }

    let provider_session_id = builder
        .session_id
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();

    let session_name = builder
        .ai_title
        .filter(|t| !t.trim().is_empty())
        .or(builder.first_user_text)
        .unwrap_or_else(|| provider_session_id.clone());

    let date_last_used = builder
        .last_ts
        .or_else(|| metadata.modified().ok().map(DateTime::<Utc>::from));

    Some(Session {
        id: Session::make_id(ProviderKind::Claude, &provider_session_id),
        provider: ProviderKind::Claude,
        provider_session_id,
        session_name,
        project_path: builder.project_path,
        date_began: builder.first_ts,
        date_last_used,
        message_count: builder.message_count,
        preview_messages: builder.preview.into_iter().collect(),
        source_path: path.to_path_buf(),
    })
}

/// Folds a single JSONL record into the builder.
fn absorb_record(builder: &mut Builder, record: &Value, preview_turns: usize) {
    if builder.session_id.is_none()
        && let Some(id) = record.get("sessionId").and_then(Value::as_str)
    {
        builder.session_id = Some(id.to_string());
    }
    if builder.project_path.is_none()
        && let Some(cwd) = record.get("cwd").and_then(Value::as_str)
    {
        builder.project_path = Some(PathBuf::from(cwd));
    }
    if let Some(ts) = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
    {
        builder.first_ts = Some(min_opt(builder.first_ts, ts));
        builder.last_ts = Some(max_opt(builder.last_ts, ts));
    }

    let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
    if record_type == "ai-title"
        && let Some(title) = record.get("aiTitle").and_then(Value::as_str)
    {
        builder.ai_title = Some(normalize_whitespace(title));
    }

    let Some((role, text, timestamp)) = message_from_record(record) else {
        return;
    };

    if role == Role::User && builder.first_user_text.is_none() {
        // Names appear in a single table row, so collapse any newlines first.
        builder.first_user_text = Some(truncate_chars(&normalize_whitespace(&text), 80));
    }

    builder.message_count += 1;
    if preview_turns > 0 {
        if builder.preview.len() == preview_turns {
            builder.preview.pop_front();
        }
        builder.preview.push_back(MessagePreview {
            role,
            content: truncate_chars(&text, MAX_PREVIEW_CHARS),
            timestamp,
        });
    }
}

/// Extracts a conversational turn from a record, or `None` when it is not a
/// readable user/assistant message (wrong type, meta/sidechain, or text-less
/// tool use). The returned text is untruncated; callers bound it as needed.
fn message_from_record(record: &Value) -> Option<(Role, String, Option<DateTime<Utc>>)> {
    let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
    if record_type != "user" && record_type != "assistant" {
        return None;
    }
    if flag(record, "isMeta") || flag(record, "isSidechain") {
        return None;
    }

    let message = record.get("message")?;
    let text = extract_text(message.get("content"));
    if text.trim().is_empty() {
        return None; // Pure tool-use/result turns carry no readable text.
    }

    let role = match message.get("role").and_then(Value::as_str) {
        Some("assistant") => Role::Assistant,
        Some("system") => Role::System,
        _ => Role::User,
    };
    let timestamp = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    Some((role, text, timestamp))
}

/// Extracts readable text from a message `content` value, which is either a
/// plain string or an array of typed blocks (only `text` blocks are kept).
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    parts.push(text);
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Reads a boolean flag, defaulting to false when absent or non-boolean.
fn flag(record: &Value, key: &str) -> bool {
    record.get(key).and_then(Value::as_bool).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_string_and_array_content() {
        let string = Value::String("hello".to_string());
        assert_eq!(extract_text(Some(&string)), "hello");

        let array = serde_json::json!([
            {"type": "text", "text": "first"},
            {"type": "tool_use", "name": "Bash"},
            {"type": "text", "text": "second"}
        ]);
        assert_eq!(extract_text(Some(&array)), "first\nsecond");

        let tool_only = serde_json::json!([{"type": "tool_use", "name": "Bash"}]);
        assert_eq!(extract_text(Some(&tool_only)), "");
    }

    #[test]
    fn resume_command_is_claude_resume_with_session_id() {
        let provider = ClaudeProvider::new(None);
        let session = Session {
            id: "claude:abc".to_string(),
            provider: ProviderKind::Claude,
            provider_session_id: "abc".to_string(),
            session_name: "x".to_string(),
            project_path: Some(PathBuf::from("/tmp/proj")),
            date_began: None,
            date_last_used: None,
            message_count: 1,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/x.jsonl"),
        };
        let command = provider
            .resume_command(&session, &AppConfig::default())
            .unwrap();
        assert_eq!(command.get_program(), "claude");
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--resume", "abc"]);
    }
}
