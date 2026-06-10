//! Command-line interface: argument parsing and non-interactive subcommands.
//!
//! The bare `auryn` command launches the TUI (Phase 2). Every other entry here
//! is a scriptable, non-interactive subcommand. Handlers return a process exit
//! code so `main` stays a thin wrapper. All session output can be rendered as a
//! human table or as JSON via `--json` for machine consumption.

use std::io::Write;
use std::process::Command;

use chrono::Utc;
use clap::{Parser, Subcommand};

use crate::app::{App, ScanOutcome};
use crate::config::AppConfig;
use crate::errors::{AurynError, Result};
use crate::format;
use crate::models::Session;
use crate::paths;
use crate::search::Filter;

/// Top-level CLI definition.
#[derive(Debug, Parser)]
#[command(
    name = "auryn",
    version,
    about = "Browse, search, preview, and resume AI coding sessions."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CommandKind>,
}

/// All supported subcommands. Absence launches the interactive TUI.
#[derive(Debug, Subcommand)]
pub enum CommandKind {
    /// List all discovered sessions.
    List {
        /// Emit machine-readable JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Filter sessions by free-text query over metadata and content.
    Filter {
        /// The text to match. Multiple words are matched conjunctively.
        query: String,
        #[arg(long)]
        json: bool,
    },
    /// Search sessions; an alias for `filter`.
    Search {
        query: String,
        #[arg(long)]
        json: bool,
    },
    /// Resume a session by id using the provider's native CLI.
    Resume {
        /// The Auryn session id, e.g. `claude:abc-123`.
        session_id: String,
    },
    /// Report environment, configuration, and provider discovery status.
    Doctor,
    /// Inspect and manage configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

/// `auryn config` actions.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the path to the configuration file.
    Path,
    /// Print the effective configuration as TOML.
    Print,
    /// Write a default configuration file if none exists.
    Init,
    /// Open the configuration file in `$EDITOR`.
    Edit,
}

/// Parses arguments and dispatches to the matching handler, returning the
/// process exit code.
pub fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        None => run_tui(),
        Some(CommandKind::List { json }) => cmd_list(json),
        Some(CommandKind::Filter { query, json }) => cmd_filter(&query, json),
        Some(CommandKind::Search { query, json }) => cmd_filter(&query, json),
        Some(CommandKind::Resume { session_id }) => cmd_resume(&session_id),
        Some(CommandKind::Doctor) => cmd_doctor(),
        Some(CommandKind::Config { action }) => cmd_config(action),
    }
}

/// Launches the interactive TUI. The TUI restores the terminal before
/// returning, so on a resume request we hand off directly to the provider's CLI
/// and return its exit code; otherwise we just exit.
fn run_tui() -> Result<i32> {
    let app = App::load()?;
    match crate::tui::run(&app)? {
        Some(command) => crate::launcher::run(command),
        None => Ok(0),
    }
}

fn cmd_list(json: bool) -> Result<i32> {
    let app = App::load()?;
    let outcome = app.scan_all();
    emit_sessions(&outcome, json)
}

fn cmd_filter(query: &str, json: bool) -> Result<i32> {
    let app = App::load()?;
    let filter = Filter::none().with_text(query);
    let outcome = app.scan_filtered(&filter);
    emit_sessions(&outcome, json)
}

fn cmd_resume(session_id: &str) -> Result<i32> {
    let app = App::load()?;
    let command = app.resume_command(session_id)?;
    // Hand off the terminal to the provider's CLI and return its exit code.
    crate::launcher::run(command)
}

fn cmd_doctor() -> Result<i32> {
    let app = App::load()?;
    let config_path = paths::config_file()?;
    let config_exists = config_path.exists();

    println!("Auryn doctor");
    println!("  config file:   {}", config_path.display());
    println!(
        "  config status: {}",
        if config_exists {
            "present"
        } else {
            "using defaults (no file)"
        }
    );
    println!("  preview turns: {}", app.config().preview_turns);
    println!("  max file size: {} bytes", app.config().max_file_bytes);

    println!("  providers:");
    if app.providers().is_empty() {
        println!("    (none registered)");
    }
    for provider in app.providers() {
        let root = provider
            .default_root()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<built-in>".to_string());
        println!("    - {:<8} root: {}", provider.display_name(), root);
    }

    let outcome = app.scan_all();
    println!("  sessions found: {}", outcome.sessions.len());
    for (provider, err) in &outcome.errors {
        println!("  scan error [{provider}]: {err}");
    }

    Ok(0)
}

