//! Gemini CLI session provider.
//!
//! Gemini stores sessions at
//! `~/.gemini/tmp/<project>/chats/session-<timestamp><id>.jsonl`, with the
//! project's real path in a sibling `<project>/.project_root` file. Despite the
//! `tmp` name, this is the persistent session store (`gemini --list-sessions`
//! reads it directly).
//!
//! Each file is an append-only log of state mutations:
//!
//! * A header line `{ kind, sessionId, startTime, lastUpdated, projectHash }`.
//! * `$set` records carrying partial updates, including an initial
//!   `$set.messages` array.
//! * Message records `{ type, content, timestamp, id }` where `type` is `user`
//!   or `gemini`; a `gemini` `content` is a string, a `user` `content` is an
//!   array of parts (only `text` parts are conversational).
//!
//! Because a streaming message is rewritten under the same `id`, messages are
//! de-duplicated by id (last write wins) and ordered by timestamp. Gemini
//! injects a `<session_context>` user turn, which is filtered out like Codex's
//! AGENTS.md context.
//!
//! Gemini 0.46 has no resume-by-id, and continuing a session forks a new one
//! (a new id sharing the same conversation origin). Resume uses the fast
//! `gemini --session-file <path>`; [`collapse_forks`] then hides the resulting
//! near-duplicate lineage in the list by keeping the most recent of a project's
//! same-start-second sessions. (The in-place `--resume <index>` path is avoided
//! because `gemini --list-sessions` is prohibitively slow.)

use std::collections::HashMap;
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

/// Environment override for the scan root.
const DIR_VAR: &str = "AURYN_GEMINI_DIR";

/// The native CLI invoked to resume a session.
const RESUME_BIN: &str = "gemini";

/// Upper bound on retained preview text per message.
const MAX_PREVIEW_CHARS: usize = 2000;

/// Discovers and resumes Gemini CLI sessions.
pub struct GeminiProvider {
    root: Option<PathBuf>,
}

impl GeminiProvider {
    /// Creates a provider scanning `root`, or nothing when `root` is `None`.
    pub fn new(root: Option<PathBuf>) -> Self {
        GeminiProvider { root }
    }

    /// Creates a provider with the root resolved from `settings`, the
    /// environment, then the platform default.
    pub fn from_settings(settings: &ProviderSettings) -> Self {
        GeminiProvider::new(resolve_root(settings))
    }
}

/// Resolves the scan root: an explicit config root wins, then `AURYN_GEMINI_DIR`,
/// then the default `~/.gemini/tmp`.
pub fn resolve_root(settings: &ProviderSettings) -> Option<PathBuf> {
    if let Some(root) = &settings.root {
        return Some(root.clone());
    }
    if let Some(dir) = std::env::var_os(DIR_VAR) {
        return Some(PathBuf::from(dir));
    }
    default_root()
}

/// The platform-default Gemini sessions directory.
fn default_root() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(".gemini").join("tmp"))
}

impl Provider for GeminiProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    fn display_name(&self) -> &'static str {
        "Gemini"
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
        for project_dir in subdirs(root) {
            let chats = project_dir.join("chats");
            if !chats.is_dir() {
                continue;
            }
            let project_root = read_project_root(&project_dir);
            for file in jsonl_files(&chats) {
                if let Some(session) = parse_session(&file, project_root.as_deref(), config) {
                    sessions.push(session);
                }
            }
        }
        Ok(collapse_forks(sessions))
    }

    fn resume_command(&self, session: &Session, _config: &AppConfig) -> Result<Command> {
        // Gemini 0.46 has no resume-by-id. `--session-file` loads the exact
        // session file; continuing it forks a new session (a Gemini behavior),
        // which `collapse_forks` hides in the list. The in-place `--resume
        // <index>` path is avoided because `gemini --list-sessions` is very slow.
        let mut command = Command::new(RESUME_BIN);
        command.arg("--session-file").arg(&session.source_path);
        apply_working_dir(&mut command, session.project_path.as_deref());
        Ok(command)
    }

    fn read_messages(&self, session: &Session, config: &AppConfig) -> Result<Vec<MessagePreview>> {
        collect_messages(&session.source_path, config)
    }
}

/// Reads the full, de-duplicated, time-ordered conversation from a Gemini
/// session file, untruncated, for export. Reuses the same record folding and
/// finalize steps as [`parse_session`] but stores complete message text (the
/// scan path truncates at storage to bound memory); the file-size bound still
/// caps total memory.
fn collect_messages(path: &Path, config: &AppConfig) -> Result<Vec<MessagePreview>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > config.max_file_bytes {
        return Err(AurynError::FileTooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
        });
    }

    let reader = BufReader::new(File::open(path)?);
    let mut builder = Builder {
        session_id: None,
        first_ts: None,
        last_ts: None,
        by_id: HashMap::new(),
    };
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        absorb_record(&mut builder, &record, usize::MAX);
    }

    let messages = finalize_messages(builder.by_id)
        .into_iter()
        .map(|m| MessagePreview {
            role: m.role,
            content: m.text,
            timestamp: m.timestamp,
        })
        .collect();
    Ok(messages)
}

