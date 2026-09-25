use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use tracing::{debug, trace, warn};

use crate::config_snapshot::model::ConfigSnapshot;
use crate::error::{AppError, AppResult};

/// Returns the base data directory: `~/.soroban-cost-estimator`.
fn data_dir() -> AppResult<PathBuf> {
    crate::paths::data_dir()
}

/// Returns the snapshots directory path without touching the filesystem.
fn snapshots_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("snapshots"))
}

/// Returns the snapshots directory, creating it if needed.
fn snapshots_dir() -> AppResult<PathBuf> {
    let dir = snapshots_path()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Saves a config snapshot to disk as a JSON file.
///
/// The filename is `{network}-{timestamp}.json` within the snapshots directory,
/// unless an explicit `--out` path is provided.
///
/// # Network calls
/// None — pure file I/O.
pub fn save_snapshot(snapshot: &ConfigSnapshot, out_path: Option<&str>) -> AppResult<PathBuf> {
    let path = match out_path {
        Some(p) => PathBuf::from(p),
        None => {
            let dir = snapshots_dir()?;
            let filename = format!(
                "{}-{}.json",
                snapshot.network,
                snapshot.timestamp.replace(':', "-")
            );
            dir.join(filename)
        }
    };

    let json = serde_json::to_string_pretty(snapshot)?;
    std::fs::write(&path, json)?;
    debug!(path = %path.display(), network = snapshot.network, ledger = snapshot.ledger, "snapshot saved");
    Ok(path)
}

/// Loads the most recent snapshot for a given network.
///
/// Scans the snapshots directory for files matching `{network}-*.json`
/// and returns the one with the latest timestamp in its filename.
///
/// # Network calls
/// None — pure file I/O.
pub fn load_latest_snapshot(network: &str) -> AppResult<ConfigSnapshot> {
    debug!(network, "loading latest snapshot");
    let dir = snapshots_dir()?;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with(&format!("{}-", network)) && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    let latest = entries
        .into_iter()
        .last()
        .ok_or_else(|| AppError::NoSnapshots(network.to_string()))?;

    let content = std::fs::read_to_string(latest.path())?;
    let snapshot: ConfigSnapshot =
        serde_json::from_str(&content).map_err(|e| AppError::SnapshotParse(e.to_string()))?;
    trace!(network, ledger = snapshot.ledger, "latest snapshot loaded");
    Ok(snapshot)
}

/// Loads a specific snapshot from an explicit path.
///
/// # Network calls
/// None — pure file I/O.
pub fn load_snapshot_from_path(path: &str) -> AppResult<ConfigSnapshot> {
    debug!(path, "loading snapshot from path");
    let content = std::fs::read_to_string(path)?;
    let snapshot: ConfigSnapshot =
        serde_json::from_str(&content).map_err(|e| AppError::SnapshotParse(e.to_string()))?;
    trace!(
        network = snapshot.network,
        ledger = snapshot.ledger,
        "snapshot loaded from path"
    );
    Ok(snapshot)
}

/// Lists all snapshots for a given network.
///
/// # Network calls
/// None — pure file I/O.
pub fn list_snapshots(network: &str) -> AppResult<Vec<PathBuf>> {
    let dir = snapshots_dir()?;
    let mut snapshots = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(&format!("{}-", network)) && name_str.ends_with(".json") {
            snapshots.push(entry.path());
        }
    }

    snapshots.sort();
    Ok(snapshots)
}

/// Loads a specific snapshot by network and timestamp.
///
/// # Network calls
/// None — pure file I/O.
pub fn load_snapshot_by_timestamp(network: &str, timestamp: &str) -> AppResult<ConfigSnapshot> {
    let dir = snapshots_dir()?;
    let ts_safe = timestamp.replace(':', "-");
    let filename = format!("{}-{}.json", network, ts_safe);
    let path = dir.join(&filename);

    if !path.exists() {
        return Err(AppError::General(format!(
            "No snapshot found for network '{}' at timestamp '{}'",
            network, timestamp
        )));
    }

    let content = std::fs::read_to_string(&path)?;
    let snapshot: ConfigSnapshot =
        serde_json::from_str(&content).map_err(|e| AppError::SnapshotParse(e.to_string()))?;
    Ok(snapshot)
}

