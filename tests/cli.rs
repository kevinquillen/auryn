//! End-to-end tests that invoke the compiled `auryn` binary.
//!
//! Each invocation isolates the config directory via `XDG_CONFIG_HOME` and
//! points the Claude provider at a non-existent root via `AURYN_CLAUDE_DIR`, so
//! output never depends on the developer's real `~/.claude` or environment.
//! Assertions target stable substrings and parsed JSON rather than
//! relative-time strings, which vary with the clock.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_auryn");

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake")
}

fn claude_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude")
}

fn codex_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex")
}

fn gemini_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gemini")
}

fn copilot_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/copilot")
}

/// Builds a command with an isolated config dir. `fake` controls whether the
/// fake provider is enabled and pointed at the fixtures. Real providers are
/// isolated to non-existent roots by `base_command`.
fn auryn(args: &[&str], fake: bool) -> std::process::Output {
    let mut cmd = base_command(args);
    if fake {
        cmd.env("AURYN_FAKE", "1");
        cmd.env("AURYN_FAKE_DIR", fixtures_dir());
    } else {
        cmd.env_remove("AURYN_FAKE");
        cmd.env_remove("AURYN_FAKE_DIR");
    }
    cmd.output().expect("failed to run auryn binary")
}

/// Runs the binary with the Claude provider pointed at the Claude fixtures and
/// the fake provider disabled.
fn auryn_claude(args: &[&str]) -> std::process::Output {
    let mut cmd = base_command(args);
    cmd.env("AURYN_CLAUDE_DIR", claude_fixtures_dir());
    cmd.output().expect("failed to run auryn binary")
}

/// Runs the binary with the Codex provider pointed at the Codex fixtures and
/// the fake provider disabled.
fn auryn_codex(args: &[&str]) -> std::process::Output {
    let mut cmd = base_command(args);
    cmd.env("AURYN_CODEX_DIR", codex_fixtures_dir());
    cmd.output().expect("failed to run auryn binary")
}

/// Runs the binary with the Gemini provider pointed at the Gemini fixtures and
/// the fake provider disabled.
fn auryn_gemini(args: &[&str]) -> std::process::Output {
    let mut cmd = base_command(args);
    cmd.env("AURYN_GEMINI_DIR", gemini_fixtures_dir());
    cmd.output().expect("failed to run auryn binary")
}

/// Runs the binary with the Copilot provider pointed at the Copilot fixtures and
/// the fake provider disabled.
fn auryn_copilot(args: &[&str]) -> std::process::Output {
    let mut cmd = base_command(args);
    cmd.env("AURYN_COPILOT_DIR", copilot_fixtures_dir());
    cmd.output().expect("failed to run auryn binary")
}

/// Builds a base command that isolates config and points every real provider at
/// a non-existent root, so no test ever reads the developer's real session
/// storage. Individual helpers override a single provider's root as needed.
fn base_command(args: &[&str]) -> Command {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    cmd.env("XDG_CONFIG_HOME", manifest.join("target/test-config-home"));
    cmd.env("AURYN_CLAUDE_DIR", manifest.join("target/test-no-claude"));
    cmd.env("AURYN_CODEX_DIR", manifest.join("target/test-no-codex"));
    cmd.env("AURYN_GEMINI_DIR", manifest.join("target/test-no-gemini"));
    cmd.env("AURYN_COPILOT_DIR", manifest.join("target/test-no-copilot"));
    // Default to the fake provider being off; `auryn` turns it on explicitly.
    cmd.env_remove("AURYN_FAKE");
    cmd.env_remove("AURYN_FAKE_DIR");
    cmd
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
    assert!(text.contains("Task Review"));
    assert!(text.contains("Minimal Session"));
}

#[test]
fn list_reports_no_sessions_when_nothing_discovered() {
    // Claude is registered but its root finds nothing, and the fake provider is
    // disabled, so no sessions are discovered.
    let out = auryn(&["list"], false);
    assert!(out.status.success());
    assert!(stdout(&out).contains("No sessions found."));
}

#[test]
fn list_renders_claude_fixture_sessions() {
    let out = auryn_claude(&["list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Open Tasks Review"));
    assert!(text.contains("Claude"));
}

#[test]
fn filter_matches_claude_conversation_content() {
    let out = auryn_claude(&["filter", "environments"]);
    assert!(out.status.success());
    let text = stdout(&out);
    // The term appears only in the widget session's conversation, so the blog
    // session (whose name contains "outline") is excluded.
    assert!(text.contains("Open Tasks Review"));
    assert!(!text.contains("outline"));
}

#[test]
fn list_renders_codex_fixture_sessions() {
    let out = auryn_codex(&["list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Codex"));
    assert!(text.contains("How should the service handle configuration?"));
}

#[test]
fn list_renders_gemini_fixture_sessions() {
    let out = auryn_gemini(&["list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Gemini"));
    assert!(text.contains("How do I run the tests?"));
    // The injected session-context turn must not surface as a name.
    assert!(!text.contains("session_context"));
}

#[test]
fn list_renders_copilot_fixture_sessions() {
    let out = auryn_copilot(&["list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Copilot"));
    assert!(text.contains("Review Rust CLI Code"));
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
    let out = auryn(&["filter", "task review"], true);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Task Review"));
    assert!(!text.contains("Minimal Session"));
}

#[test]
fn search_is_an_alias_for_filter() {
    let filter = stdout(&auryn(&["filter", "task"], true));
    let search = stdout(&auryn(&["search", "task"], true));
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

// The fake provider's resume command is `echo`, which is a real binary on
// Unix but a shell builtin on Windows, so this hand-off check is Unix-only.
#[cfg(unix)]
#[test]
fn resume_executes_provider_command_and_returns_its_code() {
    // The fake provider's resume command is `echo resume <id>`, so a successful
    // hand-off prints that line and exits 0.
    let out = auryn(&["resume", "fake:fixture-valid-001"], true);
    assert!(out.status.success());
    assert!(stdout(&out).contains("resume fake:fixture-valid-001"));
}

#[test]
fn resume_unknown_session_fails() {
    let out = auryn(&["resume", "fake:does-not-exist"], true);
    assert!(!out.status.success());
}
