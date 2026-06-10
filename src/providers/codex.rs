//! OpenAI Codex CLI session provider.
//!
//! Codex stores one JSONL "rollout" file per session under
//! `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl`. Each line is a
//! record `{ type, timestamp, payload }`:
//!
//! * `session_meta` (the first line) carries the session `id` and `cwd`.
//! * `response_item` with `payload.type == "message"` carries a conversational
//!   turn; `payload.role` is `user`/`assistant`/`developer`/`system` and
//!   `payload.content` is an array of blocks (`input_text`/`output_text`) each
//!   with a `text` field.
//! * Other records (`function_call`, `reasoning`, `event_msg`, ...) are skipped.
//!
//! Parsing is defensive and streaming, mirroring the Claude provider: files are
//! size-bounded, read line-by-line, malformed lines are skipped, only the last
//! N turns are kept for preview, and non-conversational roles (developer,
//! system) and text-less items are excluded from the count and preview.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use directories::BaseDirs;
use serde_json::Value;

use crate::config::{AppConfig, ProviderSettings};
use crate::errors::Result;
use crate::models::{MessagePreview, ProviderKind, Role, Session};
use crate::providers::Provider;
use crate::providers::util::{
    max_opt, min_opt, normalize_whitespace, parse_timestamp, truncate_chars,
};

/// Environment override for the scan root, used for tests and non-standard
/// installs. Takes effect only when the config does not set an explicit root.
const DIR_VAR: &str = "AURYN_CODEX_DIR";

/// The native CLI invoked to resume a session.
const RESUME_BIN: &str = "codex";

/// Upper bound on retained preview text per message.
const MAX_PREVIEW_CHARS: usize = 2000;

/// Bound on directory recursion below the sessions root (layout is YYYY/MM/DD).
const MAX_DEPTH: usize = 6;

/// Discovers and resumes OpenAI Codex CLI sessions.
pub struct CodexProvider {
    root: Option<PathBuf>,
}

impl CodexProvider {
    /// Creates a provider scanning `root`, or nothing when `root` is `None`.
    pub fn new(root: Option<PathBuf>) -> Self {
        CodexProvider { root }
    }

    /// Creates a provider with the root resolved from `settings`, the
    /// environment, then the platform default.
    pub fn from_settings(settings: &ProviderSettings) -> Self {
        CodexProvider::new(resolve_root(settings))
    }
}

/// Resolves the scan root: an explicit config root wins, then `AURYN_CODEX_DIR`,
/// then the default `~/.codex/sessions`.
pub fn resolve_root(settings: &ProviderSettings) -> Option<PathBuf> {
    if let Some(root) = &settings.root {
        return Some(root.clone());
    }
    if let Some(dir) = std::env::var_os(DIR_VAR) {
        return Some(PathBuf::from(dir));
    }
    default_root()
}

/// The platform-default Codex sessions directory.
fn default_root() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(".codex").join("sessions"))
}

impl Provider for CodexProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Codex
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn default_root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn scan(&self, config: &AppConfig) -> Result<Vec<Session>> {
        let root = match &self.root {
            Some(root) if root.is_dir() => root,
            _ => return Ok(Vec::new()),
        };
        let mut files = Vec::new();
        collect_jsonl(root, MAX_DEPTH, &mut files);

        let mut sessions = Vec::new();
        for file in files {
            if let Some(session) = parse_session(&file, config) {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    fn resume_command(&self, session: &Session, _config: &AppConfig) -> Result<Command> {
        // Shell-free: `codex resume <id>` with the project directory as cwd.
        let mut command = Command::new(RESUME_BIN);
        command.arg("resume").arg(&session.provider_session_id);
        if let Some(path) = &session.project_path {
            command.current_dir(path);
        }
        Ok(command)
    }
}

/// Recursively collects `.jsonl` files under `dir`, bounded by `depth`.
fn collect_jsonl(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect_jsonl(&path, depth - 1, out);
            }
        } else if path.extension() == Some(OsStr::new("jsonl")) {
            out.push(path);
        }
    }
}

/// Accumulates session data across a rollout file's records.
#[derive(Default)]
struct Builder {
    session_id: Option<String>,
    project_path: Option<PathBuf>,
    first_user_text: Option<String>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    message_count: usize,
    preview: VecDeque<MessagePreview>,
}

