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
/// responsible for creating it if needed (or call [`ensure_data_dir`]).
pub fn data_dir() -> AppResult<PathBuf> {
    let home = env_home()
        .or_else(dirs::home_dir)
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    Ok(home.join(".soroban-cost-estimator"))
}

/// Returns the base data directory: `~/.soroban-cost-estimator`, creating it
/// if needed.
///
/// # Platform permissions
///
/// - **Unix-like systems** (Linux, macOS, BSD): If the directory does not exist,
///   it is created with user-only permissions (`0o700` / `rwx------`) so that
///   cache records and other local state are inaccessible to other local users.
///   If the directory already exists, its permissions are left untouched so
///   existing user preferences are preserved.
/// - **Non-Unix platforms** (e.g. Windows): Filesystem permissions are governed
///   by the operating system's default Access Control Lists (ACLs) within the
///   user profile directory.
pub fn ensure_data_dir() -> AppResult<PathBuf> {
    let dir = data_dir()?;
    if !dir.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            use std::os::unix::fs::PermissionsExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            builder.create(&dir)?;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        #[cfg(not(unix))]
        {
            std::fs::create_dir_all(&dir)?;
        }
    }
    Ok(dir)
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
