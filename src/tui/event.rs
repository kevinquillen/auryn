//! Translates key events into state changes and high-level actions.
//!
//! [`handle_key`] is the single entry point: it mutates [`TuiState`] for
//! navigation, mode changes, and filtering, and returns an [`Action`] for the
//! things only the run loop can do (quit, resume a session, rescan). Keeping
//! this logic free of terminal I/O makes key handling unit-testable.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::commands::{self, ParsedCommand};
use super::state::{Mode, TuiState};

/// Lines the preview pane scrolls per page-scroll keystroke.
const PREVIEW_PAGE: u16 = 5;

/// A high-level outcome the run loop must act on after a key is handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing for the loop to do; state may have changed.
    None,
    /// Tear down the TUI and exit.
    Quit,
    /// Tear down the TUI and resume the session with this Auryn id.
    Resume(String),
    /// Rescan providers and refresh the session list.
    Refresh,
}

/// Handles a single key event, mutating `state` and returning an [`Action`].
pub fn handle_key(state: &mut TuiState, key: KeyEvent) -> Action {
    // Ignore key-release/repeat events emitted on some platforms.
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }

    // While the help overlay is open, any key simply dismisses it.
    if state.show_help() {
        state.dismiss_help();
        return Action::None;
    }

    match state.mode() {
        Mode::Normal => handle_normal(state, key),
        Mode::Command => handle_command(state, key),
    }
}

fn handle_normal(state: &mut TuiState, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl-guarded preview scrolling must be matched before the unguarded
    // single-key bindings for the same letters (e.g. plain `d` toggles details).
    match key.code {
        KeyCode::Char('d') if ctrl => {
            state.scroll_preview_down(PREVIEW_PAGE);
            return Action::None;
        }
        KeyCode::Char('u') if ctrl => {
            state.scroll_preview_up(PREVIEW_PAGE);
            return Action::None;
        }
        _ => {}
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('/') => {
            state.enter_command();
            Action::None
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            state.cycle_provider();
            announce_provider(state);
            Action::None
        }
        KeyCode::Char('r') | KeyCode::Char('R') => Action::Refresh,
        KeyCode::Char('d') | KeyCode::Char('D') => {
            state.toggle_details();
            Action::None
        }
        KeyCode::Char('?') => {
            state.toggle_help();
            Action::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.select_previous();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.select_next();
            Action::None
        }
        KeyCode::Home | KeyCode::Char('g') => {
            state.select_first();
            Action::None
        }
        KeyCode::End | KeyCode::Char('G') => {
            state.select_last();
            Action::None
        }
        KeyCode::PageUp => {
            state.previous_page();
            Action::None
        }
        KeyCode::PageDown => {
            state.next_page();
            Action::None
        }
        KeyCode::Enter => match state.selected_session() {
            Some(session) => Action::Resume(session.id.clone()),
            None => Action::None,
        },
        _ => Action::None,
    }
}

fn handle_command(state: &mut TuiState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.cancel_command();
            Action::None
        }
        KeyCode::Enter => {
            let parsed = commands::parse(state.input());
            state.cancel_command();
            apply_command(state, parsed)
        }
        KeyCode::Backspace => {
            state.pop_input();
            Action::None
        }
        KeyCode::Char(c) => {
            state.push_input(c);
            Action::None
        }
        _ => Action::None,
    }
}

/// Applies a parsed slash command to the state, returning any loop-level action.
fn apply_command(state: &mut TuiState, parsed: ParsedCommand) -> Action {
    match parsed {
        ParsedCommand::Filter(text) => {
            state.set_text_filter(&text);
            if text.is_empty() {
                state.set_status("Cleared text filter");
            } else {
                state.set_status(format!("Filter: {text}"));
            }
            Action::None
        }
        ParsedCommand::Provider(provider) => {
            state.set_provider_filter(provider);
            announce_provider(state);
            Action::None
        }
        ParsedCommand::Clear => {
            state.clear_filters();
            state.set_status("Cleared all filters");
            Action::None
        }
        ParsedCommand::Help => {
            state.toggle_help();
            Action::None
        }
        ParsedCommand::Refresh => Action::Refresh,
        ParsedCommand::Empty => Action::None,
        ParsedCommand::Invalid(message) => {
            state.set_status(message);
            Action::None
        }
    }
}

/// Sets a status line describing the active provider restriction.
fn announce_provider(state: &mut TuiState) {
    match state.filter().provider {
        Some(p) => state.set_status(format!("Provider: {}", p.display_name())),
        None => state.set_status("Provider: all"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, Session};
    use std::path::PathBuf;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn session(name: &str, provider: ProviderKind) -> Session {
        Session {
            id: Session::make_id(provider, name),
            provider,
            provider_session_id: name.to_string(),
            session_name: name.to_string(),
            project_path: None,
            date_began: None,
            date_last_used: None,
            message_count: 0,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/x"),
        }
    }

    fn sample() -> TuiState {
        TuiState::new(vec![
            session("Alpha Notes", ProviderKind::Claude),
            session("Service Setup", ProviderKind::Codex),
        ])
    }

    #[test]
    fn arrows_move_selection() {
        let mut state = sample();
        assert_eq!(handle_key(&mut state, key(KeyCode::Down)), Action::None);
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Service Setup"
        );
        handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Alpha Notes"
        );
    }

    #[test]
    fn q_quits_and_enter_resumes() {
        let mut state = sample();
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Char('q'))),
            Action::Quit
        );
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(action, Action::Resume("claude:Alpha Notes".to_string()));
    }

    #[test]
    fn slash_enters_command_mode_and_filters() {
        let mut state = sample();
        handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.mode(), Mode::Command);
        for c in "service".chars() {
            handle_key(&mut state, key(KeyCode::Char(c)));
        }
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(action, Action::None);
        assert_eq!(state.mode(), Mode::Normal);
        assert_eq!(state.visible_count(), 1);
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Service Setup"
        );
    }

    #[test]
    fn escape_cancels_command_without_filtering() {
        let mut state = sample();
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Char('x')));
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.mode(), Mode::Normal);
        assert_eq!(state.visible_count(), 2);
    }

    #[test]
    fn refresh_key_requests_refresh() {
        let mut state = sample();
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Char('r'))),
            Action::Refresh
        );
    }

    #[test]
    fn ctrl_d_and_u_scroll_preview_without_moving_selection() {
        let mut state = sample();
        let selected = state.selected_index();
        handle_key(&mut state, ctrl(KeyCode::Char('d')));
        assert_eq!(state.preview_scroll(), PREVIEW_PAGE);
        assert_eq!(state.selected_index(), selected);
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.preview_scroll(), 0);
    }

    #[test]
    fn provider_command_filters_by_provider() {
        let mut state = sample();
        handle_key(&mut state, key(KeyCode::Char('/')));
        for c in "provider codex".chars() {
            handle_key(&mut state, key(KeyCode::Char(c)));
        }
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.visible_count(), 1);
        assert_eq!(
            state.selected_session().unwrap().provider,
            ProviderKind::Codex
        );
    }

    #[test]
    fn help_overlay_toggles_and_dismisses() {
        let mut state = sample();
        handle_key(&mut state, key(KeyCode::Char('?')));
        assert!(state.show_help());
        // Any key dismisses help.
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert!(!state.show_help());
    }
}
