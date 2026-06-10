//! Rendering for the TUI.
//!
//! [`render`] draws the whole frame from a [`TuiState`] snapshot: the session
//! table, the independently-scrolled preview pane, a status/command line, and a
//! footer of key hints, plus an optional help overlay. The palette is
//! intentionally terminal-native -- default foreground/background with bold
//! headers, dim metadata, and reverse-video selection -- so Auryn inherits the
//! user's theme rather than imposing one.

use chrono::Utc;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};

use super::commands::REGISTRY;
use super::state::{Mode, TuiState};
use crate::format;
use crate::models::{ProviderKind, Role, Session};

/// Named-ANSI accent for each provider. Named colors (not RGB) are remapped by
/// the terminal emulator to the user's configured palette, so accents stay
/// harmonious with whatever theme the terminal uses.
fn provider_color(provider: ProviderKind) -> Color {
    match provider {
        ProviderKind::Claude => Color::Magenta,
        ProviderKind::Codex => Color::Green,
        ProviderKind::Gemini => Color::Blue,
        ProviderKind::Fake => Color::Yellow,
    }
}

/// Named-ANSI accent for each conversational role in the preview.
fn role_color(role: Role) -> Color {
    match role {
        Role::User => Color::Green,
        Role::Assistant => Color::Cyan,
        Role::System => Color::Yellow,
    }
}

/// Draws the complete interface for the current state.
pub fn render(frame: &mut Frame, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_table(frame, chunks[0], state);
    render_preview(frame, chunks[1], state);
    render_status(frame, chunks[2], state);
    render_footer(frame, chunks[3]);

    if state.show_help() {
        render_help(frame);
    }
}

fn render_table(frame: &mut Frame, area: Rect, state: &TuiState) {
    let now = Utc::now();
    let header = Row::new(["Tool", "Date Began", "Date Last Used", "Messages", "Session Name"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let dim = Style::default().add_modifier(Modifier::DIM);
    let rows = state.visible_sessions().map(|s| {
        Row::new(vec![
            Cell::from(s.provider.display_name())
                .style(Style::default().fg(provider_color(s.provider))),
            Cell::from(format::absolute_date_opt(s.date_began)).style(dim),
            Cell::from(format::relative_time_opt(s.date_last_used, now)).style(dim),
            Cell::from(s.message_count.to_string()).style(dim),
            Cell::from(s.session_name.clone()),
        ])
    });

    let widths = [
        Constraint::Length(8),
        Constraint::Length(14),
        Constraint::Length(16),
        Constraint::Length(8),
        Constraint::Min(10),
    ];

    let title = format!(
        " Sessions ({} of {}) ",
        state.visible_count(),
        state.total_count()
    );
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut table_state = TableState::default();
    if state.visible_count() > 0 {
        table_state.select(Some(state.selected_index()));
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_preview(frame: &mut Frame, area: Rect, state: &TuiState) {
    let block = Block::default().borders(Borders::ALL).title(" Preview ");
    let text = match state.selected_session() {
        Some(session) => preview_text(session, state.show_details()),
        None => Text::from("No session selected."),
    };
    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((state.preview_scroll(), 0));
    frame.render_widget(paragraph, area);
}

/// Builds the preview body with light visual structure: a bold role label per
/// turn, indented content, and a blank line between turns. Optional details
/// prepend dim session metadata.
fn preview_text(session: &Session, show_details: bool) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    let dim = Style::default().add_modifier(Modifier::DIM);

    if show_details {
        lines.push(Line::from(Span::styled(
            format!("id        {}", session.id),
            dim,
        )));
        if let Some(path) = &session.project_path {
            lines.push(Line::from(Span::styled(
                format!("project   {}", path.display()),
                dim,
            )));
        }
        lines.push(Line::from(Span::styled(
            format!(
                "began     {}",
                format::absolute_date_opt(session.date_began)
            ),
            dim,
        )));
        lines.push(Line::from(Span::styled(
            format!("messages  {}", session.message_count),
            dim,
        )));
        lines.push(Line::from(Span::styled(
            format!("source    {}", session.source_path.display()),
            dim,
        )));
        lines.push(Line::from(""));
    }

    if session.preview_messages.is_empty() {
        lines.push(Line::from(Span::styled("(no preview available)", dim)));
    }

    for (i, message) in session.preview_messages.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            message.role.label().to_string(),
            Style::default()
                .fg(role_color(message.role))
                .add_modifier(Modifier::BOLD),
        )));
        for content_line in message.content.lines() {
            lines.push(Line::from(format!("  {content_line}")));
        }
    }

    Text::from(lines)
}