/// Result of validating a single snapshot file.
#[derive(Debug, Clone)]
pub struct SnapshotValidation {
    pub path: PathBuf,
    pub filename: String,
    pub valid: bool,
    pub error: Option<String>,
}

/// Validates all stored snapshot files for a given network.
///
/// Each file is checked for:
/// - Readable (file exists and is not empty)
/// - Valid JSON (deserializes as `ConfigSnapshot`)
/// - Non-empty network field
/// - Non-zero ledger
///
/// Returns a list of validation results, one per file.
///
/// # Network calls
/// None — pure file I/O.
pub fn validate_all_snapshots(network: &str) -> AppResult<Vec<SnapshotValidation>> {
    let paths = list_snapshots(network)?;
    let mut results = Vec::with_capacity(paths.len());

    for path in paths {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        match validate_single_snapshot(&path) {
            Ok(()) => {
                results.push(SnapshotValidation {
                    path,
                    filename,
                    valid: true,
                    error: None,
                });
            }
            Err(e) => {
                results.push(SnapshotValidation {
                    path,
                    filename,
                    valid: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(results)
}

/// Validates a single snapshot file.
fn validate_single_snapshot(path: &std::path::Path) -> AppResult<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AppError::General(format!("cannot read file: {e}")))?;

    if content.trim().is_empty() {
        return Err(AppError::General("file is empty".to_string()));
    }

    let snapshot: ConfigSnapshot = serde_json::from_str(&content)
        .map_err(|e| AppError::General(format!("invalid JSON: {e}")))?;

    if snapshot.network.is_empty() {
        return Err(AppError::General("network field is empty".to_string()));
    }

    if snapshot.ledger == 0 {
        return Err(AppError::General("ledger is zero".to_string()));
    }

    Ok(())
}

/// Resolves a user-supplied snapshot identifier to a path on disk.
///
/// The identifier is first treated as a path — absolute, or relative to the
/// current directory. If nothing exists there it is retried as a bare
/// filename inside the snapshots directory. Returns
/// [`AppError::SnapshotNotFound`] when neither location holds a file, which
/// is the "snapshot does not exist" contract of `config snapshot delete`.
///
/// # Network calls
/// None — pure file I/O.
pub fn resolve_snapshot_path(identifier: &str) -> AppResult<PathBuf> {
    let direct = PathBuf::from(identifier);
    if direct.is_file() {
        return Ok(direct);
    }

    let in_snapshots_dir = snapshots_path()?.join(identifier);
    if in_snapshots_dir.is_file() {
        return Ok(in_snapshots_dir);
    }

    Err(AppError::SnapshotNotFound(identifier.to_string()))
}

/// Deletes a single snapshot file from disk.
///
/// # Network calls
/// None — pure file I/O.
pub fn delete_snapshot_file(path: &Path) -> AppResult<()> {
    std::fs::remove_file(path)?;
    debug!(path = %path.display(), "snapshot deleted");
    Ok(())
}

/// Lists every stored snapshot file, across all networks.
///
/// # Network calls
/// None — pure file I/O.
pub fn list_all_snapshots() -> AppResult<Vec<PathBuf>> {
    let dir = snapshots_dir()?;
    let mut snapshots = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().ends_with(".json") {
            snapshots.push(entry.path());
        }
    }

    snapshots.sort();
    Ok(snapshots)
}

/// Parses an RFC-3339 timestamp into a UTC instant.
///
/// Returns `None` for formats this tool does not recognize, so callers can
/// skip a file rather than fail an entire purge on one odd snapshot.
pub fn parse_snapshot_timestamp(timestamp: &str) -> Option<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(timestamp.trim());
    parsed.ok().map(|dt| dt.with_timezone(&Utc))
}

/// True when `timestamp` lies more than `days` days before `now`.
///
/// A `days` value large enough to overflow the representable duration
/// matches nothing, so an absurd request is a no-op instead of a panic.
pub fn is_older_than(timestamp: DateTime<Utc>, days: u64, now: DateTime<Utc>) -> bool {
    let Ok(delta) = i64::try_from(days) else {
        return false;
    };
    let Some(window) = TimeDelta::try_days(delta) else {
        return false;
    };
    let Some(cutoff) = now.checked_sub_signed(window) else {
        return false;
    };
    timestamp < cutoff
}

/// Reads the timestamp recorded inside a snapshot file.
///
/// Returns `Ok(None)` when the field is missing or unparseable.
fn snapshot_timestamp_utc(path: &Path) -> AppResult<Option<DateTime<Utc>>> {
    let content = std::fs::read_to_string(path)?;
    let snapshot: ConfigSnapshot =
        serde_json::from_str(&content).map_err(|e| AppError::SnapshotParse(e.to_string()))?;
    Ok(parse_snapshot_timestamp(&snapshot.timestamp))
}

/// Lists snapshot files older than `days` days.
///
/// Age is derived from each file's recorded `timestamp` field; files whose
/// timestamp is missing or unparseable are skipped (and warned about) rather
/// than deleted. When `network` is `Some`, only that network's snapshots are
/// considered.
///
/// # Network calls
/// None — pure file I/O.
pub fn find_snapshots_older_than(days: u64, network: Option<&str>) -> AppResult<Vec<PathBuf>> {
    let paths = match network {
        Some(network) => list_snapshots(network)?,
        None => list_all_snapshots()?,
    };

    let now = Utc::now();
    let mut stale = Vec::new();
    for path in paths {
        match snapshot_timestamp_utc(&path) {
            Ok(Some(timestamp)) if is_older_than(timestamp, days, now) => stale.push(path),
            Ok(_) => {}
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "skipping snapshot whose timestamp could not be read"
                );
            }
        }
    }

    Ok(stale)
}

