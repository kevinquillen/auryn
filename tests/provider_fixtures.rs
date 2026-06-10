//! Fixture-based tests for the fake provider's scanning path.
//!
//! These exercise the real discovery logic against an on-disk directory of
//! session files: valid sessions parse, missing fields are tolerated, corrupt
//! files are skipped rather than aborting the scan, and non-JSON files are
//! ignored. Real provider fixtures (Claude, Codex, Gemini) follow the same
//! pattern in later phases.

use std::path::PathBuf;

use auryn::config::AppConfig;
use auryn::models::{ProviderKind, Role};
use auryn::providers::Provider;
use auryn::providers::fake::FakeProvider;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake")
}

#[test]
fn scans_only_valid_json_sessions() {
    let provider = FakeProvider::new(Some(fixtures_dir()));
    let sessions = provider.scan(&AppConfig::default()).unwrap();

    // valid-session.json and missing-fields.json parse; corrupt.json is
    // skipped and notes.txt is ignored for lacking a .json extension.
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.provider == ProviderKind::Fake));
}

#[test]
fn valid_session_is_fully_parsed() {
    let provider = FakeProvider::new(Some(fixtures_dir()));
    let sessions = provider.scan(&AppConfig::default()).unwrap();
    let valid = sessions
        .iter()
        .find(|s| s.provider_session_id == "fixture-valid-001")
        .expect("valid fixture session present");

    assert_eq!(valid.id, "fake:fixture-valid-001");
    assert_eq!(valid.session_name, "Task Review");
    assert_eq!(valid.message_count, 42);
    assert_eq!(
        valid.project_path,
        Some(PathBuf::from("/home/dev/projects/alpha"))
    );
    assert!(valid.date_began.is_some());
    assert!(valid.date_last_used.is_some());
    assert_eq!(valid.preview_messages.first().unwrap().role, Role::User);
}

#[test]
fn missing_fields_are_tolerated() {
    let provider = FakeProvider::new(Some(fixtures_dir()));
    let sessions = provider.scan(&AppConfig::default()).unwrap();
    let minimal = sessions
        .iter()
        .find(|s| s.provider_session_id == "fixture-minimal-002")
        .expect("minimal fixture session present");

    assert_eq!(minimal.session_name, "Minimal Session");
    assert_eq!(minimal.message_count, 0);
    assert!(minimal.project_path.is_none());
    assert!(minimal.date_began.is_none());
    assert!(minimal.preview_messages.is_empty());
}

#[test]
fn oversized_files_are_skipped() {
    // Force every fixture over the limit so none are read.
    let config = AppConfig {
        max_file_bytes: 1,
        ..AppConfig::default()
    };
    let provider = FakeProvider::new(Some(fixtures_dir()));
    let sessions = provider.scan(&config).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn preview_respects_configured_turn_count() {
    let config = AppConfig {
        preview_turns: 1,
        ..AppConfig::default()
    };
    let provider = FakeProvider::new(Some(fixtures_dir()));
    let sessions = provider.scan(&config).unwrap();
    let valid = sessions
        .iter()
        .find(|s| s.provider_session_id == "fixture-valid-001")
        .unwrap();
    // Four turns in the file, only the last retained for preview.
    assert_eq!(valid.preview_messages.len(), 1);
    assert_eq!(valid.preview_messages[0].role, Role::Assistant);
}
