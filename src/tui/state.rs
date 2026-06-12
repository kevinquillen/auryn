//! Interactive state for the TUI, decoupled from rendering and terminal I/O.
//!
//! [`TuiState`] owns the scanned sessions, the active filter, the current
//! selection, and the independently-tracked preview scroll offset. Every state
//! transition is a plain method so the behavior the spec cares about --
//! selection movement, preview updates, search mode, and independent preview
//! scrolling -- is unit-testable without a real terminal.

use crate::models::{ProviderKind, Session};
use crate::search::Filter;

/// Number of sessions shown per page in the list view.
pub const PAGE_SIZE: usize = 25;

/// Input mode the interface is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Navigating the session list and issuing single-key actions.
    Normal,
    /// Editing the `/` command line.
    Command,
}

/// All mutable interface state.
#[derive(Debug, Clone)]
pub struct TuiState {
    /// Every scanned session, unfiltered, in recency order.
    sessions: Vec<Session>,
    /// Indices into `sessions` that pass the active filter, in display order.
    visible: Vec<usize>,
    /// Selected position within `visible` (not into `sessions`).
    selected: usize,
    /// Preview vertical scroll offset, tracked independently of selection.
    preview_scroll: u16,
    mode: Mode,
    /// Text of the `/` command line while in [`Mode::Command`].
    input: String,
    filter: Filter,
    status: Option<String>,
    show_help: bool,
    show_details: bool,
}

impl TuiState {
    /// Builds initial state from a scanned session list.
    pub fn new(sessions: Vec<Session>) -> Self {
        let mut state = TuiState {
            sessions,
            visible: Vec::new(),
            selected: 0,
            preview_scroll: 0,
            mode: Mode::Normal,
            input: String::new(),
            filter: Filter::none(),
            status: None,
            show_help: false,
            show_details: false,
        };
        state.recompute_visible();
        state
    }

    /// Replaces the session list (e.g. after a refresh), preserving the current
    /// filter and keeping the selection on the same session id when possible.
    pub fn set_sessions(&mut self, sessions: Vec<Session>) {
        let previous_id = self.selected_session().map(|s| s.id.clone());
        self.sessions = sessions;
        self.recompute_visible();
        if let Some(id) = previous_id
            && let Some(pos) = self.visible.iter().position(|&i| self.sessions[i].id == id)
        {
            self.selected = pos;
        }
    }

    /// Recomputes the visible set from the current filter and clamps selection.
    /// Visible sessions are ranked best-match-first when a text query is active.
    fn recompute_visible(&mut self) {
        self.visible = self.filter.rank_indices(&self.sessions);
        if self.selected >= self.visible.len() {
            self.selected = self.visible.len().saturating_sub(1);
        }
        self.preview_scroll = 0;
    }

    // --- Read accessors used by the view ------------------------------------

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn show_help(&self) -> bool {
        self.show_help
    }

    pub fn show_details(&self) -> bool {
        self.show_details
    }

    pub fn preview_scroll(&self) -> u16 {
        self.preview_scroll
    }

    /// The position of the selection within the visible list.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The sessions currently visible under the active filter.
    pub fn visible_sessions(&self) -> impl Iterator<Item = &Session> {
        self.visible.iter().map(|&i| &self.sessions[i])
    }

    // --- Pagination ---------------------------------------------------------

    /// Zero-based index of the page the selection currently falls on. The page
    /// is derived from the selection so there is no separate page state to keep
    /// in sync as the selection or filter changes.
    pub fn current_page(&self) -> usize {
        self.selected / PAGE_SIZE
    }

    /// Total number of pages, always at least one even when nothing is visible.
    pub fn page_count(&self) -> usize {
        self.visible.len().div_ceil(PAGE_SIZE).max(1)
    }

    /// The selection's position within the current page, for row highlighting.
    pub fn selected_on_page(&self) -> usize {
        self.selected % PAGE_SIZE
    }

    /// The sessions on the current page, in display order.
    pub fn page_sessions(&self) -> impl Iterator<Item = &Session> {
        let start = self.current_page() * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(self.visible.len());
        self.visible[start..end].iter().map(|&i| &self.sessions[i])
    }

    /// Number of sessions visible under the active filter.
    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    /// Total number of scanned sessions, ignoring the filter.
    pub fn total_count(&self) -> usize {
        self.sessions.len()
    }

    /// The currently selected session, if any are visible.
    pub fn selected_session(&self) -> Option<&Session> {
        self.visible.get(self.selected).map(|&i| &self.sessions[i])
    }