/// Loads a snapshot from an explicit path, naming the file in any error.
///
/// Unlike [`load_snapshot_from_path`], the error message includes the
/// offending path — `config snapshot diff` compares two files in one run, so
/// the user must know which one could not be read or parsed.
///
/// # Network calls
/// None — pure file I/O.
pub fn load_snapshot_checked(path: &Path) -> AppResult<ConfigSnapshot> {
    let display = path.display().to_string();
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(display.clone())
        } else {
            AppError::General(format!("failed to read snapshot '{display}': {e}"))
        }
    })?;

    let parsed: Result<ConfigSnapshot, serde_json::Error> = serde_json::from_str(&content);
    parsed.map_err(|e| AppError::SnapshotParse(format!("'{display}': {e}")))
}

#[cfg(test)]
mod offline_tests {
    use super::*;

    #[test]
    fn parse_snapshot_timestamp_accepts_rfc3339() {
        let parsed = parse_snapshot_timestamp("2026-01-01T00:00:00+00:00");
        let dt = parsed.expect("valid RFC-3339 timestamp");
        assert_eq!(dt.to_rfc3339(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn parse_snapshot_timestamp_tolerates_whitespace() {
        let parsed = parse_snapshot_timestamp(" 2026-01-01T00:00:00Z ");
        assert!(parsed.is_some());
    }

    #[test]
    fn parse_snapshot_timestamp_rejects_garbage() {
        assert!(parse_snapshot_timestamp("not-a-timestamp").is_none());
        assert!(parse_snapshot_timestamp("").is_none());
    }

    fn fixed_now() -> DateTime<Utc> {
        let parsed = parse_snapshot_timestamp("2026-06-01T00:00:00+00:00");
        parsed.expect("valid fixture timestamp")
    }

    #[test]
    fn is_older_than_compares_against_cutoff() {
        let now = fixed_now();
        let old = now - TimeDelta::days(40);
        let recent = now - TimeDelta::days(10);

        assert!(is_older_than(old, 30, now));
        assert!(!is_older_than(recent, 30, now));
    }

    #[test]
    fn is_older_than_is_false_at_the_boundary() {
        let now = fixed_now();
        let exact = now - TimeDelta::days(30);
        // Strictly older than the cutoff, so exactly N days is kept.
        assert!(!is_older_than(exact, 30, now));
    }

    #[test]
    fn is_older_than_overflowing_window_matches_nothing() {
        let now = fixed_now();
        assert!(!is_older_than(now - TimeDelta::days(10), u64::MAX, now));
    }
}
