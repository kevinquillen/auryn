//! Core domain models shared across providers, search, and the TUI.
//!
//! These types are the normalized representation every provider maps its
//! native session format onto, so that no code outside `providers/` needs to
//! know how an individual tool stores its sessions on disk.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies which AI coding tool a session originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Claude,
    Codex,
    Gemini,
    /// Synthetic provider used to validate the architecture before real
    /// provider scanners exist. Never represents real user data.
    Fake,
}

impl ProviderKind {
    /// Stable machine-readable identifier, used in session ids and JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Claude => "claude",
            ProviderKind::Codex => "codex",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Fake => "fake",
        }
    }

    /// Human-facing label shown in the session table.
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderKind::Claude => "Claude",
            ProviderKind::Codex => "Codex",
            ProviderKind::Gemini => "Gemini",
            ProviderKind::Fake => "Fake",
        }
    }

    /// Every provider kind, in the MVP support order.
    pub fn all() -> [ProviderKind; 4] {
        [
            ProviderKind::Claude,
            ProviderKind::Codex,
            ProviderKind::Gemini,
            ProviderKind::Fake,
        ]
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

impl FromStr for ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(ProviderKind::Claude),
            "codex" | "openai" => Ok(ProviderKind::Codex),
            "gemini" => Ok(ProviderKind::Gemini),
            "fake" => Ok(ProviderKind::Fake),
            other => Err(format!("unknown provider: {other}")),
        }
    }
}

/// The author of a single conversational message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    /// Short label used when rendering a preview turn.
    pub fn label(&self) -> &'static str {
        match self {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
        }
    }
}

/// A single conversational turn captured for previewing, already truncated to
/// a bounded length by the originating provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePreview {
    pub role: Role,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
}

/// A normalized AI coding session, independent of which provider produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Auryn-stable identifier, unique across providers.
    pub id: String,
    pub provider: ProviderKind,
    /// The provider's own native session identifier, used when resuming.
    pub provider_session_id: String,
    pub session_name: String,
    pub project_path: Option<PathBuf>,
    pub date_began: Option<DateTime<Utc>>,
    pub date_last_used: Option<DateTime<Utc>>,
    pub message_count: usize,
    pub preview_messages: Vec<MessagePreview>,
    /// The file or directory this session was parsed from.
    pub source_path: PathBuf,
}

impl Session {
    /// Builds the Auryn-stable id from a provider and its native session id.
    pub fn make_id(provider: ProviderKind, provider_session_id: &str) -> String {
        format!("{}:{}", provider.as_str(), provider_session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_round_trips_through_str() {
        for kind in ProviderKind::all() {
            let parsed: ProviderKind = kind.as_str().parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn provider_kind_parsing_is_case_insensitive_and_aliased() {
        assert_eq!(
            "CLAUDE".parse::<ProviderKind>().unwrap(),
            ProviderKind::Claude
        );
        assert_eq!(
            "  Gemini ".parse::<ProviderKind>().unwrap(),
            ProviderKind::Gemini
        );
        assert_eq!(
            "openai".parse::<ProviderKind>().unwrap(),
            ProviderKind::Codex
        );
    }

    #[test]
    fn unknown_provider_is_rejected() {
        assert!("aider".parse::<ProviderKind>().is_err());
    }

    #[test]
    fn make_id_namespaces_by_provider() {
        let id = Session::make_id(ProviderKind::Claude, "abc-123");
        assert_eq!(id, "claude:abc-123");
    }
}
