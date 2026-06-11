//! Fixture-based tests for the GitHub Copilot CLI session provider.
//!
//! The fixtures mirror the real layout (`<root>/<session-id>/events.jsonl` plus
//! `workspace.yaml`) and exercise the parser: per-session discovery, the name
//! and project path from `workspace.yaml`, message counting that excludes
//! non-message events, text extraction from `data.content`, the name fallback
//! to the first user message when `workspace.yaml` has no name, and tolerance of
//! corrupt lines.

use std::path::PathBuf;

use auryn::config::AppConfig;
use auryn::models::{ProviderKind, Role};
use auryn::providers::Provider;
use auryn::providers::copilot::CopilotProvider;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot")
}

fn scan() -> Vec<auryn::models::Session> {
    CopilotProvider::new(Some(fixtures_root()))
        .scan(&AppConfig::default())
        .unwrap()
}

#[test]
fn scans_sessions_across_directories() {
    let sessions = scan();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.provider == ProviderKind::Copilot));
}

#[test]
fn parses_name_cwd_count_dates_and_preview() {
    let sessions = scan();
    let alpha = sessions
        .iter()
        .find(|s| s.provider_session_id == "11111111-1111-1111-1111-111111111111")
        .expect("alpha session present");

    assert_eq!(alpha.id, "copilot:11111111-1111-1111-1111-111111111111");
    // Name comes from workspace.yaml.
    assert_eq!(alpha.session_name, "Review Rust CLI Code");
    assert_eq!(
        alpha.project_path,
        Some(PathBuf::from("/home/dev/projects/alpha"))
    );

    // Two user + two assistant messages. session.start, turn_start, the tool
    // event, the system message, and the corrupt line are all excluded.
    assert_eq!(alpha.message_count, 4);
    assert_eq!(alpha.preview_messages.len(), 4);
    assert_eq!(alpha.preview_messages[0].role, Role::User);
    assert!(
        alpha.preview_messages[0]
            .content
            .contains("review the open tasks")
    );
    let last = alpha.preview_messages.last().unwrap();
    assert_eq!(last.role, Role::Assistant);
    assert!(last.content.contains("Both staging and production"));

    // Dates come from workspace.yaml created_at/updated_at.
    assert_eq!(
        alpha.date_began.unwrap().to_rfc3339(),
        "2026-06-11T15:52:06.798+00:00"
    );
    assert_eq!(
        alpha.date_last_used.unwrap().to_rfc3339(),
        "2026-06-11T15:55:36.388+00:00"
    );
}

#[test]
fn name_falls_back_to_first_user_message_without_workspace_name() {
    let sessions = scan();
    let service = sessions
        .iter()
        .find(|s| s.provider_session_id == "22222222-2222-2222-2222-222222222222")
        .expect("service session present");
    assert_eq!(
        service.session_name,
        "How should the service handle configuration?"
    );
    assert_eq!(service.message_count, 2);
}

#[test]
fn oversized_event_logs_are_skipped() {
    let config = AppConfig {
        max_file_bytes: 1,
        ..AppConfig::default()
    };
    let sessions = CopilotProvider::new(Some(fixtures_root()))
        .scan(&config)
        .unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn missing_root_yields_empty_without_error() {
    let provider = CopilotProvider::new(Some(PathBuf::from("/nonexistent/auryn/copilot")));
    assert!(provider.scan(&AppConfig::default()).unwrap().is_empty());
}

#[test]
fn resume_command_targets_native_cli() {
    let sessions = scan();
    let provider = CopilotProvider::new(Some(fixtures_root()));
    let command = provider
        .resume_command(&sessions[0], &AppConfig::default())
        .unwrap();
    assert_eq!(command.get_program(), "copilot");
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args[0].starts_with("--resume="));
}