fn cmd_config(action: ConfigAction) -> Result<i32> {
    match action {
        ConfigAction::Path => {
            println!("{}", paths::config_file()?.display());
            Ok(0)
        }
        ConfigAction::Print => {
            let config = AppConfig::load()?;
            print!("{}", config.to_toml()?);
            Ok(0)
        }
        ConfigAction::Init => {
            let path = paths::config_file()?;
            if path.exists() {
                println!("Configuration already exists at {}", path.display());
                return Ok(0);
            }
            let written = AppConfig::default().save()?;
            println!("Wrote default configuration to {}", written.display());
            Ok(0)
        }
        ConfigAction::Edit => {
            let path = paths::config_file()?;
            if !path.exists() {
                AppConfig::default().save()?;
            }
            let editor = std::env::var("EDITOR").or_else(|_| std::env::var("VISUAL"));
            match editor {
                Ok(editor) => {
                    // Shell-free: spawn the editor binary directly with the path.
                    let status = Command::new(editor).arg(&path).status()?;
                    Ok(status.code().unwrap_or(0))
                }
                Err(_) => {
                    eprintln!("No $EDITOR set. Edit the file directly: {}", path.display());
                    Ok(1)
                }
            }
        }
    }
}

/// Renders a scan outcome as either JSON or a human table, and reports any
/// per-provider scan errors on stderr. A downstream reader closing the pipe
/// (e.g. `auryn list | head`) is treated as a clean exit rather than an error.
fn emit_sessions(outcome: &ScanOutcome, json: bool) -> Result<i32> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let result = if json {
        serde_json::to_writer_pretty(&mut handle, &outcome.sessions)
            .map_err(AurynError::from)
            .and_then(|_| writeln!(handle).map_err(AurynError::from))
    } else {
        write!(handle, "{}", render_table(&outcome.sessions)).map_err(AurynError::from)
    };
    if let Err(err) = result {
        if is_broken_pipe(&err) {
            return Ok(0);
        }
        return Err(err);
    }
    for (provider, err) in &outcome.errors {
        eprintln!("warning: provider {provider} failed to scan: {err}");
    }
    Ok(0)
}

/// True when an error stems from a downstream reader closing the pipe.
fn is_broken_pipe(err: &AurynError) -> bool {
    match err {
        AurynError::Io(io) => io.kind() == std::io::ErrorKind::BrokenPipe,
        AurynError::Json(json) => json.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe),
        _ => false,
    }
}

/// Renders sessions as an aligned, terminal-native text table.
fn render_table(sessions: &[Session]) -> String {
    if sessions.is_empty() {
        return "No sessions found.\n".to_string();
    }

    let now = Utc::now();
    let headers = [
        "Tool",
        "Date Began",
        "Date Last Used",
        "Messages",
        "Session Name",
    ];

    let rows: Vec<[String; 5]> = sessions
        .iter()
        .map(|s| {
            [
                s.provider.display_name().to_string(),
                format::absolute_date_opt(s.date_began),
                format::relative_time_opt(s.date_last_used, now),
                s.message_count.to_string(),
                s.session_name.clone(),
            ]
        })
        .collect();

    let mut widths = headers.map(|h| h.len());
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    let mut out = String::new();
    push_row(&mut out, &headers.map(|h| h.to_string()), &widths);
    for row in &rows {
        push_row(&mut out, row, &widths);
    }
    out
}

fn push_row(out: &mut String, cells: &[String; 5], widths: &[usize; 5]) {
    let mut line = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        let pad = widths[i].saturating_sub(cell.chars().count());
        line.push_str(cell);
        for _ in 0..pad {
            line.push(' ');
        }
    }
    // Trim trailing padding on the final column for clean output.
    out.push_str(line.trim_end());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, Session};
    use std::path::PathBuf;

    fn session(name: &str, provider: ProviderKind, count: usize) -> Session {
        Session {
            id: Session::make_id(provider, name),
            provider,
            provider_session_id: name.to_string(),
            session_name: name.to_string(),
            project_path: None,
            date_began: None,
            date_last_used: None,
            message_count: count,
            preview_messages: Vec::new(),
            source_path: PathBuf::from("/tmp/x"),
        }
    }

    #[test]
    fn empty_table_has_friendly_message() {
        assert_eq!(render_table(&[]), "No sessions found.\n");
    }

    #[test]
    fn table_includes_headers_and_rows() {
        let sessions = vec![session("Alpha Notes", ProviderKind::Claude, 87)];
        let table = render_table(&sessions);
        assert!(table.contains("Session Name"));
        assert!(table.contains("Alpha Notes"));
        assert!(table.contains("Claude"));
        assert!(table.contains("87"));
    }

    #[test]
    fn cli_parses_list_json_flag() {
        let cli = Cli::try_parse_from(["auryn", "list", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(CommandKind::List { json: true })
        ));
    }

    #[test]
    fn cli_parses_filter_query() {
        let cli = Cli::try_parse_from(["auryn", "filter", "open tasks"]).unwrap();
        match cli.command {
            Some(CommandKind::Filter { query, json }) => {
                assert_eq!(query, "open tasks");
                assert!(!json);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn bare_invocation_has_no_subcommand() {
        let cli = Cli::try_parse_from(["auryn"]).unwrap();
        assert!(cli.command.is_none());
    }
}
