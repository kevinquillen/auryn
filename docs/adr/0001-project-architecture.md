# ADR-001: Project Architecture

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Auryn discovers, indexes, previews, and resumes AI coding sessions from multiple
providers through one terminal interface. The architecture must let new
providers be added without touching CLI or TUI code, must keep provider-specific
parsing isolated, and must be testable without a real terminal or real provider
installations.

## Decision Drivers

* New providers must not require changes to TUI or CLI code.
* Business logic must be unit- and integration-testable in isolation.
* The binary should stay thin so logic can be exercised directly by tests.
* Provider on-disk formats must not leak into shared code.

## Considered Options

* A single binary crate with all logic in `main.rs` and submodules.
* A library crate plus a thin binary, with a normalized domain model and a
  provider trait as the only extension point.
* A multi-crate workspace (separate crates per layer/provider).

## Decision Outcome

Chosen option: "library crate plus thin binary with a normalized model and a
provider trait." A `lib.rs` exposes `models`, `config`, `paths`, `errors`,
`format`, `providers`, `search`, `app`, and `cli`. `main.rs` only maps a parsed
command to a process exit code. Every provider maps its native session format
onto the shared `Session`/`MessagePreview` model, so nothing outside
`providers/` knows how a tool stores sessions, and the TUI (Phase 2) contains no
provider-specific logic.

### Consequences

* Good: integration tests drive the library directly and via the compiled
  binary; the boundary between layers is explicit.
* Good: a fake provider validates the full pipeline before any real scanner
  exists (see ADR-002).
* Bad: a single library crate offers weaker compile-time isolation than a
  workspace; revisited only if build times or coupling become a problem.

## Pros and Cons of the Options

### All logic in the binary

* Good, because simplest to start.
* Bad, because logic cannot be integration-tested without spawning the binary.

### Library plus thin binary

* Good, because testable, with a clear extension seam.
* Neutral, adds a small amount of module wiring.

### Multi-crate workspace

* Good, because strongest isolation.
* Bad, because premature for an MVP; more ceremony than the problem warrants.
