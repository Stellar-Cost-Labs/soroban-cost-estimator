use std::path::PathBuf;

use tracing::{debug, trace};

use crate::config_snapshot::model::ConfigSnapshot;
use crate::error::{AppError, AppResult};

/// Returns the base data directory: `~/.soroban-cost-estimator`.
fn data_dir() -> AppResult<PathBuf> {
    crate::paths::data_dir()
}

/// Returns the snapshots directory, creating it if needed.
fn snapshots_dir() -> AppResult<PathBuf> {
    let dir = data_dir()?.join("snapshots");
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

/// Loads the two most recent snapshots for a network, oldest first.
///
/// Returns `(previous, latest)` — the pair that `config diff
/// --against-previous` compares. [`list_snapshots`] already orders files
/// oldest → newest, so the last two entries are snapshot N-1 and snapshot N.
///
/// # Errors
/// [`AppError::NotEnoughSnapshots`] when the network has fewer than two
/// snapshots on disk — a diff needs two points in time to be meaningful.
///
/// # Network calls
/// None — pure file I/O.
pub fn load_last_two_snapshots(network: &str) -> AppResult<(ConfigSnapshot, ConfigSnapshot)> {
    debug!(network, "loading the two most recent snapshots");
    let paths = list_snapshots(network)?;
    if paths.len() < 2 {
        return Err(AppError::NotEnoughSnapshots {
            network: network.to_string(),
            found: paths.len(),
        });
    }

    let previous = load_snapshot_from_path(&paths[paths.len() - 2].to_string_lossy())?;
    let latest = load_snapshot_from_path(&paths[paths.len() - 1].to_string_lossy())?;
    trace!(
        previous = paths[paths.len() - 2].display().to_string(),
        latest = paths[paths.len() - 1].display().to_string(),
        "loaded snapshot pair"
    );
    Ok((previous, latest))
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
    list_snapshots_in_dir(&dir, network)
}

/// Deletes snapshots for a network whose files are older than `retain_days`.
///
/// Retention is measured against each snapshot file's **modification time**
/// (`metadata.modified()`), so a snapshot counts as expired once its file has
/// not been touched for `retain_days` days. Returns the number of snapshots
/// deleted.
///
/// # Network calls
/// None — pure file I/O.
pub fn clean_old_snapshots(network: &str, retain_days: u64) -> AppResult<usize> {
    let dir = snapshots_dir()?;
    clean_old_snapshots_in_dir(&dir, network, retain_days)
}

/// Core retention logic over an explicit snapshots directory, so tests can
/// point it at a temporary directory without touching the real data dir.
fn clean_old_snapshots_in_dir(
    dir: &std::path::Path,
    network: &str,
    retain_days: u64,
) -> AppResult<usize> {
    let cutoff =
        chrono::Utc::now() - chrono::Duration::days(i64::try_from(retain_days).unwrap_or(i64::MAX));
    let mut removed = 0;
    for path in list_snapshots_in_dir(dir, network)? {
        let modified = std::fs::metadata(&path)?.modified()?;
        let modified_utc: chrono::DateTime<chrono::Utc> = modified.into();
        if modified_utc < cutoff {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    debug!(network, retain_days, removed, "cleaned old snapshots");
    Ok(removed)
}

/// Core listing logic over an explicit snapshots directory.
fn list_snapshots_in_dir(dir: &std::path::Path, network: &str) -> AppResult<Vec<PathBuf>> {
    let mut snapshots = Vec::new();
    for entry in std::fs::read_dir(dir)? {
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

/// A bundle of config snapshots for export/import.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotBundle {
    pub snapshots: Vec<ConfigSnapshot>,
}

/// Exports snapshots to a single JSON bundle file.
pub fn export_snapshots(network: Option<&str>, output_path: &str) -> AppResult<()> {
    let dir = snapshots_dir()?;
    let mut snapshots = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name_str = entry.file_name().to_string_lossy().into_owned();
        if !name_str.ends_with(".json") {
            continue;
        }
        if let Some(net) = network {
            if !name_str.starts_with(&format!("{}-", net)) {
                continue;
            }
        }
        let content = std::fs::read_to_string(entry.path())?;
        if let Ok(snapshot) = serde_json::from_str::<ConfigSnapshot>(&content) {
            snapshots.push(snapshot);
        }
    }

    let bundle = SnapshotBundle { snapshots };
    let json = serde_json::to_string_pretty(&bundle)
        .map_err(|e| AppError::General(format!("Failed to serialize bundle: {}", e)))?;
    std::fs::write(output_path, json)?;
    Ok(())
}

/// Imports snapshots from a JSON bundle file.
pub fn import_snapshots(bundle_path: &str) -> AppResult<usize> {
    let content = std::fs::read_to_string(bundle_path)?;
    let bundle: SnapshotBundle = serde_json::from_str(&content)
        .map_err(|e| AppError::SnapshotParse(format!("Invalid bundle: {}", e)))?;

    let mut imported = 0;
    for snapshot in bundle.snapshots {
        let ts_safe = snapshot.timestamp.replace(':', "-");
        let filename = format!("{}-{}.json", snapshot.network, ts_safe);
        let path = snapshots_dir()?.join(&filename);
        if !path.exists() {
            let json = serde_json::to_string_pretty(&snapshot)
                .map_err(|e| AppError::General(format!("Failed to serialize snapshot: {}", e)))?;
            std::fs::write(&path, json)?;
            imported += 1;
            println!("  Imported snapshot: {}", filename);
        }
    }
    Ok(imported)
}

/// Summary of one saved snapshot, read from the snapshot file itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SnapshotMetadata {
    pub filename: String,
    pub network: String,
    pub timestamp: String,
    pub ledger_sequence: u32,
    /// `None` for snapshots saved before the protocol version was recorded.
    pub protocol_version: Option<u32>,
}

/// The header fields of a snapshot file. The config sections are not
/// materialised; serde skips them.
#[derive(serde::Deserialize)]
struct SnapshotHeader {
    network: String,
    timestamp: String,
    ledger: u32,
    #[serde(default)]
    protocol_version: Option<u32>,
}

/// Lists metadata for saved snapshots in the snapshots directory.
///
/// `network = None` lists every network. Filtering uses the `network` field
/// stored inside each file, not the filename. Results are sorted by network,
/// then timestamp (oldest first), then filename.
///
/// A missing snapshots directory means no snapshots. As with `config list`,
/// an unreadable or malformed `.json` file fails the listing and names the
/// file; `config validate` reports every bad file at once.
///
/// # Network calls
/// None — pure file I/O.
pub fn list_snapshots_metadata(network: Option<&str>) -> AppResult<Vec<SnapshotMetadata>> {
    let dir = data_dir()?.join("snapshots");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(AppError::General(format!(
                "cannot read snapshot directory {}: {e}",
                dir.display()
            )));
        }
    };

    let mut out = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| {
                AppError::General(format!(
                    "cannot read snapshot directory {}: {e}",
                    dir.display()
                ))
            })?
            .path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| AppError::SnapshotParse(format!("cannot read {}: {e}", path.display())))?;
        let header: SnapshotHeader = serde_json::from_str(&text)
            .map_err(|e| AppError::SnapshotParse(format!("{}: {e}", path.display())))?;
        if network.is_some_and(|n| n != header.network) {
            continue;
        }
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(SnapshotMetadata {
            filename,
            network: header.network,
            timestamp: header.timestamp,
            ledger_sequence: header.ledger,
            protocol_version: header.protocol_version,
        });
    }
    out.sort_by(|a, b| {
        (&a.network, &a.timestamp, &a.filename).cmp(&(&b.network, &b.timestamp, &b.filename))
    });
    trace!(count = out.len(), ?network, "snapshot metadata listed");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates an empty temp directory for one test, mirroring the
    /// `temp_home` helper in the integration suite.
    fn temp_snapshots_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sce-store-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    /// Writes a placeholder snapshot file named like the real store:
    /// `{network}-{timestamp}.json` (with `:` replaced by `-`).
    fn write_snapshot_file(dir: &std::path::Path, network: &str, stamp: &str) {
        let filename = format!("{}-{}.json", network, stamp.replace(':', "-"));
        std::fs::write(dir.join(filename), b"{}").expect("failed to write snapshot");
    }

    /// Rewrites a snapshot file's modification time to `days_ago` days in the
    /// past, simulating an old snapshot.
    fn age_snapshot_file(path: &std::path::Path, days_ago: u64) {
        let old =
            std::time::SystemTime::now() - std::time::Duration::from_secs(days_ago * 24 * 60 * 60);
        filetime::set_file_mtime(path, filetime::FileTime::from_system_time(old))
            .expect("failed to set mtime");
    }

    #[test]
    fn clean_removes_snapshots_older_than_retention() {
        let dir = temp_snapshots_dir("clean-old");
        // Two snapshots aged beyond the 30-day retention window.
        write_snapshot_file(&dir, "testnet", "2026-01-01T00-00-00.000000+00-00");
        age_snapshot_file(
            &dir.join("testnet-2026-01-01T00-00-00.000000+00-00.json"),
            90,
        );
        write_snapshot_file(&dir, "testnet", "2026-01-02T00-00-00.000000+00-00");
        age_snapshot_file(
            &dir.join("testnet-2026-01-02T00-00-00.000000+00-00.json"),
            60,
        );
        // One recent snapshot stays within the window.
        write_snapshot_file(&dir, "testnet", "2026-01-03T00-00-00.000000+00-00");

        let removed =
            clean_old_snapshots_in_dir(&dir, "testnet", 30).expect("clean should succeed");
        assert_eq!(
            removed, 2,
            "should remove the two snapshots older than 30 days"
        );

        let remaining = list_snapshots_in_dir(&dir, "testnet").expect("list should succeed");
        assert_eq!(remaining.len(), 1);
        assert!(
            remaining[0]
                .to_string_lossy()
                .ends_with("2026-01-03T00-00-00.000000+00-00.json")
        );
    }

    #[test]
    fn clean_is_noop_when_all_snapshots_are_recent() {
        let dir = temp_snapshots_dir("clean-noop");
        write_snapshot_file(&dir, "testnet", "2026-01-01T00-00-00.000000+00-00");
        write_snapshot_file(&dir, "testnet", "2026-01-02T00-00-00.000000+00-00");

        let removed =
            clean_old_snapshots_in_dir(&dir, "testnet", 30).expect("clean should succeed");
        assert_eq!(removed, 0, "fresh snapshots should be kept");
        assert_eq!(
            list_snapshots_in_dir(&dir, "testnet").expect("list").len(),
            2
        );
    }

    #[test]
    fn clean_only_touches_the_requested_network() {
        let dir = temp_snapshots_dir("clean-other-network");
        write_snapshot_file(&dir, "testnet", "2026-01-01T00-00-00.000000+00-00");
        age_snapshot_file(
            &dir.join("testnet-2026-01-01T00-00-00.000000+00-00.json"),
            90,
        );
        // A mainnet snapshot with the same age must survive a testnet clean.
        write_snapshot_file(&dir, "mainnet", "2026-01-01T00-00-00.000000+00-00");
        age_snapshot_file(
            &dir.join("mainnet-2026-01-01T00-00-00.000000+00-00.json"),
            90,
        );

        let removed =
            clean_old_snapshots_in_dir(&dir, "testnet", 30).expect("clean should succeed");
        assert_eq!(removed, 1);
        assert_eq!(
            list_snapshots_in_dir(&dir, "testnet").expect("list").len(),
            0
        );
        assert_eq!(
            list_snapshots_in_dir(&dir, "mainnet").expect("list").len(),
            1,
            "mainnet snapshots are untouched by a testnet clean"
        );
    }

    #[test]
    fn clean_with_empty_dir_returns_zero() {
        let dir = temp_snapshots_dir("clean-empty");
        let removed =
            clean_old_snapshots_in_dir(&dir, "testnet", 30).expect("clean should succeed");
        assert_eq!(removed, 0);
    }
}
