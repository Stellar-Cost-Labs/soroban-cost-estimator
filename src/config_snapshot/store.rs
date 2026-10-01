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
