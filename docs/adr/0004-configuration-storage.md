# ADR-004: Configuration Storage

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Auryn needs user configuration (preview depth, file-size bounds, per-provider
enablement and scan-root overrides) stored in a location that respects each
platform's conventions, is human-editable, and never breaks when read by a
different Auryn version than wrote it.

## Decision Drivers

* Follow platform conventions on Linux, macOS, and Windows.
* Human-readable and hand-editable.
* A missing file must never be fatal; defaults must always apply.
* Forward and backward compatible across versions.

## Considered Options

* TOML via `serde`, located with `directories::ProjectDirs`.
* JSON in the same location.
* YAML in the same location.

## Decision Outcome

Chosen option: "TOML located with `directories::ProjectDirs`." The file lives at
`~/.config/auryn/config.toml` (Linux), `~/Library/Application Support/Auryn/`
(macOS), and `%APPDATA%\Auryn\` (Windows). Every field has a default via
`#[serde(default)]`, so an absent or partial file produces a fully valid config;
a malformed file is a hard error so the user can fix it rather than run with
silently wrong settings. Unknown keys are tolerated for forward compatibility.

### Consequences

* Good: `config init/print/path/edit` are straightforward; round-trips are
  tested.
* Good: a newer config does not break an older binary.
* Bad: a corrupt file fails the run; mitigated by a clear error message and
  `config path`/`config edit`.

## Pros and Cons of the Options

### TOML

* Good, because idiomatic for Rust CLI config and pleasant to hand-edit.
* Good, because comment support and an obvious table structure.

### JSON

* Good, because ubiquitous.
* Bad, because no comments and noisier to edit by hand.

### YAML

* Good, because compact.
* Bad, because whitespace-sensitive and a heavier parser dependency.
