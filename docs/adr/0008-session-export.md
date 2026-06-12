# ADR-008: Session Export

* Status: accepted
* Date: 2026-06-12
* Deciders: Kevin Quillen

## Context and Problem Statement

Users want a portable copy of a conversation: to archive it, read it outside the
originating tool, or carry its content into a new session in a different client
(for example moving from Copilot to Claude). The session list already normalizes
every provider into a common model, but the `Session` it carries holds only a
bounded preview (the last `preview_turns` turns, each truncated), which is the
wrong source for an archive. The question is what "export" should produce and
how to read the full conversation without weakening the read-only,
untrusted-input posture.

## Decision Drivers

* Produce a faithful, complete record, not a lossy preview snapshot.
* Stay read-only against provider storage and treat session files as untrusted.
* Keep the provider abstraction intact: no provider-specific logic outside
  `providers/`.
* Compose with the shell (stdout by default) and stay scriptable.

## Considered Options

* Full transcript: re-read the session file and emit every readable turn,
  untruncated, via a new `Provider` method.
* Preview snapshot: serialize the already-loaded bounded preview.
* Cross-provider conversion: write another client's native session so it can be
  resumed there.

## Decision Outcome

Chosen option: "full transcript." A `read_messages` method is added to the
`Provider` trait; each provider re-reads its own `source_path` and returns every
readable user/assistant turn in order, untruncated, applying the same size bound
and the same filtering the preview uses (so injected context, tool-only turns,
and meta records are excluded consistently). The scan path is unchanged: it still
builds a bounded preview for fast listing. The shared per-record predicate keeps
the preview and the export in agreement on what counts as a message. An `export`
subcommand renders a `Transcript` (session metadata plus the full messages) to
Markdown or JSON, to stdout by default or to `--out <path>`, using the same
`provider:session_id` id form and prefix routing as resume.

Cross-provider conversion was rejected: a session id is a provider's private
handle, the on-disk formats are structurally different, and a conversation is
tied to the model and tokenizer that produced it. Forging another client's
session state would also violate the read-only rule. The honest migration path is
export plus seeding a new session with that content, which export enables without
Auryn writing any provider state.

### Consequences

* Good: export is a faithful archive and a real basis for migrating content
  between tools.
* Good: read-only and untrusted-input guarantees are preserved; export never
  writes provider state and bounds file reads like scan.
* Good: rendering is provider-agnostic; new providers get export by implementing
  one method.
* Bad: the full read re-parses the file and holds the whole conversation in
  memory (bounded by `max_file_bytes`), a cost paid only on an explicit export.
* Bad: a small amount of per-provider extraction is shared between the preview
  and full-read paths, which must stay in sync; the shared predicate mitigates
  this.

## Pros and Cons of the Options

### Full transcript

* Good, because it is what an archive or migration actually needs.
* Good, because it reuses the normalized model, so output is uniform across
  providers.
* Bad, because it adds a trait method every provider must implement.

### Preview snapshot

* Good, because it is trivial (serialize what is already loaded).
* Bad, because it is lossy and misleading as an "export."

### Cross-provider conversion

* Good, because it would be the most seamless migration in theory.
* Bad, because it is infeasible and unsafe: session ids are private handles,
  formats differ structurally, and continuity is tied to the original model.
