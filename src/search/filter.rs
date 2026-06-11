//! Composable session filtering and ranking.
//!
//! A [`Filter`] combines an optional free-text query with an optional provider
//! restriction. Text matching is case-insensitive and term-wise conjunctive:
//! each term must match either the session metadata (name, provider, project
//! path) *fuzzily* or the conversation content as a *substring* (see
//! [`crate::search::score`]). Matches are ranked best-first by summed score, so
//! title hits surface above content-only hits. With no text query, results keep
//! their incoming order (recency).

use fuzzy_matcher::skim::SkimMatcherV2;

use crate::models::{ProviderKind, Session};
use crate::search::score;

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
        self.score(session, &score::matcher()).is_some()
    }

    /// Scores a session against this filter, or `None` if it does not match.
    /// The provider restriction is a hard gate; the text query contributes the
    /// fuzzy score used for ranking.
    fn score(&self, session: &Session, matcher: &SkimMatcherV2) -> Option<i64> {
        if let Some(provider) = self.provider
            && session.provider != provider
        {
            return None;
        }
        match &self.text {
            None => Some(0),
            Some(query) => score::score(
                &metadata_text(session),
                &content_text(session),
                query,
                matcher,
            ),
        }
    }

    /// Returns the indices of matching sessions, ranked best-match-first. Ties
    /// (including the no-text case, where every score is equal) preserve the
    /// input order via a stable sort.
    pub fn rank_indices(&self, sessions: &[Session]) -> Vec<usize> {
        let matcher = score::matcher();
        let mut scored: Vec<(usize, i64)> = sessions
            .iter()
            .enumerate()
            .filter_map(|(i, s)| self.score(s, &matcher).map(|sc| (i, sc)))
            .collect();
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        scored.into_iter().map(|(i, _)| i).collect()
    }

    /// Returns the matching sessions, ranked best-match-first.
    pub fn apply(&self, sessions: Vec<Session>) -> Vec<Session> {
        let order = self.rank_indices(&sessions);
        let mut slots: Vec<Option<Session>> = sessions.into_iter().map(Some).collect();
        order
            .into_iter()
            .map(|i| slots[i].take().expect("each index taken once"))
            .collect()
    }
}

/// Builds the metadata blob (name, provider, project path) matched fuzzily.
fn metadata_text(session: &Session) -> String {
    let mut parts: Vec<String> = vec![
        session.session_name.clone(),
        session.provider.display_name().to_string(),
        session.provider.as_str().to_string(),
    ];
    if let Some(path) = &session.project_path {
        parts.push(path.to_string_lossy().into_owned());
    }
    parts.join("\n")
}

/// Builds the lowercased conversation-content blob matched as a substring.
fn content_text(session: &Session) -> String {
    session
        .preview_messages
        .iter()
        .map(|m| m.content.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n")
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
    fn apply_filters_to_matching_sessions() {
        let sessions = vec![
            session("Alpha", ProviderKind::Claude, None, &[]),
            session("Other", ProviderKind::Claude, None, &[]),
            session("Alpha Two", ProviderKind::Claude, None, &[]),
        ];
        let result = Filter::none().with_text("alpha").apply(sessions);
        assert_eq!(result.len(), 2);
        let names: Vec<_> = result.iter().map(|s| s.session_name.as_str()).collect();
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Alpha Two"));
        assert!(!names.contains(&"Other"));
    }

    #[test]
    fn no_text_filter_preserves_input_order() {
        let sessions = vec![
            session("First", ProviderKind::Claude, None, &[]),
            session("Second", ProviderKind::Claude, None, &[]),
            session("Third", ProviderKind::Claude, None, &[]),
        ];
        let result = Filter::none().apply(sessions);
        let names: Vec<_> = result.iter().map(|s| s.session_name.as_str()).collect();
        assert_eq!(names, ["First", "Second", "Third"]);
    }

    #[test]
    fn ranks_better_matches_first_regardless_of_input_order() {
        // The stronger (exact, contiguous) match should rank ahead of the
        // weaker (scattered subsequence) one, even though it is listed last.
        let sessions = vec![
            session(
                "rich electric cursive notes",
                ProviderKind::Claude,
                None,
                &[],
            ),
            session("recursive plan", ProviderKind::Claude, None, &[]),
        ];
        let result = Filter::none().with_text("recursive").apply(sessions);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].session_name, "recursive plan");
    }

    #[test]
    fn fuzzy_subsequence_matches() {
        let s = session("Drupal Graph", ProviderKind::Claude, None, &[]);
        // "drpl" is a subsequence of "drupal".
        assert!(Filter::none().with_text("drpl").matches(&s));
    }
}
