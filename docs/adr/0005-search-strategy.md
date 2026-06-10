# ADR-005: Search Strategy

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Search is a first-class feature: users filter sessions by name, project path,
provider, and conversation content, and it must feel instant. We must choose how
sessions are searched without prematurely adopting heavy indexing
infrastructure.

## Decision Drivers

* Must feel instant for realistic local session counts.
* Local-first: no external services or background daemons.
* Must search metadata and preview content together.
* Case-insensitive by default.
* Must leave room for fuzzy, regex, and full-text indexing later.

## Considered Options

* In-memory filtering over scanned sessions (term-wise substring matching).
* An embedded full-text index (e.g. Tantivy) built at startup.
* An on-disk SQLite cache with FTS.

## Decision Outcome

Chosen option: "in-memory filtering initially." A composable `Filter` combines
an optional provider restriction with an optional free-text query; text matching
is case-insensitive and term-wise conjunctive across a session's searchable text
(name, provider, project path, and preview content). For the session counts a
single user accumulates locally this is effectively instant and requires no
index build, cache invalidation, or extra dependency. Regex, boolean queries,
and a persistent full-text index remain future work behind the same
`Filter`/search API without changing callers.

### Realized matching (Phase 6)

Within the in-memory strategy, each query term is matched two ways, mirroring
the spec's split between metadata and content search:

* **Metadata** (name, provider, project path) is matched *fuzzily* via
  `fuzzy-matcher` (SkimMatcherV2), so short or mistyped queries still find a
  title and the fuzzy score provides ranking.
* **Content** (conversation text) is matched as a case-insensitive *substring*
  ("equivalent to grep" per the spec), avoiding the noise of fuzzy-subsequence
  matches across large transcripts.

Terms are conjunctive (all must match); metadata hits contribute their fuzzy
score and content-only hits a floor score, so results rank title matches above
content matches. With no query, recency order is preserved.

### Consequences

* Good: zero index-maintenance cost; nothing to invalidate when sessions change.
* Good: trivially testable; scoring is a pure function over text.
* Good: the metadata-fuzzy/content-grep split keeps fuzzy's forgiveness for
  titles without flooding results with incidental content subsequences.
* Bad: a full content scan over very large histories will eventually want an
  index; the API is shaped so that change stays internal to `search/`.

## Pros and Cons of the Options

### In-memory filtering

* Good, because simplest, dependency-free, and instant at MVP scale.
* Bad, because linear in total content size.

### Embedded full-text index

* Good, because scales to very large corpora with ranking.
* Bad, because index build time, on-disk state, and a heavy dependency for a
  problem we do not yet have.

### SQLite + FTS

* Good, because durable and queryable.
* Bad, because introduces a cache to keep in sync with provider-owned files,
  which Auryn must never mutate (ADR-007).
