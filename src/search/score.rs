//! Scoring for session search.
//!
//! Search is term-wise conjunctive: every whitespace-separated query term must
//! match. Each term is matched two ways, matching the spec's split between
//! metadata and content search:
//!
//! * **Metadata** (name, project path, provider) is matched *fuzzily*, so short
//!   or mistyped queries still find a title, and the fuzzy score ranks the best
//!   title matches first.
//! * **Content** (conversation text) is matched as a case-insensitive
//!   *substring* -- "equivalent to grep" per the spec -- which avoids the noise
//!   of fuzzy-subsequence matches across large transcripts.
//!
//! A term that hits metadata contributes its fuzzy score; a term that only hits
//! content contributes a floor score, so content-only matches are included but
//! rank below any title match. All matching is case-insensitive.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

/// Score contributed by a term that matches conversation content but not
/// metadata; below any positive fuzzy metadata score so titles rank first.
const CONTENT_SCORE: i64 = 0;

/// Builds the configured fuzzy matcher. Construct once and reuse across a batch
/// of candidates; `ignore_case` makes matching case-insensitive by default.
pub fn matcher() -> SkimMatcherV2 {
    SkimMatcherV2::default().ignore_case()
}

/// Scores a candidate against a multi-term `query`. `metadata` is matched
/// fuzzily; `content` (which must be lowercased) is matched as a substring.
/// Returns `None` if any term matches neither, `Some(0)` for an empty query,
/// and otherwise the summed score (higher is a better match).
pub fn score(metadata: &str, content: &str, query: &str, matcher: &SkimMatcherV2) -> Option<i64> {
    let mut total = 0;
    let mut matched_any = false;
    for term in query.split_whitespace() {
        matched_any = true;
        if let Some(term_score) = matcher.fuzzy_match(metadata, term) {
            total += term_score;
        } else if content.contains(&term.to_lowercase()) {
            total += CONTENT_SCORE;
        } else {
            return None;
        }
    }
    if matched_any { Some(total) } else { Some(0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_scores_zero() {
        assert_eq!(score("anything", "", "", &matcher()), Some(0));
        assert_eq!(score("anything", "", "   ", &matcher()), Some(0));
    }

    #[test]
    fn fuzzy_metadata_outranks_content_only_match() {
        let m = matcher();
        let title = score("config service", "", "config", &m).unwrap();
        let content_only = score("unrelated title", "store the config here", "config", &m).unwrap();
        assert!(
            title > content_only,
            "title {title} should beat content-only {content_only}"
        );
    }

    #[test]
    fn metadata_matching_is_fuzzy_but_content_is_substring() {
        let m = matcher();
        // "drpl" fuzzy-matches the "drupal" title.
        assert!(score("drupal graph", "", "drpl", &m).is_some());
        // But a scattered subsequence is NOT accepted inside content.
        assert!(score("unrelated", "debug the parallel loader", "drpl", &m).is_none());
        // A literal content substring is accepted.
        assert!(
            score(
                "unrelated",
                "we need recursive cte support",
                "recursive cte",
                &m
            )
            .is_some()
        );
    }

    #[test]
    fn is_case_insensitive() {
        let m = matcher();
        assert!(score("Alpha Notes", "", "alpha", &m).is_some());
        assert!(score("alpha notes", "", "ALPHA", &m).is_some());
        assert!(score("title", "staging environment", "STAGING", &m).is_some());
    }

    #[test]
    fn all_terms_must_match() {
        let m = matcher();
        assert!(score("alpha service", "config defaults", "alpha config", &m).is_some());
        assert!(score("alpha service", "config defaults", "alpha kubernetes", &m).is_none());
    }
}
