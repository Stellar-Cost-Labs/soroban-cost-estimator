//! User configuration file (`config.toml`).
//!
//! Provides defaults for CLI options. Explicit CLI arguments always win over
//! values loaded here. Discovery order:
//!
//! 1. An explicit `--config <path>` (must exist).
//! 2. `$XDG_CONFIG_HOME/soroban-cost-estimator/config.toml`
//! 3. `~/.soroban-cost-estimator/config.toml`
//!
//! A missing file in the default locations is not an error. A file that
//! exists but cannot be read or parsed is reported as [`AppError::Config`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{AppError, AppResult};

/// Name of the config file inside each search directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Defaults loaded from the user's `config.toml`. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    /// Network used when `--network` is not given.
    pub default_network: Option<String>,
    /// Per-network RPC endpoint overrides, keyed by network name.
    #[serde(default)]
    pub rpc_urls: BTreeMap<String, String>,
    /// Output format used when `--format` is not given.
    pub format: Option<String>,
    /// RPC timeout in seconds used when `--timeout` is not given.
    pub timeout_secs: Option<u64>,
}

impl UserConfig {
    /// Parses a config from TOML text. `origin` is used in error messages.
    pub fn from_toml_str(text: &str, origin: &Path) -> AppResult<Self> {
        toml::from_str(text).map_err(|e| {
            AppError::Config(format!("invalid config file {}: {e}", origin.display()))
        })
    }

    /// Reads and parses a config file that is expected to exist.
    pub fn from_file(path: &Path) -> AppResult<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            AppError::Config(format!("cannot read config file {}: {e}", path.display()))
        })?;
        Self::from_toml_str(&text, path)
    }

    /// Loads the effective user config.
    ///
    /// With `explicit = Some(path)` the file must exist. Otherwise the default
    /// locations are searched and an empty config is returned when none exist.
    pub fn load(explicit: Option<&Path>) -> AppResult<Self> {
        if let Some(path) = explicit {
            if !path.is_file() {
                return Err(AppError::Config(format!(
                    "config file not found: {}",
                    path.display()
                )));
            }
            return Self::from_file(path);
        }
        match discover_config_path()? {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default()),
        }
    }

    /// RPC URL configured for `network`, if any.
    pub fn rpc_url_for(&self, network: &str) -> Option<&str> {
        self.rpc_urls.get(network).map(String::as_str)
    }
}

/// Candidate config paths in precedence order (XDG first, then home).
pub fn candidate_config_paths() -> AppResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        paths.push(
            PathBuf::from(xdg)
                .join("soroban-cost-estimator")
                .join(CONFIG_FILE_NAME),
        );
    }
    paths.push(crate::paths::data_dir()?.join(CONFIG_FILE_NAME));
    Ok(paths)
}

/// First existing config file among the default locations.
pub fn discover_config_path() -> AppResult<Option<PathBuf>> {
    Ok(candidate_config_paths()?.into_iter().find(|p| p.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_keys() {
        let text = r#"
default_network = "testnet"
format = "json"
timeout_secs = 45

[rpc_urls]
testnet = "https://t.example"
mainnet = "https://m.example"
"#;
        let cfg = UserConfig::from_toml_str(text, Path::new("x.toml")).unwrap();
        assert_eq!(cfg.default_network.as_deref(), Some("testnet"));
        assert_eq!(cfg.format.as_deref(), Some("json"));
        assert_eq!(cfg.timeout_secs, Some(45));
        assert_eq!(cfg.rpc_url_for("testnet"), Some("https://t.example"));
        assert_eq!(cfg.rpc_url_for("mainnet"), Some("https://m.example"));
        assert_eq!(cfg.rpc_url_for("futurenet"), None);
    }

    #[test]
    fn empty_file_is_default() {
        let cfg = UserConfig::from_toml_str("", Path::new("x.toml")).unwrap();
        assert_eq!(cfg, UserConfig::default());
    }

    #[test]
    fn malformed_toml_is_config_error() {
        let err = UserConfig::from_toml_str("default_network = ", Path::new("bad.toml"))
            .unwrap_err();
        assert!(matches!(err, AppError::Config(ref m) if m.contains("bad.toml")));
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = UserConfig::from_toml_str("netwrk = \"testnet\"", Path::new("x.toml"))
            .unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn explicit_missing_path_errors() {
        let err = UserConfig::load(Some(Path::new("/definitely/not/here.toml"))).unwrap_err();
        assert!(matches!(err, AppError::Config(ref m) if m.contains("not found")));
    }
}
