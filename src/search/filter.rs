//! Composable session filtering.
//!
//! A [`Filter`] combines an optional free-text query with an optional provider
//! restriction. Text matching is case-insensitive and term-wise conjunctive:
//! every whitespace-separated term must appear somewhere in the session's
//! searchable text (name, project path, provider, and preview content). This
//! satisfies the spec's metadata-plus-content search; fuzzy and regex matching
//! are layered on in Phase 6.

use crate::models::{ProviderKind, Session};

/// A reusable, composable session filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Free-text query; `None` or empty matches everything.
    pub text: Option<String>,
    /// Restrict to a single provider; `None` matches all providers.
    pub provider: Option<ProviderKind>,
}

impl Filter {
    /// A filter that matches every session.
    pub fn none() -> Self {
        Filter::default()
    }

    /// Returns a filter with the given text query.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.text = if text.trim().is_empty() {
            None
        } else {
            Some(text)
        };
        self
    }

    /// Returns a filter restricted to a single provider.
    pub fn with_provider(mut self, provider: ProviderKind) -> Self {
        self.provider = Some(provider);
        self
    }

    /// True when no constraints are active.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.provider.is_none()
    }

    /// Tests a single session against this filter.
    pub fn matches(&self, session: &Session) -> bool {
        if let Some(provider) = self.provider
            && session.provider != provider
        {
            return false;
        }
        match &self.text {
            None => true,
            Some(query) => text_matches(session, query),
        }
    }

    /// Returns only the sessions that satisfy this filter, preserving order.
    pub fn apply(&self, sessions: Vec<Session>) -> Vec<Session> {
        if self.is_empty() {
            return sessions;
        }
        sessions.into_iter().filter(|s| self.matches(s)).collect()
    }
}

/// True when every term in `query` appears in the session's searchable text.
fn text_matches(session: &Session, query: &str) -> bool {
    let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
    if terms.is_empty() {
        return true;
    }
    let haystack = searchable_text(session);
    terms.iter().all(|term| haystack.contains(term))
}

/// Builds the lowercased text blob a query is matched against.
fn searchable_text(session: &Session) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(session.session_name.to_lowercase());
    parts.push(session.provider.display_name().to_lowercase());
    parts.push(session.provider.as_str().to_string());
    if let Some(path) = &session.project_path {
        parts.push(path.to_string_lossy().to_lowercase());
    }
    for message in &session.preview_messages {
        parts.push(message.content.to_lowercase());
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MessagePreview, Role};
    use std::path::PathBuf;

    fn session(
        name: &str,
        provider: ProviderKind,
        project: Option<&str>,
        content: &[&str],
    ) -> Session {
        Session {
            id: Session::make_id(provider, name),
            provider,
            provider_session_id: name.to_string(),
            session_name: name.to_string(),
            project_path: project.map(PathBuf::from),
            date_began: None,
            date_last_used: None,
            message_count: content.len(),
            preview_messages: content
                .iter()
                .map(|c| MessagePreview {
                    role: Role::User,
                    content: c.to_string(),
                    timestamp: None,
                })
                .collect(),
            source_path: PathBuf::from("/tmp/x"),
        }
    }

    #[test]
    fn empty_filter_matches_everything() {
        let s = session("Anything", ProviderKind::Claude, None, &[]);
        assert!(Filter::none().matches(&s));
    }

    #[test]
    fn matches_session_name_case_insensitively() {
        let s = session("Alpha Notes", ProviderKind::Claude, None, &[]);
        assert!(Filter::none().with_text("alpha").matches(&s));
        assert!(Filter::none().with_text("ALPHA").matches(&s));
    }

    #[test]
    fn matches_project_path() {
        let s = session(
            "X",
            ProviderKind::Claude,
            Some("/home/dev/projects/widgets"),
            &[],
        );
        assert!(Filter::none().with_text("widgets").matches(&s));
    }

    #[test]
    fn matches_conversation_content() {
        let s = session(
            "X",
            ProviderKind::Claude,
            None,
            &["Please review the staging environment."],
        );
        assert!(Filter::none().with_text("staging environment").matches(&s));
    }

    #[test]
    fn all_terms_must_match() {
        let s = session(
            "Alpha Notes",
            ProviderKind::Claude,
            None,
            &["review the staging environment"],
        );
        // "alpha" is in the name and "staging" in content: both present.
        assert!(Filter::none().with_text("alpha staging").matches(&s));
        // "kubernetes" appears nowhere.
        assert!(!Filter::none().with_text("alpha kubernetes").matches(&s));
    }

    #[test]
    fn provider_restriction_excludes_other_providers() {
        let claude = session("A", ProviderKind::Claude, None, &[]);
        let codex = session("B", ProviderKind::Codex, None, &[]);
        let filter = Filter::none().with_provider(ProviderKind::Claude);
        assert!(filter.matches(&claude));
        assert!(!filter.matches(&codex));
    }

    #[test]
    fn apply_preserves_order_of_matches() {
        let sessions = vec![
            session("Alpha", ProviderKind::Claude, None, &[]),
            session("Other", ProviderKind::Claude, None, &[]),
            session("Alpha Two", ProviderKind::Claude, None, &[]),
        ];
        let result = Filter::none().with_text("alpha").apply(sessions);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].session_name, "Alpha");
        assert_eq!(result[1].session_name, "Alpha Two");
    }
}
