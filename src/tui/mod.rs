//! Terminal user interface.
//!
//! This module owns the interactive loop and terminal lifecycle; all decision
//! logic lives in the testable submodules: [`state`] (interface state),
//! [`event`] (key handling), [`commands`] (slash commands), and [`view`]
//! (rendering). The loop scans once up front, redraws on every event, and
//! returns the id of a session to resume (or `None` to just exit) so the caller
//! performs the actual provider hand-off.

pub mod commands;
pub mod event;
pub mod state;
pub mod view;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event as term_event;

use crate::app::App;
use crate::errors::Result;
use event::Action;
use state::TuiState;

/// Launches the interface, runs until the user quits or chooses to resume, and
/// restores the terminal. Returns the Auryn id of a session to resume, if any.
pub fn run(app: &App) -> Result<Option<String>> {
    let outcome = app.scan_all();
    let mut state = TuiState::new(outcome.sessions);
    if !outcome.errors.is_empty() {
        state.set_status(format!(
            "{} provider(s) failed to scan; results may be incomplete",
            outcome.errors.len()
        ));
    }

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, app, &mut state);
    ratatui::restore();
    result
}

/// The redraw-and-handle loop. Returns the session id to resume, or `None`.
fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &App,
    state: &mut TuiState,
) -> Result<Option<String>> {
    loop {
        terminal.draw(|frame| view::render(frame, state))?;

        if let term_event::Event::Key(key) = term_event::read()? {
            match event::handle_key(state, key) {
                Action::Quit => return Ok(None),
                Action::Resume(session_id) => return Ok(Some(session_id)),
                Action::Refresh => {
                    let outcome = app.scan_all();
                    state.set_sessions(outcome.sessions);
                    state.set_status("Refreshed session index");
                }
                Action::None => {}
            }
        }
    }
}
