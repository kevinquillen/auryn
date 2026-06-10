//! Platform-aware filesystem locations.
//!
//! All knowledge of where configuration lives is centralized here so the rest
//! of the app never hardcodes a path. Provider scan roots live in each
//! provider module, since those follow the individual tool's conventions.

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::errors::{AurynError, Result};

/// Qualifier/organization/application triple handed to [`ProjectDirs`].
///
/// Yields `~/.config/auryn` on Linux, `~/Library/Application Support/Auryn`
/// on macOS, and `%APPDATA%\Auryn` on Windows.
const APP_NAME: &str = "Auryn";

/// Returns the platform project directories, or an error if the OS does not
/// expose a home directory (e.g. some sandboxed environments).
pub fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("", "", APP_NAME).ok_or(AurynError::NoConfigDir)
}

/// Absolute path to the configuration directory, creating nothing.
pub fn config_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().to_path_buf())
}

/// Absolute path to the primary configuration file.
pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}
