//! Slash-command parsing for the TUI command line.
//!
//! Commands are registered declaratively in [`REGISTRY`] so `/help` and future
//! additions (e.g. `/tag`, `/favorite`) stay data-driven: adding a command is a
//! registry entry plus a match arm in the executor, with no changes to event
//! handling or rendering. Parsing is pure and fully unit-testable. As a
//! convenience, a `/` line whose first word is not a known command is treated
//! as a quick free-text filter, matching the spec's "/ Filter" affordance.

use crate::models::ProviderKind;

/// Metadata describing one slash command, used to drive the `/help` overlay.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub args: &'static str,
    pub description: &'static str,
}

/// The registered slash commands, in display order.
pub const REGISTRY: &[CommandSpec] = &[
    CommandSpec {
        name: "filter",
        args: "<text>",
        description: "Filter sessions by text (also: just type after /).",
    },
    CommandSpec {
        name: "provider",
        args: "<name|all>",
        description: "Show only one provider, or all.",
    },
    CommandSpec {
        name: "clear",
        args: "",
        description: "Clear all active filters.",
    },
    CommandSpec {
        name: "refresh",
        args: "",
        description: "Rebuild the session index.",
    },
    CommandSpec {
        name: "help",
        args: "",
        description: "Toggle this help.",
    },
];

/// The result of parsing a command-line entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    /// Set (or clear, when empty) the free-text filter.
    Filter(String),
    /// Restrict to one provider, or clear the restriction with `None`.
    Provider(Option<ProviderKind>),
    /// Clear all filters.
    Clear,
    /// Toggle the help overlay.
    Help,
    /// Rebuild the session index.
    Refresh,
    /// Nothing actionable (empty input).
    Empty,
    /// A recognized command with an invalid argument; carries a user message.
    Invalid(String),
}

/// Parses a command-line entry. A leading `/` is optional. Unknown leading
/// words fall back to free-text filtering over the whole entry.
pub fn parse(input: &str) -> ParsedCommand {
    let trimmed = input.trim();
    let body = trimmed.strip_prefix('/').unwrap_or(trimmed).trim();
    if body.is_empty() {
        return ParsedCommand::Empty;
    }

    let (head, rest) = match body.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (body, ""),
    };

    match head.to_ascii_lowercase().as_str() {
        "filter" | "search" => ParsedCommand::Filter(rest.to_string()),
        "clear" => ParsedCommand::Clear,
        "help" => ParsedCommand::Help,
        "refresh" => ParsedCommand::Refresh,
        "provider" => parse_provider(rest),
        // Unknown command: treat the full body as a quick filter query.
        _ => ParsedCommand::Filter(body.to_string()),
    }
}

/// Parses the argument to `/provider`. Empty, `all`, or `any` clears the
/// restriction; otherwise the name must resolve to a known provider.
fn parse_provider(arg: &str) -> ParsedCommand {
    let arg = arg.trim();
    if arg.is_empty() || arg.eq_ignore_ascii_case("all") || arg.eq_ignore_ascii_case("any") {
        return ParsedCommand::Provider(None);
    }
    match arg.parse::<ProviderKind>() {
        Ok(kind) => ParsedCommand::Provider(Some(kind)),
        Err(_) => ParsedCommand::Invalid(format!("unknown provider: {arg}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filter_command() {
        assert_eq!(
            parse("/filter open tasks"),
            ParsedCommand::Filter("open tasks".to_string())
        );
    }

    #[test]
    fn search_is_an_alias_for_filter() {
        assert_eq!(
            parse("/search alpha"),
            ParsedCommand::Filter("alpha".to_string())
        );
    }

    #[test]
    fn parses_provider_command() {
        assert_eq!(
            parse("/provider claude"),
            ParsedCommand::Provider(Some(ProviderKind::Claude))
        );
        assert_eq!(parse("/provider all"), ParsedCommand::Provider(None));
        assert_eq!(parse("/provider"), ParsedCommand::Provider(None));
    }

    #[test]
    fn invalid_provider_reports_message() {
        match parse("/provider nope") {
            ParsedCommand::Invalid(msg) => assert!(msg.contains("nope")),
            other => panic!("expected invalid, got {other:?}"),
        }
    }

    #[test]
    fn parses_bare_keyword_commands() {
        assert_eq!(parse("/clear"), ParsedCommand::Clear);
        assert_eq!(parse("/help"), ParsedCommand::Help);
        assert_eq!(parse("/refresh"), ParsedCommand::Refresh);
    }

    #[test]
    fn leading_slash_is_optional() {
        assert_eq!(parse("clear"), ParsedCommand::Clear);
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(parse(""), ParsedCommand::Empty);
        assert_eq!(parse("/"), ParsedCommand::Empty);
        assert_eq!(parse("   "), ParsedCommand::Empty);
    }

    #[test]
    fn unknown_command_falls_back_to_filter() {
        // `/alpha` is not a command, so it filters by "alpha".
        assert_eq!(parse("/alpha"), ParsedCommand::Filter("alpha".to_string()));
        assert_eq!(
            parse("/open tasks"),
            ParsedCommand::Filter("open tasks".to_string())
        );
    }

    #[test]
    fn registry_lists_user_facing_commands() {
        let names: Vec<_> = REGISTRY.iter().map(|c| c.name).collect();
        assert!(names.contains(&"filter"));
        assert!(names.contains(&"provider"));
        assert!(names.contains(&"help"));
    }
}
