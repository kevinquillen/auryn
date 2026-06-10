//! Fixture-based tests for the Claude Code session provider.
//!
//! The fixtures mirror the real on-disk layout
//! (`<root>/<encoded-cwd>/<session-id>.jsonl`) and exercise the parser's
//! behavior: title resolution, message counting that excludes meta and
//! tool-only turns, preview extraction, timestamp range, name fallback, and
//! tolerance of corrupt lines.

use std::path::PathBuf;

use auryn::config::AppConfig;
use auryn::models::{ProviderKind, Role};
use auryn::providers::Provider;
use auryn::providers::claude::ClaudeProvider;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude")
}

fn scan() -> Vec<auryn::models::Session> {
    let provider = ClaudeProvider::new(Some(fixtures_root()));
    provider.scan(&AppConfig::default()).unwrap()
}

#[test]
fn scans_sessions_across_project_directories() {
    let sessions = scan();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.provider == ProviderKind::Claude));
}

#[test]
fn parses_title_count_dates_and_preview() {
    let sessions = scan();
    let widget = sessions
        .iter()
        .find(|s| s.provider_session_id == "11111111-1111-1111-1111-111111111111")
        .expect("widget session present");

    assert_eq!(widget.id, "claude:11111111-1111-1111-1111-111111111111");
    assert_eq!(widget.session_name, "Open Tasks Review");
    assert_eq!(
        widget.project_path,
        Some(PathBuf::from("/home/dev/projects/widget"))
    );

    // Counts only readable user/assistant turns: u1, a1, u2, a3. The meta
    // record, the tool-use-only assistant turn, and the corrupt line are all
    // excluded.
    assert_eq!(widget.message_count, 4);
    assert_eq!(widget.preview_messages.len(), 4);
    assert_eq!(widget.preview_messages[0].role, Role::User);
    assert!(
        widget.preview_messages[0]
            .content
            .contains("review the open tasks")
    );
    let last = widget.preview_messages.last().unwrap();
    assert_eq!(last.role, Role::Assistant);
    assert!(last.content.contains("Both staging and production"));

    assert_eq!(
        widget.date_began.unwrap().to_rfc3339(),
        "2026-06-01T10:00:00+00:00"
    );
    assert_eq!(
        widget.date_last_used.unwrap().to_rfc3339(),
        "2026-06-08T16:30:00+00:00"
    );
}

#[test]
fn session_name_falls_back_to_first_user_message() {
    let sessions = scan();
    let blog = sessions
        .iter()
        .find(|s| s.provider_session_id == "22222222-2222-2222-2222-222222222222")
        .expect("blog session present");
    // No ai-title record, so the name comes from the first user message.
    assert_eq!(
        blog.session_name,
        "Draft an outline for an article on developer tools."
    );
    assert_eq!(blog.message_count, 2);
}

#[test]
fn oversized_files_are_skipped() {
    let config = AppConfig {
        max_file_bytes: 1,
        ..AppConfig::default()
    };
    let provider = ClaudeProvider::new(Some(fixtures_root()));
    assert!(provider.scan(&config).unwrap().is_empty());
}

#[test]
fn missing_root_yields_empty_without_error() {
    let provider = ClaudeProvider::new(Some(PathBuf::from("/nonexistent/auryn/claude")));
    assert!(provider.scan(&AppConfig::default()).unwrap().is_empty());

    let none = ClaudeProvider::new(None);
    assert!(none.scan(&AppConfig::default()).unwrap().is_empty());
}

#[test]
fn resume_command_targets_native_cli() {
    let sessions = scan();
    let widget = &sessions[0];
    let provider = ClaudeProvider::new(Some(fixtures_root()));
    let command = provider
        .resume_command(widget, &AppConfig::default())
        .unwrap();
    assert_eq!(command.get_program(), "claude");
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[0], "--resume");
}
