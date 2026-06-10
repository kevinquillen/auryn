# ADR-007: Security Model

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Auryn reads session files written by other tools and launches provider CLIs to
resume work. Those files are untrusted input, and resuming spawns processes.
Auryn must establish hard boundaries: it is a read-only, local-first discovery
and resume tool, not a credential manager, proxy, or router.

## Decision Drivers

* Never store, read, or transmit credentials.
* Never mutate provider configuration or session files.
* Treat all session files as untrusted input.
* Never execute commands found inside provider metadata.
* Never spawn processes through a shell.

## Considered Options

* Read-only access with explicit, argument-built process spawning.
* Convenience-first access that may write back caches or pass paths to a shell.

## Decision Outcome

Chosen option: "read-only access with explicit, argument-built spawning." Auryn
is read-only against provider session storage and never writes to
provider-owned paths. Parsing is defensive: file sizes are bounded
(`max_file_bytes`), malformed content is skipped rather than trusted, and a
single corrupt file never aborts discovery. Resume builds a
`std::process::Command` argument-by-argument (e.g. `Command::new("claude")
.arg("--resume")`) and never invokes `sh -c` or `cmd /c`, so content from a
session file can never be interpreted as a command. No credentials, API keys,
routing, proxying, telemetry, or background daemons are involved. The fake
provider is gated behind `AURYN_FAKE` so synthetic data never appears in normal
use.

### Consequences

* Good: a malicious or corrupt session file cannot achieve code execution or
  cause data loss in provider files.
* Good: the trust boundary is explicit and testable (e.g. resume commands assert
  no shell program).
* Bad: building commands argument-by-argument is more verbose than a shell
  string; this is the intended trade-off.

## Pros and Cons of the Options

### Read-only, argument-built spawning

* Good, because it eliminates shell-injection and file-mutation risk by design.
* Good, because it matches the stated non-goals (no credentials, no routing).

### Convenience-first access

* Good, because slightly less code.
* Bad, because shell spawning and cache writes reintroduce exactly the risks the
  product promises to avoid.
