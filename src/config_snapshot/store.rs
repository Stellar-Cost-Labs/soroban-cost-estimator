use std::path::PathBuf;

use tracing::{debug, trace};

use crate::config_snapshot::model::ConfigSnapshot;
use crate::error::{AppError, AppResult};

/// Returns the base data directory: `~/.soroban-cost-estimator`.
fn data_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    Ok(home.join(".soroban-cost-estimator"))
}

/// Returns the snapshots directory, creating it if needed.
fn snapshots_dir() -> AppResult<PathBuf> {
    let dir = data_dir()?.join("snapshots");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the cache directory, creating it if needed.
pub fn cache_dir() -> AppResult<PathBuf> {
    let dir = data_dir()?.join("cache");
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

/// Lists all available snapshots for a given network.
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

/// Deletes snapshots older than `days` for the specified network.
///
/// # Network calls
/// None — pure file I/O.
pub fn clean_old_snapshots(network: &str, days: u64) -> AppResult<usize> {
    let dir = snapshots_dir()?;
    let mut deleted = 0;

    // Calculate the cutoff date
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);

    if !dir.exists() {
        return Ok(0);
    }

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with(&format!("{}-", network)) && name_str.ends_with(".json") {
            let metadata = entry.metadata()?;
            if let Ok(modified) = metadata.modified() {
                let modified_dt = chrono::DateTime::<chrono::Utc>::from(modified);

                if modified_dt < cutoff && std::fs::remove_file(entry.path()).is_ok() {
                    deleted += 1;
                }
            }
        }
    }

    Ok(deleted)
}
