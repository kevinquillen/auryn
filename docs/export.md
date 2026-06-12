# Export

`auryn export` writes a session's full conversation to a portable document, for
archiving it, reading it outside the originating tool, or carrying its content
into a new session in a different client.

## Usage

```bash
auryn export <session-id>                  # Markdown to stdout (default)
auryn export <session-id> --format json    # JSON to stdout
auryn export <session-id> --out chat.md    # write to a file
auryn export <session-id> --format json --out chat.json
```

The session id uses the same `provider:session_id` form as `resume`, for example
`claude:abc-123`. Run `auryn list --json` to see full ids. A bare id without a
provider prefix is rejected.

Output goes to standard output unless `--out <path>` is given, so export composes
with redirection and pipes:

```bash
auryn export claude:abc-123 > conversation.md
auryn export codex:def-456 --format json | jq '.messages | length'
```

## What is exported

Export reads the full, untruncated conversation from the session file, not the
bounded preview shown in the list. It includes every readable user and assistant
turn in chronological order, with the same filtering the list uses: injected
context (such as Codex's AGENTS.md turns or Gemini's session context), tool-only
turns, and provider metadata are excluded.

### Markdown

A metadata header (provider, session id, project path, dates, message count)
followed by a `## User` or `## Assistant` heading and the content for each turn.

### JSON

An object with session metadata and a `messages` array of `{ role, content,
timestamp }`. Absent optional fields (project path, dates) are omitted rather
than emitted as null.

## Notes on migrating between tools

There is no cross-provider "resume": a session id is a provider's private handle,
the on-disk formats differ structurally, and a conversation is tied to the model
that produced it. The supported migration path is to export a conversation and
seed a new session in the target tool with that content. Export is read-only and
never writes provider state. See `docs/adr/0008-session-export.md`.
