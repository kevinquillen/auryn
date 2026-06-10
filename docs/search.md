# Search and filtering

Search is a first-class feature and runs entirely in memory over the scanned
sessions. See `docs/adr/0005-search-strategy.md`.

## How matching works

A query is split into whitespace-separated terms, and matching is conjunctive:
every term must match. Each term is matched two ways:

* Session metadata (name, provider, project path) is matched fuzzily, so short
  or mistyped queries still find a title. The fuzzy score ranks the best title
  matches first.
* Conversation content (the preview turns) is matched as a case-insensitive
  substring, which is equivalent to grep and avoids the noise of fuzzy
  subsequence matches across large transcripts.

A term that matches metadata contributes its fuzzy score; a term that matches
only content contributes a floor score. Results are ranked best first, so title
matches sort above content-only matches. With no query, results keep their
recency order.

All matching is case-insensitive.

## Command line

```bash
auryn filter "open tasks"
auryn search "config"        # alias for filter
auryn filter "drpl" --json   # fuzzy: matches a "Drupal" title
```

## In the TUI

Press `/` to open the command line, then type a slash command:

```text
/filter <text>      filter by text
/provider <name>    show only one provider (or "all")
/clear              clear all filters
/refresh            rebuild the session index
/help               toggle help
```

A `/` entry whose first word is not a known command is treated as a quick filter,
so `/drupal` filters by "drupal". Press P to cycle the provider filter directly.

The visible list re-ranks as you type, with the best matches first.

## Future work

Regular expressions, boolean queries, and a persistent full-text index are
possible future enhancements behind the same search API.
