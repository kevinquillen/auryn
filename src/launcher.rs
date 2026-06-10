//! Process hand-off for resuming a session.
//!
//! The launcher takes a fully-built [`Command`] from a provider and runs it
//! attached to the current terminal, so the provider's own CLI takes over until
//! it exits. When resuming from the TUI, the caller restores the terminal
//! (leaves the alternate screen, disables raw mode) before invoking the
//! launcher, so the child starts with a clean, normal terminal.
//!
//! The command is always built argument-by-argument by the provider and run
//! directly; no shell is ever involved.

use std::process::{Command, ExitStatus};

use crate::errors::{AurynError, Result};

/// Spawns `command` with inherited stdio, waits for it to finish, and returns
/// its exit code. A failure to launch (e.g. the provider binary is not on
/// `PATH`) is reported as a provider error rather than a panic.
pub fn run(mut command: Command) -> Result<i32> {
    let program = command.get_program().to_string_lossy().into_owned();
    // `status` inherits stdin/stdout/stderr by default and blocks until exit.
    let status = command.status().map_err(|err| {
        AurynError::provider(program, format!("failed to launch resume command: {err}"))
    })?;
    Ok(exit_code(status))
}

/// Maps an [`ExitStatus`] to a process exit code, translating termination by
/// signal (Unix) into the conventional `128 + signal` form.
fn exit_code(status: ExitStatus) -> i32 {
    match status.code() {
        Some(code) => code,
        None => terminated_code(status),
    }
}

/// Exit code for a process that exited without a normal code (Unix: by signal).
#[cfg(unix)]
fn terminated_code(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map_or(1, |signal| 128 + signal)
}

#[cfg(not(unix))]
fn terminated_code(_status: ExitStatus) -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn propagates_success_and_failure_exit_codes() {
        assert_eq!(run(Command::new("true")).unwrap(), 0);
        assert_eq!(run(Command::new("false")).unwrap(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn captures_specific_exit_code() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("exit 7");
        assert_eq!(run(command).unwrap(), 7);
    }

    #[test]
    fn missing_binary_is_a_provider_error_not_a_panic() {
        let command = Command::new("auryn-no-such-binary-xyz");
        let err = run(command).unwrap_err();
        assert!(matches!(err, AurynError::Provider { .. }));
    }
}