/// Parses one rollout file into a [`Session`], or `None` if it is too large,
/// unreadable, or has no conversational messages.
fn parse_session(path: &Path, config: &AppConfig) -> Option<Session> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > config.max_file_bytes {
        return None;
    }

    let reader = BufReader::new(File::open(path).ok()?);
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
        absorb_record(&mut builder, &record, config.preview_turns);
    }

    if builder.message_count == 0 {
        return None;
    }

    let provider_session_id = builder
        .session_id
        .or_else(|| id_from_filename(path))
        .unwrap_or_default();

    let session_name = builder
        .first_user_text
        .unwrap_or_else(|| provider_session_id.clone());

    let date_last_used = builder
        .last_ts
        .or_else(|| metadata.modified().ok().map(DateTime::<Utc>::from));

    Some(Session {
        id: Session::make_id(ProviderKind::Codex, &provider_session_id),
        provider: ProviderKind::Codex,
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

/// Folds a single rollout record into the builder.
fn absorb_record(builder: &mut Builder, record: &Value, preview_turns: usize) {
    if let Some(ts) = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
    {
        builder.first_ts = Some(min_opt(builder.first_ts, ts));
        builder.last_ts = Some(max_opt(builder.last_ts, ts));
    }

    let payload = match record.get("payload") {
        Some(payload) => payload,
        None => return,
    };

    match record.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            if let Some(id) = payload.get("id").and_then(Value::as_str) {
                builder.session_id = Some(id.to_string());
            }
            if builder.project_path.is_none()
                && let Some(cwd) = payload.get("cwd").and_then(Value::as_str)
            {
                builder.project_path = Some(PathBuf::from(cwd));
            }
        }
        Some("response_item") => absorb_message(builder, payload, preview_turns),
        _ => {}
    }
}

/// Folds a `response_item` payload into the builder when it is a conversational
/// `user`/`assistant` message with readable text.
fn absorb_message(builder: &mut Builder, payload: &Value, preview_turns: usize) {
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return;
    }
    let role = match payload.get("role").and_then(Value::as_str) {
        Some("assistant") => Role::Assistant,
        Some("user") => Role::User,
        // `developer`/`system` are instructions, not conversation.
        _ => return,
    };

    let text = extract_text(payload.get("content"));
    if text.trim().is_empty() {
        return;
    }
    // Codex injects AGENTS.md and environment/instruction context as `user`
    // turns; these are not real conversation, so exclude them from the name,
    // count, and preview.
    if role == Role::User && is_injected_context(&text) {
        return;
    }

    if role == Role::User && builder.first_user_text.is_none() {
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
            timestamp: None,
        });
    }
}

/// True when a `user` message is Codex-injected context (AGENTS.md, the
/// environment/instruction blocks, or an aborted-turn marker) rather than a
/// real prompt typed by the user.
fn is_injected_context(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("# AGENTS.md instructions")
        || head.starts_with("<INSTRUCTIONS")
        || head.starts_with("<environment_context")
        || head.starts_with("<user_instructions")
        || head.starts_with("<turn_aborted")
}

/// Joins the `text` fields of a message's content blocks (e.g. `input_text`,
/// `output_text`), ignoring blocks without text.
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(text);
                }
            }
            parts.join("\n")
        }
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Extracts the trailing UUID from a `rollout-<timestamp>-<uuid>` file stem, as
/// a fallback when no `session_meta` record provides the id.
fn id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 5 {
        return None;
    }
    // A UUID is the last five hyphen-separated groups (8-4-4-4-12).
    Some(parts[parts.len() - 5..].join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_content_blocks() {
        let content = serde_json::json!([
            {"type": "input_text", "text": "hello"},
            {"type": "input_text", "text": "world"}
        ]);
        assert_eq!(extract_text(Some(&content)), "hello\nworld");

        let empty = serde_json::json!([{"type": "image", "data": "..."}]);
        assert_eq!(extract_text(Some(&empty)), "");
    }

    #[test]
    fn recognizes_injected_context_messages() {
        assert!(is_injected_context(
            "# AGENTS.md instructions for /home/dev/x"
        ));
        assert!(is_injected_context(
            "<turn_aborted>interrupted</turn_aborted>"
        ));
        assert!(is_injected_context("<INSTRUCTIONS>do this</INSTRUCTIONS>"));
        assert!(!is_injected_context("How should the service work?"));
        assert!(!is_injected_context("# My heading in a real message"));
    }

    #[test]
    fn id_from_filename_extracts_trailing_uuid() {
        let path =
            PathBuf::from("rollout-2026-06-08T08-47-05-019ea745-b9aa-7f31-affe-aa6a582befbb.jsonl");
        assert_eq!(
            id_from_filename(&path).as_deref(),
            Some("019ea745-b9aa-7f31-affe-aa6a582befbb")
        );
    }

    #[test]
    fn resume_command_is_codex_resume_with_session_id() {
        let provider = CodexProvider::new(None);
        let session = Session {
            id: "codex:abc".to_string(),
            provider: ProviderKind::Codex,
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
        assert_eq!(command.get_program(), "codex");
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["resume", "abc"]);
    }
}
