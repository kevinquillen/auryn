# Providers

Auryn reads each tool's own on-disk session storage and normalizes it onto a
common `Session` model. Nothing outside `src/providers/` knows a provider's
on-disk format, and the TUI and CLI contain no provider-specific logic. See
`docs/adr/0002-provider-plugin-system.md`.

Parsing is defensive for every provider: file sizes are bounded by
`max_file_bytes`, files are read line by line so memory stays bounded, malformed
lines are skipped, and only the last `preview_turns` conversational turns are
kept for preview.

## Claude Code

* Storage: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`. The file stem
  is the session id.
* Format: JSONL. Each line has a `type`. `user` and `assistant` lines carry
  messages, an `ai-title` line carries the session name, and other lines are
  metadata. Records flagged `isMeta` or `isSidechain`, and messages with no
  readable text (for example pure tool calls), are excluded from the count and
  preview.
* Name: the `ai-title`, falling back to the first user message.
* Resume: `claude --resume <session-id>`, run in the session's project directory.
* Root override: `AURYN_CLAUDE_DIR`.

## OpenAI Codex CLI

* Storage: `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl`.
* Format: JSONL of `{ type, timestamp, payload }`. A `session_meta` line carries
  the id and project path. A `response_item` line with `payload.type == "message"`
  carries a turn; only `user` and `assistant` roles are conversational, and text
  is read from the content blocks. Function calls, reasoning, and event records
  are skipped.
* Injected context: Codex injects AGENTS.md and environment and instruction
  blocks as `user` messages. These are excluded from the name, count, and
  preview.
* Name: the first real user message.
* Resume: `codex resume <id>`, run in the session's project directory.
* Root override: `AURYN_CODEX_DIR`.

## Gemini CLI

* Storage: `~/.gemini/tmp/<project>/chats/session-<timestamp><id>.jsonl`, with
  the project's real path in a sibling `<project>/.project_root` file. Despite
  the `tmp` name, this is the persistent session store.
* Format: an append-only log of state mutations. A header line carries the
  session id and start time, `$set` records carry partial updates, and message
  records carry turns. Because a streamed message is rewritten under the same
  id, messages are de-duplicated by id (last write wins). The injected
  `<session_context>` user turn is excluded.
* Name: the first real user message.
* Resume: `gemini --session-file <path>`, run in the session's project directory.
* Root override: `AURYN_GEMINI_DIR`.

### Gemini fork behavior

Gemini 0.46 has no resume-by-id. Continuing a session creates a new session with
a fresh id that shares the same conversation origin, so repeated resumes produce
near-duplicate entries. Auryn collapses these in the list: sessions in the same
project whose start time falls within the same second are treated as one
lineage, and only the most recently used is shown. The older fork files remain
on disk; remove them with `gemini --delete-session <index>`.

The in-place `gemini --resume <index>` path is not used because it requires the
`gemini --list-sessions` call, which is very slow, plus a second process launch.

## Adding a provider

Implement the `Provider` trait and register it in `src/providers/mod.rs`. Add
synthetic fixtures under `tests/fixtures/<name>/`. See `CONTRIBUTING.md`.
