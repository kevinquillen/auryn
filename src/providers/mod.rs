//! Provider abstraction and registry.
//!
//! A [`Provider`] knows how to discover sessions for one AI coding tool and how
//! to build the native command that resumes one. The rest of Auryn -- the CLI,
//! the search index, and the TUI -- depends only on this trait and the
//! normalized [`Session`] model, never on a provider's on-disk format. New
//! providers are added by implementing this trait and registering them in
//! [`build_registry`]; no TUI or CLI code changes are required.

pub mod fake;

use std::process::Command;

use crate::config::AppConfig;
use crate::errors::Result;
use crate::models::{ProviderKind, Session};

/// Discovers and resumes sessions for a single AI coding tool.
pub trait Provider {
    /// Which tool this provider represents.
    fn kind(&self) -> ProviderKind;

    /// Human-facing name, e.g. for the session table and `doctor` output.
    fn display_name(&self) -> &'static str;

    /// The default location this provider scans, for diagnostics. `None` when
    /// the provider is synthetic or has no single root.
    fn default_root(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Discovers all sessions this provider can see. Implementations must treat
    /// session files as untrusted: tolerate malformed entries, bound file
    /// sizes, and never execute content found inside them.
    fn scan(&self, config: &AppConfig) -> Result<Vec<Session>>;

    /// Builds the native command that resumes `session`. Implementations must
    /// construct the command argument-by-argument and never invoke a shell.
    fn resume_command(&self, session: &Session, config: &AppConfig) -> Result<Command>;
}

/// Builds the set of active providers for the given configuration.
///
/// In Phase 1 only the synthetic [`fake::FakeProvider`] exists, gated behind
/// the `AURYN_FAKE` environment variable so it never appears during normal
/// use. Real providers (Claude, Codex, Gemini) are registered here in later
/// phases; callers iterate the returned slice without knowing the membership.
pub fn build_registry(_config: &AppConfig) -> Vec<Box<dyn Provider>> {
    let mut providers: Vec<Box<dyn Provider>> = Vec::new();

    if fake::fake_enabled() {
        providers.push(Box::new(fake::FakeProvider::from_env()));
    }

    providers
}
