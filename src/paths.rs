//! Shared on-disk data directory resolution.
//!
//! Both the estimate cache and the config-snapshot store keep their files
//! under `~/.soroban-cost-estimator`. This module centralizes how that
//! directory is resolved.

use std::path::{Path, PathBuf};

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

/// Ensure a data directory exists, creating it with user-only permissions
/// (`0o700`) on Unix-like systems when it does not yet exist.
///
/// If `dir` already exists, its permissions are preserved unchanged.
/// On non-Unix platforms (such as Windows), directory access is governed by
/// the inherited user-profile ACL.
pub fn ensure_private_dir(dir: &Path) -> AppResult<()> {
    if dir.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Create a new file at `path` with user-only permissions (`0o600` on Unix) if
/// it does not already exist.
///
/// Returns `Ok(true)` when a new file was created, or `Ok(false)` when `path`
/// already existed (preserving its existing permissions untouched).
/// On non-Unix platforms, file permissions are inherited from the parent user
/// profile directory ACL.
pub fn create_private_file_if_missing(path: &Path) -> AppResult<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            ensure_private_dir(parent)?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(_file) => {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    #[cfg(not(unix))]
    {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(_file) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

/// Candidate SQLite journal/WAL sidecar paths for a database file.
#[must_use]
pub fn sqlite_sidecar_paths(db_path: &Path) -> [PathBuf; 3] {
    let base = db_path.as_os_str();
    let mut wal = base.to_os_string();
    wal.push("-wal");
    let mut shm = base.to_os_string();
    shm.push("-shm");
    let mut journal = base.to_os_string();
    journal.push("-journal");
    [
        PathBuf::from(wal),
        PathBuf::from(shm),
        PathBuf::from(journal),
    ]
}

/// Apply user-only permissions (`0o600` on Unix) to any SQLite sidecar files
/// that were newly created during database initialization. Pre-existing
/// sidecars are left untouched.
pub fn restrict_new_sidecar_permissions(sidecars: &[(PathBuf, bool)]) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for (path, existed_before) in sidecars {
            if !*existed_before && path.exists() {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = sidecars;
    }
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