fn render_status(frame: &mut Frame, area: Rect, state: &TuiState) {
    let line = if state.mode() == Mode::Command {
        // Show the command line being edited and place the cursor at its end.
        let text = format!("/{}", state.input());
        frame.set_cursor_position((area.x + text.len() as u16, area.y));
        Line::from(text)
    } else {
        let mut spans = Vec::new();
        if let Some(provider) = state.filter().provider {
            spans.push(Span::styled(
                format!("[{}] ", provider.display_name()),
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(status) = state.status() {
            spans.push(Span::raw(status.to_string()));
        }
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let hint = "Enter Resume   / Filter   P Provider   R Refresh   D Details   ? Help   Q Quit";
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().add_modifier(Modifier::DIM),
        ))),
        area,
    );
}

fn render_help(frame: &mut Frame) {
    let area = centered_rect(60, 50, frame.area());
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Slash commands",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for spec in REGISTRY {
        let usage = if spec.args.is_empty() {
            format!("/{}", spec.name)
        } else {
            format!("/{} {}", spec.name, spec.args)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{usage:<22}"), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(spec.description),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press any key to close",
        Style::default().add_modifier(Modifier::DIM),
    )));

    let popup = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, area);
    frame.render_widget(popup, area);
}

/// Computes a centered rectangle occupying the given percentage of `area`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MessagePreview, ProviderKind, Role};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn session() -> Session {
        Session {
            id: "claude:drupal".to_string(),
            provider: ProviderKind::Claude,
            provider_session_id: "drupal".to_string(),
            session_name: "Drupal Graph".to_string(),
            project_path: Some(PathBuf::from("/home/dev/drupal")),
            date_began: None,
            date_last_used: None,
            message_count: 87,
            preview_messages: vec![MessagePreview {
                role: Role::User,
                content: "We need recursive CTE support.".to_string(),
                timestamp: None,
            }],
            source_path: PathBuf::from("/tmp/drupal.json"),
        }
    }

    /// Renders state into a test backend and returns the buffer's text.
    fn render_to_string(state: &TuiState) -> String {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn renders_session_table_and_preview() {
        let state = TuiState::new(vec![session()]);
        let text = render_to_string(&state);
        assert!(text.contains("Sessions"));
        assert!(text.contains("Drupal Graph"));
        assert!(text.contains("Preview"));
        assert!(text.contains("recursive CTE"));
        assert!(text.contains("Enter Resume"));
    }

    #[test]
    fn renders_empty_state_without_panicking() {
        let state = TuiState::new(vec![]);
        let text = render_to_string(&state);
        assert!(text.contains("No session selected."));
    }

    #[test]
    fn details_toggle_shows_metadata() {
        let mut state = TuiState::new(vec![session()]);
        state.toggle_details();
        let text = render_to_string(&state);
        assert!(text.contains("project"));
        assert!(text.contains("/home/dev/drupal"));
    }

    #[test]
    fn help_overlay_lists_commands() {
        let mut state = TuiState::new(vec![session()]);
        state.toggle_help();
        let text = render_to_string(&state);
        assert!(text.contains("Slash commands"));
        assert!(text.contains("/provider"));
    }
}