/// Collapses Gemini fork lineages for display. Continuing a Gemini session
/// creates a new session id sharing the same conversation origin, so repeated
/// resumes pile up near-identical entries. Sessions in the same project whose
/// start time falls within the same second are treated as one lineage, keeping
/// only the most recently used. Sessions without a start time are left as-is.
fn collapse_forks(sessions: Vec<Session>) -> Vec<Session> {
    use std::collections::HashMap;
    let mut newest: HashMap<(String, i64), Session> = HashMap::new();
    let mut ungrouped: Vec<Session> = Vec::new();
    for session in sessions {
        let Some(began) = session.date_began else {
            ungrouped.push(session);
            continue;
        };
        let project = session
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let key = (project, began.timestamp());
        match newest.get(&key) {
            Some(existing) if existing.date_last_used >= session.date_last_used => {}
            _ => {
                newest.insert(key, session);
            }
        }
    }
    newest.into_values().chain(ungrouped).collect()
}

/// Returns the immediate subdirectories of `dir`, tolerating IO errors.
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path);
            }
        }
    }
    out
}

/// Returns the `.jsonl` files directly inside `dir`.
fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(OsStr::new("jsonl")) {
                out.push(path);
            }
        }
    }
    out
}

/// Reads the project's real path from `<project_dir>/.project_root`.
fn read_project_root(project_dir: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(project_dir.join(".project_root")).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// A de-duplicated conversational message.
struct GMessage {
    role: Role,
    text: String,
    timestamp: Option<DateTime<Utc>>,
}

/// Accumulates session data across a Gemini session file's records.
struct Builder {
    session_id: Option<String>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    /// Messages keyed by id; a streaming rewrite under the same id overwrites
    /// the earlier, partial version.
    by_id: HashMap<String, GMessage>,
}

/// Parses one Gemini session file into a [`Session`], or `None` if it is too
/// large, unreadable, or has no conversational messages.
fn parse_session(path: &Path, project_root: Option<&Path>, config: &AppConfig) -> Option<Session> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > config.max_file_bytes {
        return None;
    }

    let reader = BufReader::new(File::open(path).ok()?);
    let mut builder = Builder {
        session_id: None,
        first_ts: None,
        last_ts: None,
        by_id: HashMap::new(),
    };

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
        absorb_record(&mut builder, &record, MAX_PREVIEW_CHARS);
    }

    // De-duplicated, ordered, conversational messages.
    let messages = finalize_messages(builder.by_id);
    if messages.is_empty() {
        return None;
    }

    let provider_session_id = builder
        .session_id
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();

    let session_name = messages
        .iter()
        .find(|m| m.role == Role::User)
        .map(|m| truncate_chars(&normalize_whitespace(&m.text), 80))
        .unwrap_or_else(|| provider_session_id.clone());

    let start = messages.len().saturating_sub(config.preview_turns);
    let preview_messages: Vec<MessagePreview> = messages[start..]
        .iter()
        .map(|m| MessagePreview {
            role: m.role,
            content: truncate_chars(&m.text, MAX_PREVIEW_CHARS),
            timestamp: m.timestamp,
        })
        .collect();

    let date_last_used = builder
        .last_ts
        .or_else(|| metadata.modified().ok().map(DateTime::<Utc>::from));

    Some(Session {
        id: Session::make_id(ProviderKind::Gemini, &provider_session_id),
        provider: ProviderKind::Gemini,
        provider_session_id,
        session_name,
        project_path: project_root.map(Path::to_path_buf),
        date_began: builder.first_ts,
        date_last_used,
        message_count: messages.len(),
        preview_messages,
        source_path: path.to_path_buf(),
    })
}

/// Reduces the de-duplicated message map to ordered, conversational messages:
/// drops empty and injected-context turns, then sorts by timestamp. Shared by
/// the scan (preview) and export (full) paths so both agree on what counts.
fn finalize_messages(by_id: HashMap<String, GMessage>) -> Vec<GMessage> {
    let mut messages: Vec<GMessage> = by_id
        .into_values()
        .filter(|m| !m.text.trim().is_empty())
        .filter(|m| !(m.role == Role::User && is_injected_context(&m.text)))
        .collect();
    messages.sort_by_key(|m| m.timestamp);
    messages
}

/// Folds one record (header, `$set`, or message) into the builder. `max_chars`
/// bounds the stored text per message: the scan path truncates to keep memory
/// bounded, while export passes `usize::MAX` to retain full text.
fn absorb_record(builder: &mut Builder, record: &Value, max_chars: usize) {
    // Header line.
    if record.get("kind").is_some() {
        if let Some(id) = record.get("sessionId").and_then(Value::as_str) {
            builder.session_id = Some(id.to_string());
        }
        update_ts(builder, record.get("startTime"));
        update_ts(builder, record.get("lastUpdated"));
        return;
    }

    // Update record: may carry `lastUpdated` and/or an initial `messages` array.
    if let Some(set) = record.get("$set") {
        update_ts(builder, set.get("lastUpdated"));
        if let Some(Value::Array(messages)) = set.get("messages") {
            for message in messages {
                absorb_message(builder, message, max_chars);
            }
        }
        return;
    }

    // Otherwise it is a streamed message record.
    absorb_message(builder, record, max_chars);
}

