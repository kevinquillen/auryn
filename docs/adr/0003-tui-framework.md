# ADR-003: Terminal User Interface Framework Selection

* Status: accepted
* Date: 2026-06-10
* Deciders: Kevin Quillen

## Context and Problem Statement

Auryn's primary interface is an interactive terminal UI: a session table, a
preview pane, a command line, and a help overlay, all of which must feel native
to the user's terminal and remain testable without a real TTY. We need a Rust
TUI framework that supports this layout, cross-platform input, and headless
rendering for tests.

## Decision Drivers

* Cross-platform (macOS, Windows, Linux) rendering and input.
* Immediate-mode rendering that is straightforward to drive from a single state
  snapshot.
* A test backend so views can be rendered and asserted without a terminal.
* Respect terminal-native colors rather than imposing a palette.
* Active maintenance and a healthy ecosystem.

## Considered Options

* Ratatui (with crossterm backend).
* Cursive.
* A hand-rolled renderer over crossterm/termion directly.

## Decision Outcome

Chosen option: "Ratatui with the crossterm backend." Ratatui is the actively
maintained successor to tui-rs, ships a `TestBackend` that renders into an
inspectable buffer (used here for view tests), and re-exports crossterm so the
input and terminal-control versions never drift. Its immediate-mode model fits
Auryn's architecture cleanly: all interface state lives in a `TuiState`, key
handling is a pure function returning high-level actions, and rendering is a
pure function of state. The palette uses only default foreground/background with
bold/dim/reverse modifiers, so Auryn inherits the user's theme (see the Theme
System section of the spec).

### Consequences

* Good: `TestBackend` lets us assert rendered output in CI without a TTY.
* Good: separating `state`/`event`/`commands`/`view` keeps logic testable and
  the run loop tiny.
* Good: crossterm is cross-platform and re-exported, avoiding version conflicts.
* Bad: immediate-mode redraw requires us to track scroll/selection state
  ourselves; this is a deliberate trade for testability and control.

## Pros and Cons of the Options

### Ratatui + crossterm

* Good, because immediate-mode, well-maintained, test backend, themeable.
* Neutral, because the application owns layout and widget state explicitly.

### Cursive

* Good, because higher-level, callback/view-tree model.
* Bad, because the retained view-tree is harder to drive from one state snapshot
  and to render headlessly for assertions.

### Hand-rolled over crossterm

* Good, because zero framework dependency.
* Bad, because we would reimplement layout, widgets, and a diffing renderer for
  no benefit over Ratatui.
