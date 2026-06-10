//! Fixture-based tests for the Gemini CLI session provider.
//!
//! The fixtures mirror the real layout
//! (`<root>/<project>/chats/session-*.jsonl` plus `<project>/.project_root`) and
//! exercise the parser: per-project discovery, project path from
//! `.project_root`, de-duplication of streamed message rewrites by id, text
//! extraction from string and parts content, exclusion of the injected
//! `<session_context>` turn and tool-only messages, the timestamp range, and
//! tolerance of corrupt lines.

use std::path::PathBuf;

use auryn::config::AppConfig;
use auryn::models::{ProviderKind, Role};
use auryn::providers::Provider;
use auryn::providers::gemini::GeminiProvider;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gemini")
}

fn scan() -> Vec<auryn::models::Session> {
    GeminiProvider::new(Some(fixtures_root()))
        .scan(&AppConfig::default())
        .unwrap()
}

#[test]
fn scans_sessions_across_projects() {
    let sessions = scan();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.provider == ProviderKind::Gemini));
}

#[test]
fn parses_id_project_count_dates_and_dedupes() {
    let sessions = scan();
    let alpha = sessions
        .iter()
        .find(|s| s.provider_session_id == "14d31fca-6eca-42ed-a940-46242cbff357")
        .expect("alpha session present");

    assert_eq!(alpha.id, "gemini:14d31fca-6eca-42ed-a940-46242cbff357");
    assert_eq!(
        alpha.project_path,
        Some(PathBuf::from("/home/dev/projects/alpha"))
    );

    // The injected <session_context> turn and the tool-only user message are
    // excluded; the streamed gemini message (written twice under id m2) counts
    // once. Conversational messages: m1, m2, m4, m5.
    assert!(!alpha.session_name.contains("session_context"));
    assert_eq!(alpha.session_name, "How do I run the tests?");
    assert_eq!(alpha.message_count, 4);
    assert_eq!(alpha.preview_messages.len(), 4);

    // De-dup keeps the completed text, not the partial "Use cargo".
    let gemini_turn = alpha
        .preview_messages
        .iter()
        .find(|m| m.role == Role::Assistant)
        .unwrap();
    assert!(
        gemini_turn
            .content
            .contains("Use cargo test to run all tests.")
    );

    let last = alpha.preview_messages.last().unwrap();
    assert_eq!(last.role, Role::Assistant);
    assert!(last.content.contains("Both staging and production"));

    assert_eq!(
        alpha.date_began.unwrap().to_rfc3339(),
        "2026-06-10T17:32:00+00:00"
    );
    assert_eq!(
        alpha.date_last_used.unwrap().to_rfc3339(),
        "2026-06-10T17:35:00+00:00"
    );
}

#[test]
fn oversized_files_are_skipped() {
    let config = AppConfig {
        max_file_bytes: 1,
        ..AppConfig::default()
    };
    let sessions = GeminiProvider::new(Some(fixtures_root()))
        .scan(&config)
        .unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn missing_root_yields_empty_without_error() {
    let provider = GeminiProvider::new(Some(PathBuf::from("/nonexistent/auryn/gemini")));
    assert!(provider.scan(&AppConfig::default()).unwrap().is_empty());
}

#[test]
fn session_source_path_points_at_the_chat_file() {
    // Resume command building (index vs file fallback) is unit-tested in the
    // provider module; here we just confirm the source path used for resume.
    let sessions = scan();
    let alpha = sessions
        .iter()
        .find(|s| s.provider_session_id == "14d31fca-6eca-42ed-a940-46242cbff357")
        .unwrap();
    assert!(
        alpha
            .source_path
            .to_string_lossy()
            .contains("session-2026-06-10T17-32-14d31fca.jsonl")
    );
}
