//! User configuration file (`config.toml`).
//!
//! The config file supplies defaults for CLI options. Precedence, lowest to
//! highest:
//!
//! 1. built-in defaults (e.g. `--network testnet`, `--timeout 30`)
//! 2. values from `config.toml`
//! 3. explicit CLI arguments
//!
//! Network and timeout defaults are applied by rewriting clap's default
//! values before parsing (see [`UserConfig::apply_cli_defaults`]), so an
//! argument the user did not type picks up the config value while an explicit
//! argument always wins. RPC URLs from `[rpc_urls]` replace the built-in
//! endpoint for that network (see [`crate::rpc::client::set_endpoint_overrides`]);
//! an explicit `--rpc-url` still takes precedence.
//!
//! # File discovery
//!
//! With `--config <path>` only that file is read, and it must exist.
//! Otherwise the first existing file among these is used:
//!
//! 1. `$XDG_CONFIG_HOME/soroban-cost-estimator/config.toml`
//! 2. the platform config dir (`~/.config` on Linux) +
//!    `soroban-cost-estimator/config.toml`
//! 3. `~/.soroban-cost-estimator/config.toml`
//!
//! No file at any default location is not an error. A file that exists but
//! cannot be read or parsed is reported as [`AppError::Config`].
//!
//! # Example
//!
//! ```toml
//! default_network = "testnet"
//! format = "json"
//! timeout_secs = 30
//!
//! [rpc_urls]
//! testnet = "https://soroban-testnet.stellar.org"
//! mainnet = "https://my-mainnet-rpc.example"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Deserializer};

use crate::cli::OutputFormat;
use crate::error::{AppError, AppResult};

/// Directory name under the platform config dir.
const APP_DIR: &str = "soroban-cost-estimator";

/// Name of the config file inside each search directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Defaults loaded from the user's `config.toml`. Every field is optional.
///
/// Unknown keys are ignored so config files written for newer versions keep
/// working.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UserConfig {
    /// Network used when `--network` is not given. `network` is accepted as
    /// the legacy spelling of this key.
    #[serde(alias = "network")]
    pub default_network: Option<String>,
    /// Per-network RPC endpoints, replacing the built-in endpoint for each
    /// listed network.
    #[serde(default)]
    pub rpc_urls: BTreeMap<String, String>,
    /// Output format used when neither `--format` nor `--json` is given.
    #[serde(default, deserialize_with = "deserialize_format")]
    pub format: Option<OutputFormat>,
    /// RPC timeout in seconds used when `--timeout` is not given.
    pub timeout_secs: Option<u64>,
    /// Legacy: RPC URL applied to commands that accept `--rpc-url`,
    /// regardless of network. Prefer `[rpc_urls]`.
    pub rpc_url: Option<String>,
    /// Legacy: `json = true` selects JSON as the global output format.
    /// Prefer `format = "json"`.
    pub json: Option<bool>,
}

fn deserialize_format<'de, D>(deserializer: D) -> Result<Option<OutputFormat>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    OutputFormat::from_str(&raw, true).map(Some).map_err(|_| {
        let allowed: Vec<&str> = OutputFormat::value_variants()
            .iter()
            .map(|f| f.as_str())
            .collect();
        serde::de::Error::custom(format!(
            "unknown format \"{raw}\", expected one of: {}",
            allowed.join(", ")
        ))
    })
}

impl UserConfig {
    /// Parses a config from TOML text. `origin` is named in error messages.
    pub fn from_toml_str(text: &str, origin: &Path) -> AppResult<Self> {
        toml::from_str(text)
            .map_err(|e| AppError::Config(format!("invalid config file {}: {e}", origin.display())))
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
    /// With `explicit = Some(path)` that file must exist and discovery is
    /// skipped. Otherwise the default locations are searched and an empty
    /// config is returned when none exist.
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
        match discover_config_path() {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default()),
        }
    }

    /// RPC URL configured for `network` under `[rpc_urls]`, if any.
    #[must_use]
    pub fn rpc_url_for(&self, network: &str) -> Option<&str> {
        self.rpc_urls.get(network).map(String::as_str)
    }

    /// Rewrites clap default values so arguments the user did not type pick
    /// up config values: every `--network` that has a built-in default gets
    /// `default_network`, and the global `--timeout` gets `timeout_secs`.
    ///
    /// Explicit arguments are unaffected, so they always win.
    #[must_use]
    pub fn apply_cli_defaults(&self, cmd: clap::Command) -> clap::Command {
        let cmd = match self.timeout_secs {
            Some(secs) if has_arg(&cmd, "timeout") => {
                cmd.mut_arg("timeout", |a| a.default_value(secs.to_string()))
            }
            _ => cmd,
        };
        match &self.default_network {
            Some(network) => with_network_default(cmd, network),
            None => cmd,
        }
    }
}

fn has_arg(cmd: &clap::Command, id: &str) -> bool {
    cmd.get_arguments().any(|a| a.get_id() == id)
}

/// Recursively sets the default of every `network` argument that already has
/// a default. Arguments without a default (e.g. an optional `--network`
/// filter) keep their "not given means all networks" meaning.
fn with_network_default(cmd: clap::Command, network: &str) -> clap::Command {
    let has_default = cmd
        .get_arguments()
        .any(|a| a.get_id() == "network" && !a.get_default_values().is_empty());
    let mut cmd = if has_default {
        cmd.mut_arg("network", |a| a.default_value(network.to_string()))
    } else {
        cmd
    };
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(name, |sub| with_network_default(sub, network));
    }
    cmd
}

