//! End-to-end tests that invoke the compiled `auryn` binary.
//!
//! Each invocation points the fake provider at the JSON fixtures and isolates
//! the config directory via `XDG_CONFIG_HOME`, so output never depends on the
//! developer's real environment. Assertions target stable substrings and
//! parsed JSON rather than relative-time strings, which vary with the clock.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_auryn");

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake")
}

/// Builds a command with an isolated config dir. `fake` controls whether the
/// fake provider is enabled and pointed at the fixtures.
fn auryn(args: &[&str], fake: bool) -> std::process::Output {
    let isolated_config = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-config-home");
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    cmd.env("XDG_CONFIG_HOME", &isolated_config);
    if fake {
        cmd.env("AURYN_FAKE", "1");
        cmd.env("AURYN_FAKE_DIR", fixtures_dir());
    } else {
        cmd.env_remove("AURYN_FAKE");
        cmd.env_remove("AURYN_FAKE_DIR");
    }
    cmd.output().expect("failed to run auryn binary")
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn list_renders_fixture_sessions() {
    let out = auryn(&["list"], true);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Session Name"));
    assert!(text.contains("Recursive CTE Investigation"));
    assert!(text.contains("Minimal Session"));
}

#[test]
fn list_without_providers_reports_no_sessions() {
    let out = auryn(&["list"], false);
    assert!(out.status.success());
    assert!(stdout(&out).contains("No sessions found."));
}

#[test]
fn list_json_emits_parseable_array() {
    let out = auryn(&["list", "--json"], true);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let array = value.as_array().expect("top-level array");
    assert_eq!(array.len(), 2);
    assert!(array.iter().all(|s| s["provider"] == "fake"));
}

#[test]
fn filter_narrows_to_matching_sessions() {
    let out = auryn(&["filter", "recursive"], true);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Recursive CTE Investigation"));
    assert!(!text.contains("Minimal Session"));
}

#[test]
fn search_is_an_alias_for_filter() {
    let filter = stdout(&auryn(&["filter", "recursive"], true));
    let search = stdout(&auryn(&["search", "recursive"], true));
    assert_eq!(filter, search);
}

#[test]
fn doctor_reports_session_count() {
    let out = auryn(&["doctor"], true);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Auryn doctor"));
    assert!(text.contains("sessions found: 2"));
}

#[test]
fn config_print_shows_defaults() {
    let out = auryn(&["config", "print"], false);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("preview_turns"));
    assert!(text.contains("[providers.claude]"));
}

#[test]
fn resume_reports_shell_free_command() {
    let out = auryn(&["resume", "fake:fixture-valid-001"], true);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Would resume by running: echo resume fake:fixture-valid-001"));
}

#[test]
fn resume_unknown_session_fails() {
    let out = auryn(&["resume", "fake:does-not-exist"], true);
    assert!(!out.status.success());
}
