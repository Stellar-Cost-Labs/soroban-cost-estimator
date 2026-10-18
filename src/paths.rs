//! Shared on-disk data directory resolution and permission policy.
//!
//! Both the estimate cache and the config-snapshot store keep their files
//! under `~/.soroban-cost-estimator`. This module centralizes how that
//! directory is resolved and how private file permissions are applied.
//!
//! # Data-Directory Permission Policy
//! - **Unix-like systems (`cfg(unix)`)**:
//!   - Managed directories (`~/.soroban-cost-estimator` and `snapshots/`) are
//!     created with mode `0o700` (user read/write/execute only).
//!   - Newly created snapshot files are opened with mode `0o600` (user
//!     read/write only) *before* any payload bytes are written, and symlinks at
//!     the target path are rejected rather than followed.
//!   - When a caller supplies an explicit output path (`--out`) that already
//!     exists on disk, its existing mode bits are preserved rather than
//!     overwritten unexpectedly.
//! - **Windows and other non-Unix platforms (`cfg(not(unix))`)**:
//!   - Files and directories inherit the user profile's standard ACLs from the
//!     parent directory (`%USERPROFILE%\.soroban-cost-estimator`).

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Default Unix permission mode (`0o700`) for managed data directories.
pub const DATA_DIR_MODE_UNIX: u32 = 0o700;

/// Default Unix permission mode (`0o600`) for newly created snapshot files.
pub const SNAPSHOT_FILE_MODE_UNIX: u32 = 0o600;

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

/// Creates a managed directory (and parents) with user-only permissions (`0o700`) on Unix.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(DATA_DIR_MODE_UNIX);
        builder.create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Writes `contents` to `path`, applying `0o600` permissions on Unix before
/// writing data.
///
/// - Rejects existing symlinks at `path` so snapshot writes never follow a
///   symlink to an unintended target.
/// - When `preserve_existing_caller_permissions` is `true` and `path` already
///   exists as a regular file, its existing permissions are left unchanged.
///   When `false` (managed store files) or when creating a new file, `0o600`
///   is enforced on Unix before any data is written.
pub fn write_snapshot_bytes(
    path: &Path,
    contents: &[u8],
    preserve_existing_caller_permissions: bool,
) -> std::io::Result<()> {
    let existed = match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "refusing to write snapshot through symlink '{}'",
                        path.display()
                    ),
                ));
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(SNAPSHOT_FILE_MODE_UNIX);
    }

    let mut file = options.open(path)?;

    #[cfg(unix)]
    if !existed || !preserve_existing_caller_permissions {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(SNAPSHOT_FILE_MODE_UNIX))?;
    }

    #[cfg(not(unix))]
    let _ = (existed, preserve_existing_caller_permissions);

    file.write_all(contents)?;
    file.flush()
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
