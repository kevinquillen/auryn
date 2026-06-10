# ADR-002: Provider Plugin System

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Auryn supports multiple AI coding tools (Claude Code, Codex, Gemini, with more
planned). Each stores sessions differently. We need an extension mechanism that
lets a new provider be added by writing one self-contained unit, without
modifying shared CLI, search, or TUI code, and that can be validated before any
real provider exists.

## Decision Drivers

* Adding a provider must be a local change confined to `providers/`.
* Shared code must depend only on a trait and the normalized model.
* Discovery must treat session files as untrusted input.
* Resume must be expressible as a native, shell-free command.
* The system must be testable without real provider installations.

## Considered Options

* A `Provider` trait with a runtime registry of trait objects.
* An enum-dispatch approach (one `ProviderKind` enum with a match per
  operation).
* Dynamic plugins loaded from shared libraries at runtime.

## Decision Outcome

Chosen option: "a `Provider` trait with a runtime registry." The trait exposes
`kind`, `display_name`, `default_root`, `scan(&AppConfig) -> Result<Vec<Session>>`,
and `resume_command(&Session, &AppConfig) -> Result<Command>`. `build_registry`
returns `Vec<Box<dyn Provider>>`; the `App` iterates it without knowing the
membership. A synthetic `FakeProvider` (gated behind `AURYN_FAKE`) implements the
trait first, validating the scan/preview/resume pipeline end-to-end before the
Claude, Codex, and Gemini scanners are written.

### Consequences

* Good: new providers are added by implementing the trait and registering them;
  no shared code changes.
* Good: trait objects keep the registry heterogeneous and ordered.
* Good: `resume_command` returns a built `std::process::Command`, never a shell
  string, satisfying the security model (ADR-007).
* Bad: trait-object dispatch is dynamic; negligible for this workload.

## Pros and Cons of the Options

### Trait + runtime registry

* Good, because open for extension, closed for modification.
* Good, because trivially mockable via the fake provider.

### Enum dispatch

* Good, because no dynamic dispatch.
* Bad, because every new provider edits central match arms, defeating the goal.

### Dynamic shared-library plugins

* Good, because third parties could ship providers independently.
* Bad, because unsafe ABI, signing, and load-path concerns far exceed MVP needs.
