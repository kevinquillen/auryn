//! Application configuration: defaults, TOML (de)serialization, and load/save.
//!
//! Configuration is intentionally small and forward-compatible. Unknown keys
//! are tolerated so that a config written by a newer Auryn does not break an
//! older one, and every field has a default so a missing file is never fatal.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{AurynError, Result};
use crate::models::ProviderKind;
use crate::paths;

/// Number of recent conversational turns a provider should capture for preview.
const DEFAULT_PREVIEW_TURNS: usize = 6;

/// Upper bound on the size of a single session file Auryn will parse, guarding
/// against pathological or malicious files. 16 MiB.
const DEFAULT_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Top-level configuration document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// How many recent turns to retain per session preview.
    pub preview_turns: usize,
    /// Maximum bytes Auryn will read from any single session file.
    pub max_file_bytes: u64,
    /// Per-provider settings.
    pub providers: ProvidersConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            preview_turns: DEFAULT_PREVIEW_TURNS,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            providers: ProvidersConfig::default(),
        }
    }
}

/// Settings for each supported provider. Each provider defaults to enabled via
/// [`ProviderSettings::default`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub claude: ProviderSettings,
    pub codex: ProviderSettings,
    pub gemini: ProviderSettings,
}

impl ProvidersConfig {
    /// Returns the settings for a given provider kind, if Auryn models it as a
    /// user-configurable provider.
    pub fn for_kind(&self, kind: ProviderKind) -> Option<&ProviderSettings> {
        match kind {
            ProviderKind::Claude => Some(&self.claude),
            ProviderKind::Codex => Some(&self.codex),
            ProviderKind::Gemini => Some(&self.gemini),
            ProviderKind::Fake => None,
        }
    }
}

/// Per-provider tuning. `root` overrides the default scan location, which is
/// useful when a tool stores sessions somewhere non-standard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub enabled: bool,
    pub root: Option<PathBuf>,
}

impl ProviderSettings {
    fn enabled() -> Self {
        ProviderSettings {
            enabled: true,
            root: None,
        }
    }
}

impl Default for ProviderSettings {
    fn default() -> Self {
        ProviderSettings::enabled()
    }
}

impl AppConfig {
    /// Loads configuration from the platform config file, falling back to
    /// defaults when no file exists. A malformed file is a hard error so the
    /// user can fix it rather than silently running with wrong settings.
    pub fn load() -> Result<AppConfig> {
        let path = paths::config_file()?;
        if !path.exists() {
            return Ok(AppConfig::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let config: AppConfig = toml::from_str(&raw)?;
        Ok(config)
    }

    /// Serializes the configuration to its canonical TOML form.
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Writes the configuration to the platform config file, creating the
    /// config directory if needed. Returns the path written.
    pub fn save(&self) -> Result<PathBuf> {
        let path = paths::config_file()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, self.to_toml()?)?;
        Ok(path)
    }

    /// Parses configuration from a TOML string. Exposed for testing without
    /// touching the filesystem.
    pub fn from_toml(raw: &str) -> Result<AppConfig> {
        toml::from_str(raw).map_err(AurynError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips_through_toml() {
        let config = AppConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = AppConfig::from_toml(&toml).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // An empty document should produce a fully-defaulted config.
        let parsed = AppConfig::from_toml("").unwrap();
        assert_eq!(parsed, AppConfig::default());
    }

    #[test]
    fn partial_config_keeps_unspecified_defaults() {
        let parsed = AppConfig::from_toml("preview_turns = 3").unwrap();
        assert_eq!(parsed.preview_turns, 3);
        assert_eq!(parsed.max_file_bytes, AppConfig::default().max_file_bytes);
        assert!(parsed.providers.claude.enabled);
    }

    #[test]
    fn unknown_keys_are_tolerated_for_forward_compatibility() {
        let parsed = AppConfig::from_toml("future_setting = true\npreview_turns = 4").unwrap();
        assert_eq!(parsed.preview_turns, 4);
    }

    #[test]
    fn provider_root_override_parses() {
        let toml = r#"
            [providers.claude]
            enabled = true
            root = "/tmp/custom-claude"
        "#;
        let parsed = AppConfig::from_toml(toml).unwrap();
        assert_eq!(
            parsed.providers.claude.root,
            Some(PathBuf::from("/tmp/custom-claude"))
        );
    }
}