/// Folds a single message object into the builder, de-duplicating by `id` and
/// bounding stored text to `max_chars`.
fn absorb_message(builder: &mut Builder, message: &Value, max_chars: usize) {
    let role = match message.get("type").and_then(Value::as_str) {
        Some("user") => Role::User,
        Some("gemini") | Some("assistant") => Role::Assistant,
        _ => return,
    };
    let id = match message.get("id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => return,
    };
    let timestamp = message
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    if let Some(ts) = timestamp {
        builder.first_ts = Some(min_opt(builder.first_ts, ts));
        builder.last_ts = Some(max_opt(builder.last_ts, ts));
    }

    let text = truncate_chars(&extract_text(message.get("content")), max_chars);
    // Last write wins: a completed streamed message overwrites its partial form.
    builder.by_id.insert(
        id,
        GMessage {
            role,
            text,
            timestamp,
        },
    );
}

/// Extracts text from a message `content`, which is either a plain string
/// (Gemini turns) or an array of parts whose `text` fields are joined (user
/// turns); non-text parts such as `functionResponse` are ignored.
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut out = Vec::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    out.push(text);
                }
            }
            out.join("\n")
        }
        _ => String::new(),
    }
}

/// True when a `user` message is the injected Gemini session context rather than
/// a real prompt.
fn is_injected_context(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("<session_context>") || head.starts_with("<environment_context")
}

/// Updates the builder's timestamp range from an optional RFC 3339 string value.
fn update_ts(builder: &mut Builder, value: Option<&Value>) {
    if let Some(ts) = value.and_then(Value::as_str).and_then(parse_timestamp) {
        builder.first_ts = Some(min_opt(builder.first_ts, ts));
        builder.last_ts = Some(max_opt(builder.last_ts, ts));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_string_and_parts_content() {
        let string = Value::String("hello from gemini".to_string());
        assert_eq!(extract_text(Some(&string)), "hello from gemini");

        let parts =
            serde_json::json!([{"text": "first"}, {"functionResponse": {}}, {"text": "second"}]);
        assert_eq!(extract_text(Some(&parts)), "first\nsecond");

        let tool_only = serde_json::json!([{"functionResponse": {}}]);
        assert_eq!(extract_text(Some(&tool_only)), "");
    }

    #[test]
    fn recognizes_injected_session_context() {
        assert!(is_injected_context(
            "<session_context> This is the Gemini CLI."
        ));
        assert!(!is_injected_context("How do I run the tests?"));
    }

    fn session(id: &str, source: &str) -> Session {
        Session {
            id: format!("gemini:{id}"),
            provider: ProviderKind::Gemini,
            provider_session_id: id.to_string(),
            session_name: "x".to_string(),
            project_path: Some(PathBuf::from("/tmp/proj")),
            date_began: None,
            date_last_used: None,
            message_count: 1,
            preview_messages: Vec::new(),
            source_path: PathBuf::from(source),
        }
    }

    #[test]
    fn resume_loads_the_session_file() {
        let provider = GeminiProvider::new(None);
        let command = provider
            .resume_command(&session("abc", "/tmp/chats/s.jsonl"), &AppConfig::default())
            .unwrap();
        assert_eq!(command.get_program(), "gemini");
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--session-file", "/tmp/chats/s.jsonl"]);
    }

    #[test]
    fn collapse_forks_keeps_newest_of_a_same_second_lineage() {
        use chrono::{TimeZone, Utc};
        let began = Utc.timestamp_opt(1_000_000_000, 163_000_000).unwrap();
        // Same began-second; a fork is 1ms later. Different last-used.
        let mut original = session("orig", "/g/orig.jsonl");
        original.date_began = Some(began);
        original.date_last_used = Some(Utc.timestamp_opt(1_000_000_100, 0).unwrap());

        let mut fork = session("fork", "/g/fork.jsonl");
        fork.date_began = Some(began + chrono::Duration::milliseconds(1));
        fork.date_last_used = Some(Utc.timestamp_opt(1_000_009_999, 0).unwrap());

        // A genuinely different session (different project) is not merged.
        let mut other = session("other", "/g/other.jsonl");
        other.project_path = Some(PathBuf::from("/other/proj"));
        other.date_began = Some(began);
        other.date_last_used = Some(Utc.timestamp_opt(1_000_000_050, 0).unwrap());

        let collapsed = collapse_forks(vec![original, fork, other]);
        let ids: std::collections::HashSet<_> = collapsed
            .iter()
            .map(|s| s.provider_session_id.clone())
            .collect();
        assert_eq!(collapsed.len(), 2);
        assert!(ids.contains("fork")); // newest of the lineage survives
        assert!(ids.contains("other")); // different project untouched
        assert!(!ids.contains("orig")); // older lineage member collapsed away
    }
}
