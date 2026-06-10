//! Fixture-based tests for the OpenAI Codex CLI session provider.
//!
//! The fixtures mirror the real nested layout
//! (`<root>/YYYY/MM/DD/rollout-<ts>-<id>.jsonl`) and exercise the rollout
//! parser: recursive discovery, session id and cwd from `session_meta`, message
//! counting that excludes developer/system and non-message items, text
//! extraction from content blocks, the timestamp range, and tolerance of
//! corrupt lines.

use std::path::PathBuf;

use auryn::config::AppConfig;
use auryn::models::{ProviderKind, Role};
use auryn::providers::Provider;
use auryn::providers::codex::CodexProvider;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex")
}

fn scan() -> Vec<auryn::models::Session> {
    CodexProvider::new(Some(fixtures_root()))
        .scan(&AppConfig::default())
        .unwrap()
}

#[test]
fn recursively_scans_rollouts_across_date_directories() {
    let sessions = scan();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.provider == ProviderKind::Codex));
}

#[test]
fn parses_id_cwd_count_dates_and_preview() {
    let sessions = scan();
    let service = sessions
        .iter()
        .find(|s| s.provider_session_id == "aaaa1111-2222-3333-4444-555566667777")
        .expect("service session present");

    assert_eq!(service.id, "codex:aaaa1111-2222-3333-4444-555566667777");
    // No title in Codex, so the name comes from the first user message.
    assert_eq!(
        service.session_name,
        "How should the service handle configuration?"
    );
    assert_eq!(
        service.project_path,
        Some(PathBuf::from("/home/dev/projects/service"))
    );

    // The injected "# AGENTS.md" context and the "<turn_aborted>" marker are
    // not real prompts, so the name is the first genuine user message.
    assert!(!service.session_name.contains("AGENTS.md"));

    // Two user + two assistant messages. The developer message, the injected
    // AGENTS.md and turn-aborted user messages, the reasoning and function_call
    // items, the event_msg, and the corrupt line are all excluded.
    assert_eq!(service.message_count, 4);
    assert_eq!(service.preview_messages.len(), 4);
    assert_eq!(service.preview_messages[0].role, Role::User);
    assert!(
        service.preview_messages[0]
            .content
            .contains("configuration")
    );
    let last = service.preview_messages.last().unwrap();
    assert_eq!(last.role, Role::Assistant);
    assert!(last.content.contains("staging and production"));

    assert_eq!(
        service.date_began.unwrap().to_rfc3339(),
        "2026-06-08T12:00:00+00:00"
    );
    assert_eq!(
        service.date_last_used.unwrap().to_rfc3339(),
        "2026-06-08T12:30:05+00:00"
    );
}

#[test]
fn oversized_files_are_skipped() {
    let config = AppConfig {
        max_file_bytes: 1,
        ..AppConfig::default()
    };
    let sessions = CodexProvider::new(Some(fixtures_root()))
        .scan(&config)
        .unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn missing_root_yields_empty_without_error() {
    let provider = CodexProvider::new(Some(PathBuf::from("/nonexistent/auryn/codex")));
    assert!(provider.scan(&AppConfig::default()).unwrap().is_empty());
}

#[test]
fn resume_command_targets_native_cli() {
    let sessions = scan();
    let provider = CodexProvider::new(Some(fixtures_root()));
    let command = provider
        .resume_command(&sessions[0], &AppConfig::default())
        .unwrap();
    assert_eq!(command.get_program(), "codex");
    let args: Vec<_> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args[0], "resume");
}
