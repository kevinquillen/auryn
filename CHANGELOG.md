# Changelog

All notable changes to Auryn are documented here. The format is based on Keep a
Changelog, and this project adheres to Semantic Versioning.

## [Unreleased]

## [v0.1.1]

* GitHub Copilot CLI provider.

## [v0.1.0]

### Added

* Core models, TOML configuration, and platform path handling.
* Provider plugin system with a normalized session model.
* Claude Code, OpenAI Codex CLI, and Gemini CLI providers.
* Synthetic fake provider for development and architecture validation.
* Ratatui terminal interface: session table, independently scrolling preview
  pane, slash-command framework, help and details overlays, provider cycling,
  and terminal-native color accents.
* Resume hand-off that launches the provider's native CLI attached to the
  terminal and returns its exit code.
* Command-line interface: `list`, `filter`, `search`, `resume`, `doctor`, and
  `config` subcommands, with `--json` output.
* Fuzzy metadata search and substring content search with ranked results.
* Release distribution configuration for GitHub Releases and Homebrew (Scoop
  deferred).

### Fixed

* Codex sessions: injected AGENTS.md and environment context are excluded from
  the session name, count, and preview.
* Gemini sessions: streamed message rewrites are de-duplicated by id, and fork
  lineages created on resume are collapsed in the list.
* Gemini parsing truncates message text at storage time, bounding memory.
* README: clarified that search covers recent preview turns, not the full
  transcript.

### Security

* The resume working directory read from untrusted session files is canonicalized
  and required to be an existing directory before use; otherwise the provider CLI
  launches in the inherited directory rather than an attacker-influenced one.
* `preview_turns` and `max_file_bytes` from configuration are clamped to hard
  ceilings, so a hostile or mistaken config cannot drive heavy CPU or memory use.

### Performance

* Resume builds its command from the already-selected session in the TUI and
  scans only the relevant provider on the command line, avoiding a full re-scan.
