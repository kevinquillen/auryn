//! Crate-wide error type.
//!
//! Provider scanners and configuration loading return [`AurynError`] so that
//! failures from untrusted session files stay typed and recoverable rather
//! than panicking. The binary entrypoint converts these into process exits.

use std::path::PathBuf;

use thiserror::Error;

/// Convenience alias for results that fail with [`AurynError`].
pub type Result<T> = std::result::Result<T, AurynError>;

/// Every fallible operation in Auryn surfaces one of these variants.
#[derive(Debug, Error)]
pub enum AurynError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("could not determine a platform configuration directory")]
    NoConfigDir,

    #[error("provider {provider}: {message}")]
    Provider { provider: String, message: String },

    #[error("session file too large to parse: {path} ({size} bytes)")]
    FileTooLarge { path: PathBuf, size: u64 },

    #[error("no provider supports session id: {0}")]
    UnknownSession(String),

    #[error(
        "session id '{0}' is missing its provider prefix; resume expects provider:session_id (for example codex:{0}). Run 'auryn list --json' to see full session ids."
    )]
    MalformedSessionId(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("failed to parse TOML: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("failed to serialize TOML: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("failed to serialize JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl AurynError {
    /// Builds a provider-scoped error from any displayable message.
    pub fn provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        AurynError::Provider {
            provider: provider.into(),
            message: message.into(),
        }
    }
}
