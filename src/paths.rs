//! Shared on-disk data directory resolution.
//!
//! Both the estimate cache and the config-snapshot store keep their files
//! under `~/.soroban-cost-estimator`. This module centralizes how that
//! directory is resolved.

use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// Returns the base data directory: `~/.soroban-cost-estimator`.
///
/// The home directory comes from `$HOME` (Unix) or `$USERPROFILE`
/// (Windows) when set, falling back to [`dirs::home_dir()`]. Preferring the
/// environment variable keeps the directory overridable — tests point it at
/// a temporary directory, and CI users can relocate the data dir without
/// touching their real profile. On Windows `dirs` resolves the profile via
/// the known-folder API (`SHGetKnownFolderPath`), which ignores `USERPROFILE`,
/// so the environment check must come first there.
///
/// This function only computes the path; callers that write into it are
/// responsible for creating it if needed.
pub fn data_dir() -> AppResult<PathBuf> {
    let home = env_home()
        .or_else(dirs::home_dir)
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    Ok(home.join(".soroban-cost-estimator"))
}

/// The home directory as given by the platform's conventional environment
/// variable, if set and non-empty.
fn env_home() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");

    var.filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Resolves the TOML configuration file path and identifies whether the path
/// came from a CLI `--config` override (`"cli"`) or the platform default
/// (`"default"`).
#[must_use]
pub fn config_file_path_with_source(cli_path: Option<&str>) -> (PathBuf, &'static str) {
    if let Some(path) = cli_path {
        (PathBuf::from(path), "cli")
    } else {
        let default_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("soroban-cost-estimator")
            .join("config.toml");
        (default_path, "default")
    }
}
