//! Binary entrypoint. Parses arguments, runs the requested command, and maps
//! the result to a process exit code. All real work lives in the library so it
//! can be tested independently of the binary.

use std::process::ExitCode;

fn main() -> ExitCode {
    match auryn::cli::run() {
        Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
        Err(err) => {
            eprintln!("auryn: {err}");
            ExitCode::FAILURE
        }
    }
}