    /// Looks up an already-scanned session by its Auryn id, without re-scanning.
    pub fn session_by_id(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    // --- Navigation ---------------------------------------------------------

    /// Moves the selection down by one, stopping at the last visible session.
    pub fn select_next(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        if self.selected + 1 < self.visible.len() {
            self.selected += 1;
            self.preview_scroll = 0;
        }
    }

    /// Moves the selection up by one, stopping at the first visible session.
    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.preview_scroll = 0;
        }
    }

    /// Selects the first visible session.
    pub fn select_first(&mut self) {
        self.selected = 0;
        self.preview_scroll = 0;
    }

    /// Selects the last visible session.
    pub fn select_last(&mut self) {
        self.selected = self.visible.len().saturating_sub(1);
        self.preview_scroll = 0;
    }

    /// Moves the selection to the first session of the next page, clamping to
    /// the last session when already on the final page.
    pub fn next_page(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let start = (self.current_page() + 1) * PAGE_SIZE;
        self.selected = start.min(self.visible.len() - 1);
        self.preview_scroll = 0;
    }

    /// Moves the selection to the first session of the previous page, staying on
    /// the first page when already there.
    pub fn previous_page(&mut self) {
        self.selected = self.current_page().saturating_sub(1) * PAGE_SIZE;
        self.preview_scroll = 0;
    }

    /// Scrolls the preview down by `lines`, independently of selection.
    pub fn scroll_preview_down(&mut self, lines: u16) {
        self.preview_scroll = self.preview_scroll.saturating_add(lines);
    }

    /// Scrolls the preview up by `lines`, independently of selection.
    pub fn scroll_preview_up(&mut self, lines: u16) {
        self.preview_scroll = self.preview_scroll.saturating_sub(lines);
    }

    // --- Modes and overlays -------------------------------------------------

    /// Enters the `/` command line with an empty buffer.
    pub fn enter_command(&mut self) {
        self.mode = Mode::Command;
        self.input.clear();
        self.status = None;
    }

    /// Leaves the command line without executing it.
    pub fn cancel_command(&mut self) {
        self.mode = Mode::Normal;
        self.input.clear();
    }

    /// Appends a typed character to the command line.
    pub fn push_input(&mut self, c: char) {
        self.input.push(c);
    }

    /// Deletes the last character of the command line.
    pub fn pop_input(&mut self) {
        self.input.pop();
    }

    /// Toggles the help overlay.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Dismisses the help overlay if shown; returns whether it was open.
    pub fn dismiss_help(&mut self) -> bool {
        let was_open = self.show_help;
        self.show_help = false;
        was_open
    }

    /// Toggles the expanded session-details view in the preview pane.
    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    /// Sets a transient status message shown in the footer area.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    // --- Filtering ----------------------------------------------------------

    /// Sets or clears the free-text portion of the filter and reapplies it.
    pub fn set_text_filter(&mut self, text: &str) {
        self.filter = self.filter.clone().with_text(text);
        self.recompute_visible();
    }

    /// Sets the provider restriction (or clears it with `None`) and reapplies.
    pub fn set_provider_filter(&mut self, provider: Option<ProviderKind>) {
        self.filter.provider = provider;
        self.recompute_visible();
    }

    /// Clears all active filters and reapplies.
    pub fn clear_filters(&mut self) {
        self.filter = Filter::none();
        self.recompute_visible();
    }

    /// Cycles the provider restriction through `None` followed by each provider
    /// that appears in the scanned sessions, in first-seen order.
    pub fn cycle_provider(&mut self) {
        let mut available: Vec<ProviderKind> = Vec::new();
        for session in &self.sessions {
            if !available.contains(&session.provider) {
                available.push(session.provider);
            }
        }
        // Sequence is: None, then each available provider.
        let current_pos = match self.filter.provider {
            None => 0,
            Some(p) => available.iter().position(|&a| a == p).map_or(0, |i| i + 1),
        };
        let next_pos = (current_pos + 1) % (available.len() + 1);
        let next = if next_pos == 0 {
            None
        } else {
            Some(available[next_pos - 1])
        };
        self.set_provider_filter(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn session(name: &str, provider: ProviderKind, content: &[&str]) -> Session {
        use crate::models::{MessagePreview, Role};
        Session {
            id: Session::make_id(provider, name),
            provider,
            provider_session_id: name.to_string(),
            session_name: name.to_string(),
            project_path: None,
            date_began: None,
            date_last_used: None,
            message_count: content.len(),
            preview_messages: content
                .iter()
                .map(|c| MessagePreview {
                    role: Role::User,
                    content: c.to_string(),
                    timestamp: None,
                })
                .collect(),
            source_path: PathBuf::from("/tmp/x"),
        }
    }

    fn sample() -> TuiState {
        TuiState::new(vec![
            session("Alpha Notes", ProviderKind::Claude, &["open tasks"]),
            session("Service Setup", ProviderKind::Codex, &["config defaults"]),
            session("Article Outline", ProviderKind::Gemini, &["five sections"]),
        ])
    }

    #[test]
    fn starts_on_first_session() {
        let state = sample();
        assert_eq!(state.selected_index(), 0);
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Alpha Notes"
        );
        assert_eq!(state.visible_count(), 3);
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut state = sample();
        state.select_next();
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Service Setup"
        );
        state.select_next();
        state.select_next(); // already at last; should not overflow
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Article Outline"
        );
        state.select_previous();
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Service Setup"
        );
        state.select_first();
        assert_eq!(state.selected_index(), 0);
        state.select_last();
        assert_eq!(state.selected_index(), 2);
    }

    #[test]
    fn moving_selection_resets_preview_scroll() {
        let mut state = sample();
        state.scroll_preview_down(5);
        assert_eq!(state.preview_scroll(), 5);
        state.select_next();
        assert_eq!(state.preview_scroll(), 0);
    }

    #[test]
    fn preview_scrolls_independently_of_selection() {
        let mut state = sample();
        let before = state.selected_index();
        state.scroll_preview_down(3);
        state.scroll_preview_down(2);
        assert_eq!(state.preview_scroll(), 5);
        // Scrolling the preview must not move the table selection.
        assert_eq!(state.selected_index(), before);
        state.scroll_preview_up(10); // saturates at 0
        assert_eq!(state.preview_scroll(), 0);
    }

    #[test]
    fn pagination_splits_visible_into_pages_of_25() {
        let many: Vec<Session> = (0..60)
            .map(|i| session(&format!("Session {i}"), ProviderKind::Claude, &["x"]))
            .collect();
        let mut state = TuiState::new(many);
        assert_eq!(state.page_count(), 3);
        assert_eq!(state.current_page(), 0);
        assert_eq!(state.page_sessions().count(), 25);

        state.next_page();
        assert_eq!(state.current_page(), 1);
        assert_eq!(state.selected_index(), 25);
        assert_eq!(state.selected_on_page(), 0);
        assert_eq!(state.page_sessions().count(), 25);

        state.next_page();
        assert_eq!(state.current_page(), 2);
        assert_eq!(state.page_sessions().count(), 10);

        // Already on the last page; selection clamps to the final session.
        state.next_page();
        assert_eq!(state.current_page(), 2);
        assert_eq!(state.selected_index(), 59);

        state.previous_page();
        assert_eq!(state.current_page(), 1);
        assert_eq!(state.selected_index(), 25);
    }

    #[test]
    fn single_page_when_under_limit() {
        let state = sample();
        assert_eq!(state.page_count(), 1);
        assert_eq!(state.current_page(), 0);
        assert_eq!(state.page_sessions().count(), 3);
    }

    #[test]
    fn text_filter_narrows_visible_sessions() {
        let mut state = sample();
        state.set_text_filter("config");
        assert_eq!(state.visible_count(), 1);
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Service Setup"
        );
        state.clear_filters();
        assert_eq!(state.visible_count(), 3);
    }

    #[test]
    fn provider_filter_restricts_to_one_provider() {
        let mut state = sample();
        state.set_provider_filter(Some(ProviderKind::Gemini));
        assert_eq!(state.visible_count(), 1);
        assert_eq!(
            state.selected_session().unwrap().session_name,
            "Article Outline"
        );
    }

    #[test]
    fn cycling_provider_walks_none_then_each_available() {
        let mut state = sample();
        assert_eq!(state.filter().provider, None);
        state.cycle_provider();
        assert_eq!(state.filter().provider, Some(ProviderKind::Claude));
        state.cycle_provider();
        assert_eq!(state.filter().provider, Some(ProviderKind::Codex));
        state.cycle_provider();
        assert_eq!(state.filter().provider, Some(ProviderKind::Gemini));
        state.cycle_provider();
        assert_eq!(state.filter().provider, None);
    }

    #[test]
    fn refresh_preserves_selection_by_id() {
        let mut state = sample();
        state.select_next(); // Service Setup
        let id = state.selected_session().unwrap().id.clone();
        // Rescan returns the same sessions in a different order.
        state.set_sessions(vec![
            session("Article Outline", ProviderKind::Gemini, &[]),
            session("Service Setup", ProviderKind::Codex, &[]),
            session("Alpha Notes", ProviderKind::Claude, &[]),
        ]);
        assert_eq!(state.selected_session().unwrap().id, id);
    }

    #[test]
    fn command_mode_edits_input_buffer() {
        let mut state = sample();
        state.enter_command();
        assert_eq!(state.mode(), Mode::Command);
        state.push_input('f');
        state.push_input('o');
        state.pop_input();
        assert_eq!(state.input(), "f");
        state.cancel_command();
        assert_eq!(state.mode(), Mode::Normal);
        assert_eq!(state.input(), "");
    }
}
