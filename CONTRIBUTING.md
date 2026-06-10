# Contributing to Auryn

Thanks for your interest in improving Auryn. This guide covers how to build,
test, and propose changes.

## Development setup

Auryn is a Rust project using the 2024 edition. You need a recent stable Rust
toolchain (1.88 or newer).

```bash
git clone https://github.com/kevinquillen/auryn
cd auryn
cargo build
cargo test
```

Before sending a change, make sure these pass:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
```

## Project layout

```text
src/
  main.rs        binary entry point
  lib.rs         library root
  cli.rs         command-line interface
  app.rs         orchestration (scan, filter, resume)
  config.rs      configuration
  launcher.rs    resume process hand-off
  models.rs      core domain models
  paths.rs       platform paths
  errors.rs      error type
  format.rs      date formatting
  providers/     provider implementations and the Provider trait
  search/        filtering and fuzzy scoring
  tui/           terminal interface (state, event, view, commands)
tests/           integration tests and fixtures
docs/            documentation and ADRs
```

## Adding a provider

Implement the `Provider` trait in `src/providers/<name>.rs` and register it in
`src/providers/mod.rs`. The TUI and CLI depend only on the trait and the
normalized `Session` model, so no interface code changes. Add realistic fixture
sessions under `tests/fixtures/<name>/` and tests that cover valid parsing,
tolerance of malformed input, and resume-command generation. See
`docs/providers.md` and `docs/adr/0002-provider-plugin-system.md`.

Never include real session data in fixtures. Hand-author neutral, synthetic
fixtures that mirror the real on-disk format.

## Testing expectations

Tests should verify behavior users care about, not trivial getters. Provider
parsers must be tested against fixtures for valid sessions, malformed input, and
preview generation. Tests must not read a developer's real session storage; use
the per-provider directory override environment variables to point at fixtures.

## Architectural decisions

Significant architectural changes require an Architectural Decision Record in
`docs/adr/`, in MADR format. See the existing records for examples.

## Commit messages

Use Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`,
`chore:`, `perf:`). Keep commits focused and meaningful rather than one large
commit. Mark breaking changes with `!` and a `BREAKING CHANGE:` body.

## Pull requests

* Keep changes scoped and described clearly.
* Ensure formatting, lints, and tests pass.
* Update documentation and the CHANGELOG when behavior changes.