/// Default config file locations, highest precedence first, deduplicated.
#[must_use]
pub fn candidate_config_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !paths.contains(&p) {
            paths.push(p);
        }
    };
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        push(PathBuf::from(xdg).join(APP_DIR).join(CONFIG_FILE_NAME));
    }
    if let Some(dir) = dirs::config_dir() {
        push(dir.join(APP_DIR).join(CONFIG_FILE_NAME));
    }
    // No resolvable home directory just means there is no home-dir candidate.
    if let Ok(dir) = crate::paths::data_dir() {
        push(dir.join(CONFIG_FILE_NAME));
    }
    paths
}

/// First existing config file among the default locations.
#[must_use]
pub fn discover_config_path() -> Option<PathBuf> {
    candidate_config_paths().into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, FromArgMatches};

    use super::*;
    use crate::cli::{Cli, Command, ConfigAction};

    fn parse(text: &str) -> UserConfig {
        UserConfig::from_toml_str(text, Path::new("test.toml")).unwrap()
    }

    fn cli_with(cfg: &UserConfig, argv: &[&str]) -> Cli {
        let mut full = vec!["soroban-cost-estimator"];
        full.extend_from_slice(argv);
        let matches = cfg
            .apply_cli_defaults(Cli::command())
            .try_get_matches_from(full)
            .unwrap();
        Cli::from_arg_matches(&matches).unwrap()
    }

    fn list_network(cli: &Cli) -> &str {
        match &cli.command {
            Command::Config {
                action: ConfigAction::List { network },
            } => network,
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn parses_all_keys() {
        let cfg = parse(
            r#"
default_network = "mainnet"
format = "json"
timeout_secs = 45

[rpc_urls]
testnet = "https://t.example"
mainnet = "https://m.example"
"#,
        );
        assert_eq!(cfg.default_network.as_deref(), Some("mainnet"));
        assert_eq!(cfg.format, Some(OutputFormat::Json));
        assert_eq!(cfg.timeout_secs, Some(45));
        assert_eq!(cfg.rpc_url_for("testnet"), Some("https://t.example"));
        assert_eq!(cfg.rpc_url_for("mainnet"), Some("https://m.example"));
        assert_eq!(cfg.rpc_url_for("futurenet"), None);
    }

    #[test]
    fn legacy_keys_still_parse() {
        let cfg = parse("network = \"mainnet\"\nrpc_url = \"http://x\"\njson = true\n");
        assert_eq!(cfg.default_network.as_deref(), Some("mainnet"));
        assert_eq!(cfg.rpc_url.as_deref(), Some("http://x"));
        assert_eq!(cfg.json, Some(true));
    }

    #[test]
    fn empty_file_is_default() {
        assert_eq!(parse(""), UserConfig::default());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert_eq!(parse("some_future_key = 1"), UserConfig::default());
    }

    #[test]
    fn format_is_case_insensitive() {
        assert_eq!(
            parse("format = \"Markdown\"").format,
            Some(OutputFormat::Markdown)
        );
    }

    #[test]
    fn unknown_format_is_config_error_listing_values() {
        let err = UserConfig::from_toml_str("format = \"text\"", Path::new("c.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, AppError::Config(_)));
        assert!(msg.contains("c.toml"), "{msg}");
        assert!(msg.contains("table, json, csv, markdown"), "{msg}");
    }

    #[test]
    fn malformed_toml_is_config_error_naming_file() {
        let err =
            UserConfig::from_toml_str("default_network = ", Path::new("bad.toml")).unwrap_err();
        assert!(matches!(err, AppError::Config(ref m) if m.contains("bad.toml")));
    }

    #[test]
    fn wrong_value_type_is_config_error() {
        let err =
            UserConfig::from_toml_str("timeout_secs = \"soon\"", Path::new("c.toml")).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    #[test]
    fn explicit_missing_path_errors() {
        let err = UserConfig::load(Some(Path::new("/definitely/not/here.toml"))).unwrap_err();
        assert!(matches!(err, AppError::Config(ref m) if m.contains("not found")));
    }

    #[test]
    fn no_config_keeps_builtin_defaults() {
        let cli = cli_with(&UserConfig::default(), &["config", "list"]);
        assert_eq!(list_network(&cli), "testnet");
        assert_eq!(cli.timeout, 30);
        assert_eq!(cli.format, None);
    }

    #[test]
    fn config_supplies_network_and_timeout_defaults() {
        let cfg = parse("default_network = \"mainnet\"\ntimeout_secs = 7");
        let cli = cli_with(&cfg, &["config", "list"]);
        assert_eq!(list_network(&cli), "mainnet");
        assert_eq!(cli.timeout, 7);
    }

    #[test]
    fn explicit_cli_args_beat_config() {
        let cfg = parse("default_network = \"mainnet\"\ntimeout_secs = 7");
        let cli = cli_with(
            &cfg,
            &["config", "list", "--network", "testnet", "--timeout", "30"],
        );
        assert_eq!(list_network(&cli), "testnet");
        assert_eq!(cli.timeout, 30);
    }

    #[test]
    fn network_default_reaches_nested_subcommands() {
        let cfg = parse("default_network = \"mainnet\"");
        let cli = cli_with(&cfg, &["cache", "list"]);
        match cli.command {
            Command::Cache {
                action: crate::cli::CacheAction::List { network, .. },
            } => assert_eq!(network, "mainnet"),
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn optional_network_filters_stay_unset() {
        let cfg = parse("default_network = \"mainnet\"");
        let cli = cli_with(&cfg, &["config", "export", "--output", "b.json"]);
        match cli.command {
            Command::Config {
                action: ConfigAction::Export { network, .. },
            } => assert_eq!(network, None),
            other => panic!("unexpected command {other:?}"),
        }
    }
}
