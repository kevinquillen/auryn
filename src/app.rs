//! Application orchestration.
//!
//! [`App`] owns the loaded configuration and the active provider registry, and
//! exposes the high-level operations the CLI and (later) the TUI build on:
//! scanning every provider into a unified, sorted session list, and locating a
//! session by id for resume. It contains no provider-specific logic.

use std::cmp::Ordering;
use std::process::Command;

use crate::config::AppConfig;
use crate::errors::{AurynError, Result};
use crate::models::Session;
use crate::providers::{self, Provider};
use crate::search::Filter;

/// The running application: configuration plus the active providers.
pub struct App {
    config: AppConfig,
    providers: Vec<Box<dyn Provider>>,
}

impl App {
    /// Loads configuration and builds the provider registry.
    pub fn load() -> Result<App> {
        let config = AppConfig::load()?;
        Ok(App::with_config(config))
    }

    /// Builds an app around an explicit configuration. Useful for tests.
    pub fn with_config(config: AppConfig) -> App {
        let providers = providers::build_registry(&config);
        App { config, providers }
    }

    /// Read-only access to the active configuration.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// The providers Auryn will scan, in registry order.
    pub fn providers(&self) -> &[Box<dyn Provider>] {
        &self.providers
    }

    /// Scans every provider and returns one unified, sorted session list.
    ///
    /// A provider that fails to scan does not abort the whole operation; its
    /// error is collected and returned alongside the sessions that did load, so
    /// the caller can surface partial failures without losing good data.
    pub fn scan_all(&self) -> ScanOutcome {
        let mut sessions = Vec::new();
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.scan(&self.config) {
                Ok(mut found) => sessions.append(&mut found),
                Err(err) => errors.push((provider.display_name(), err)),
            }
        }
        sort_by_recency(&mut sessions);
        ScanOutcome { sessions, errors }
    }

    /// Scans every provider and applies `filter`, returning the sorted matches.
    pub fn scan_filtered(&self, filter: &Filter) -> ScanOutcome {
        let mut outcome = self.scan_all();
        outcome.sessions = filter.apply(outcome.sessions);
        outcome
    }

    /// Builds the native resume command for the session with the given id.
    pub fn resume_command(&self, session_id: &str) -> Result<Command> {
        let outcome = self.scan_all();
        let session = outcome
            .sessions
            .into_iter()
            .find(|s| s.id == session_id)
            .ok_or_else(|| AurynError::UnknownSession(session_id.to_string()))?;
        let provider = self
            .providers
            .iter()
            .find(|p| p.kind() == session.provider)
            .ok_or_else(|| AurynError::UnknownSession(session_id.to_string()))?;
        provider.resume_command(&session, &self.config)
    }
}

/// The result of scanning all providers: the sessions found plus any
/// per-provider errors that did not abort the scan.
pub struct ScanOutcome {
    pub sessions: Vec<Session>,
    pub errors: Vec<(&'static str, AurynError)>,
}

/// Sorts sessions most-recently-used first, with unknown dates sorted last and
/// ties broken by session name for stable, deterministic output.
fn sort_by_recency(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| {
        match (b.date_last_used, a.date_last_used) {
            (Some(bd), Some(ad)) => bd.cmp(&ad),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| a.session_name.cmp(&b.session_name))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, Session};
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    fn session(name: &str, last_used: Option<i64>) -> Session {
        Session {
            id: name.to_string(),
            provider: ProviderKind::Fake,
            provider_session_id: name.to_string(),
            session_name: name.to_string(),
            project_path: None,
            date_began: None,
            date_last_used: last_used.map(|h| Utc.timestamp_opt(h, 0).unwrap()),
            message_count: 0,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/x"),
        }
    }

    #[test]
    fn sort_orders_most_recent_first_and_nulls_last() {
        let mut sessions = vec![
            session("older", Some(100)),
            session("newest", Some(300)),
            session("undated", None),
            session("middle", Some(200)),
        ];
        sort_by_recency(&mut sessions);
        let order: Vec<_> = sessions.iter().map(|s| s.session_name.as_str()).collect();
        assert_eq!(order, ["newest", "middle", "older", "undated"]);
    }

    #[test]
    fn sort_breaks_ties_by_name() {
        let mut sessions = vec![session("zebra", Some(100)), session("alpha", Some(100))];
        sort_by_recency(&mut sessions);
        let order: Vec<_> = sessions.iter().map(|s| s.session_name.as_str()).collect();
        assert_eq!(order, ["alpha", "zebra"]);
    }
}
