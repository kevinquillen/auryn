# Auryn

[![CI](https://github.com/kevinquillen/auryn/actions/workflows/ci.yml/badge.svg)](https://github.com/kevinquillen/auryn/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/kevinquillen/auryn?display_name=tag&sort=semver)](https://github.com/kevinquillen/auryn/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](https://www.rust-lang.org)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey)](#install)

Auryn is a local-first AI session browser and resumer. It discovers, searches,
previews, and resumes AI coding sessions from multiple providers through a
single terminal interface, so you can return to earlier work regardless of which
tool created the session.

## What Auryn does not do

Auryn is read-only against provider session storage and never modifies
provider-owned files. It does not:

* manage credentials or store API keys
* proxy or transmit traffic
* route or switch models
* modify provider configuration or session files
* run background daemons, telemetry, or analytics

It is a discovery, search, preview, and resume tool, not a router, proxy,
gateway, or credential manager.

## Supported providers

* Claude Code
* OpenAI Codex CLI
* Gemini CLI
* Aider (coming soon)
* CoPilot (coming soon)

Auryn reads each tool's own on-disk session storage. New providers can be added
without changing the TUI or CLI. See `docs/providers.md`.

## Install

Prebuilt binaries for macOS, Linux, and Windows are attached to each
[GitHub Release](https://github.com/kevinquillen/auryn/releases). Download the
archive for your platform, extract it, and put `auryn` on your `PATH`.

Homebrew (macOS and Linux):

```bash
brew install kevinquillen/tap/auryn
```

Windows: download the `.zip` from the latest release, extract it, and put
`auryn.exe` on your `PATH`.

From source (requires a Rust toolchain):

```bash
cargo install --path .
```

## Usage

Launch the interactive TUI:

```bash
auryn
```

In the TUI:

* Up and Down (or k and j) move the selection
* Ctrl-d and Ctrl-u (or PageDown and PageUp) scroll the preview independently
* Enter resumes the selected session
* `/` opens the command line (slash commands)
* P cycles the provider filter
* R refreshes the session index
* D toggles session details
* `?` shows help
* Q or Esc quits

Non-interactive commands:

```bash
auryn list                 # list all sessions
auryn list --json          # machine-readable output
auryn filter "open tasks"  # filter by text
auryn search "config"      # alias for filter
auryn resume <session-id>  # resume a session by id
auryn doctor               # environment and discovery diagnostics
auryn config path          # print the config file path
auryn config print         # print the effective config
auryn config init          # write a default config file
auryn config edit          # open the config in $EDITOR
```

## Search

Search matches session metadata (name, provider, project path) fuzzily and
conversation content as a case-insensitive substring, and ranks the best matches
first. See `docs/search.md`.

## Configuration

Configuration is a TOML file stored in the platform configuration directory
(`~/.config/auryn/` on Linux, `~/Library/Application Support/Auryn/` on macOS,
`%APPDATA%\Auryn\` on Windows). All settings have defaults, so a missing file is
never an error. See `docs/configuration.md`.

## Security

Auryn treats session files as untrusted input, bounds file sizes, tolerates
malformed content, and never executes commands found in session metadata. It
spawns provider commands directly, never through a shell. See `SECURITY.md` and
`docs/adr/0007-security-model.md`.

## Documentation

* `docs/configuration.md` - configuration reference
* `docs/providers.md` - how each provider is discovered and resumed
* `docs/search.md` - search and filtering behavior
* `docs/release.md` - release and packaging process
* `docs/adr/` - architectural decision records

## License

MIT. See `LICENSE`.
