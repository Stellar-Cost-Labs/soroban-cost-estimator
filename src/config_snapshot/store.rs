use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
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

/// Lists all snapshots for a given network, oldest → newest.
///
/// # Network calls
/// None — pure file I/O.
pub fn list_snapshots(network: &str) -> AppResult<Vec<PathBuf>> {
    list_snapshots_in(&snapshots_dir()?, network)
}

/// Lists the `{network}-*.json` files in `dir`, sorted oldest → newest.
///
/// Filenames are `{network}-{timestamp}.json` where the timestamp is an
/// RFC 3339 UTC instant with `:` replaced by `-`, so a plain lexicographic
/// sort of the paths is also a chronological one.
fn list_snapshots_in(dir: &Path, network: &str) -> AppResult<Vec<PathBuf>> {
    let prefix = format!("{network}-");
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
        if name_str.starts_with(&prefix) && name_str.ends_with(".json") {
            snapshots.push(entry.path());
        }
    }

    snapshots.sort();
    Ok(snapshots)
}

/// Applies a retention policy to the snapshots directory for `network` and
/// deletes the snapshots it no longer wants.
///
/// `retain_count` keeps only the newest `N` snapshots; `retain_days` drops
/// snapshots recorded more than `D` days ago. When both are supplied a
/// snapshot is deleted if **either** rule rejects it. Supplying neither is a
/// no-op, so a caller can forward an optional `--retain` value unconditionally.
///
/// The newest snapshot is never deleted: not by `--retain 0`, and not by an
/// age rule however stale the file is. A retention run must never leave a
/// network with nothing left to diff against.
///
/// Returns the deleted paths, oldest first.
///
/// # Errors
/// Fails if a snapshot that needs an age check cannot be read or records an
/// unparseable timestamp. The policy is decided in full before anything is
/// removed, so a failed run leaves the directory untouched.
///
/// # Network calls
/// None — pure file I/O.
pub fn prune_snapshots(
    network: &str,
    retain_count: Option<usize>,
    retain_days: Option<u32>,
) -> AppResult<Vec<PathBuf>> {
    let dir = snapshots_dir()?;
    prune_snapshots_in(&dir, network, retain_count, retain_days, Utc::now())
}

/// [`prune_snapshots`] against an explicit directory and clock.
///
/// Split out so the policy — count, age, both, and the "never delete the
/// latest" invariant — can be tested against a temporary directory without
/// touching the user's real data directory or depending on the wall clock.
fn prune_snapshots_in(
    dir: &Path,
    network: &str,
    retain_count: Option<usize>,
    retain_days: Option<u32>,
    now: DateTime<Utc>,
) -> AppResult<Vec<PathBuf>> {
    let paths = list_snapshots_in(dir, network)?;
    if paths.len() < 2 || (retain_count.is_none() && retain_days.is_none()) {
        // A lone snapshot is by definition the latest one, and no explicit
        // policy means no policy — either way there is nothing safe to remove.
        return Ok(Vec::new());
    }

    let newest = paths.len() - 1;
    // `--retain N` keeps the newest N. Treating N as at least 1 is what makes
    // the "never delete the latest snapshot" invariant hold for `--retain 0`.
    let count_cutoff = retain_count.map(|count| paths.len().saturating_sub(count.max(1)));

    // Pass 1: decide. Age rules read snapshot files, so a corrupt one must
    // fail the run *before* a single file is removed.
    let mut doomed = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if index == newest {
            continue;
        }
        let over_count = count_cutoff.is_some_and(|cutoff| index < cutoff);
        let too_old = match retain_days {
            Some(days) => snapshot_age_days(path, now)? > i64::from(days),
            None => false,
        };
        if over_count || too_old {
            doomed.push(path.clone());
        }
    }

    // Pass 2: act. `paths` is sorted oldest → newest, so `doomed` is too.
    for path in &doomed {
        std::fs::remove_file(path)?;
    }
    debug!(
        network,
        pruned = doomed.len(),
        remaining = paths.len() - doomed.len(),
        "snapshot retention applied"
    );
    Ok(doomed)
}

/// Whole days between a snapshot's recorded timestamp and `now`.
///
/// # Network calls
/// None — reads one snapshot file.
fn snapshot_age_days(path: &Path, now: DateTime<Utc>) -> AppResult<i64> {
    let snapshot = load_snapshot_from_path(&path.to_string_lossy())?;
    let recorded = DateTime::parse_from_rfc3339(&snapshot.timestamp).map_err(|e| {
        AppError::SnapshotParse(format!(
            "{} records an unparseable timestamp '{}': {e}",
            path.display(),
            snapshot.timestamp
        ))
    })?;
    Ok(now
        .signed_duration_since(recorded.with_timezone(&Utc))
        .num_days())
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use chrono::Duration;

    use super::*;

    /// Counter that keeps each test's temporary directory unique without
    /// pulling in another dev-dependency.
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Creates a unique empty directory for one test.
    fn temp_dir(label: &str) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("sce-store-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Minimal but structurally valid snapshot JSON.
    fn snapshot_json(network: &str, ledger: u32, timestamp: &str) -> String {
        serde_json::json!({
            "network": network,
            "ledger": ledger,
            "timestamp": timestamp,
            "contract_compute": null,
            "contract_ledger_cost": null,
            "contract_historical_data": null,
            "contract_events": null,
            "contract_bandwidth": null,
            "state_archival": null,
        })
        .to_string()
    }

    /// Writes one snapshot, named exactly as `save_snapshot` names it.
    fn write_snapshot(dir: &Path, network: &str, at: DateTime<Utc>, ledger: u32) -> PathBuf {
        let timestamp = at.to_rfc3339();
        let path = dir.join(format!("{network}-{}.json", timestamp.replace(':', "-")));
        std::fs::write(&path, snapshot_json(network, ledger, &timestamp)).expect("write snapshot");
        path
    }

    fn count_snapshots(dir: &Path, network: &str) -> usize {
        list_snapshots_in(dir, network)
            .expect("list snapshots")
            .len()
    }

    #[test]
    fn test_prune_without_a_policy_is_a_noop() {
        let dir = temp_dir("no-policy");
        let now = Utc::now();
        for i in 0..3 {
            write_snapshot(&dir, "testnet", now - Duration::days(i), i as u32);
        }

        let deleted = prune_snapshots_in(&dir, "testnet", None, None, now).expect("prune");
        assert!(deleted.is_empty(), "no policy means nothing is pruned");
        assert_eq!(count_snapshots(&dir, "testnet"), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_by_count_keeps_newest_n() {
        let dir = temp_dir("retain-count");
        let now = Utc::now();
        for i in 0..5 {
            write_snapshot(&dir, "testnet", now - Duration::days(4 - i), i as u32);
        }
        let all = list_snapshots_in(&dir, "testnet").expect("list");

        let deleted = prune_snapshots_in(&dir, "testnet", Some(2), None, now).expect("prune");

        assert_eq!(deleted, all[..3].to_vec(), "the three oldest are pruned");
        assert_eq!(
            list_snapshots_in(&dir, "testnet").expect("list"),
            all[3..].to_vec(),
            "the two newest survive, oldest first"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_retain_zero_still_keeps_latest() {
        let dir = temp_dir("retain-zero");
        let now = Utc::now();
        for i in 0..3 {
            write_snapshot(&dir, "testnet", now - Duration::days(2 - i), i as u32);
        }

        let deleted = prune_snapshots_in(&dir, "testnet", Some(0), None, now).expect("prune");

        assert_eq!(deleted.len(), 2, "--retain 0 is treated as --retain 1");
        assert_eq!(count_snapshots(&dir, "testnet"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_by_age_deletes_only_old_snapshots() {
        let dir = temp_dir("retain-days");
        let now = Utc::now();
        let stale = write_snapshot(&dir, "testnet", now - Duration::days(40), 1);
        let recent = write_snapshot(&dir, "testnet", now - Duration::days(10), 2);
        let newest = write_snapshot(&dir, "testnet", now - Duration::days(1), 3);

        let deleted = prune_snapshots_in(&dir, "testnet", None, Some(30), now).expect("prune");

        assert_eq!(
            deleted,
            vec![stale],
            "only the 40-day-old snapshot is stale"
        );
        let remaining = list_snapshots_in(&dir, "testnet").expect("list");
        assert!(
            remaining.contains(&recent),
            "a 10-day-old snapshot is recent"
        );
        assert!(remaining.contains(&newest), "the newest snapshot is recent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_by_age_never_deletes_the_latest() {
        // Every snapshot is older than the threshold, but the newest one is
        // the only thing left to diff against, so it has to survive.
        let dir = temp_dir("retain-age-latest");
        let now = Utc::now();
        for i in 0..3 {
            write_snapshot(&dir, "testnet", now - Duration::days(200 - i), i as u32);
        }

        let deleted = prune_snapshots_in(&dir, "testnet", None, Some(1), now).expect("prune");

        assert_eq!(deleted.len(), 2, "all but the newest are older than a day");
        assert_eq!(count_snapshots(&dir, "testnet"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_count_and_days_combine_as_a_union() {
        let dir = temp_dir("retain-union");
        let now = Utc::now();
        // Oldest to newest.
        write_snapshot(&dir, "testnet", now - Duration::days(50), 1); // over count and age
        write_snapshot(&dir, "testnet", now - Duration::days(2), 2); // over count only
        write_snapshot(&dir, "testnet", now - Duration::days(1), 3); // kept
        write_snapshot(&dir, "testnet", now, 4); // newest, protected

        let deleted = prune_snapshots_in(&dir, "testnet", Some(2), Some(30), now).expect("prune");

        assert_eq!(deleted.len(), 2, "either rule rejecting a file prunes it");
        assert_eq!(count_snapshots(&dir, "testnet"), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_single_snapshot_is_a_noop() {
        let dir = temp_dir("retain-single");
        let now = Utc::now();
        write_snapshot(&dir, "testnet", now - Duration::days(365), 1);

        let deleted = prune_snapshots_in(&dir, "testnet", Some(1), Some(1), now).expect("prune");

        assert!(deleted.is_empty(), "the only snapshot is also the latest");
        assert_eq!(count_snapshots(&dir, "testnet"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_empty_directory_is_a_noop() {
        let dir = temp_dir("retain-empty");
        let deleted =
            prune_snapshots_in(&dir, "testnet", Some(1), Some(1), Utc::now()).expect("prune");
        assert!(deleted.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_leaves_other_networks_alone() {
        let dir = temp_dir("retain-network");
        let now = Utc::now();
        write_snapshot(&dir, "testnet", now - Duration::days(90), 1);
        write_snapshot(&dir, "testnet", now, 2);
        let other = write_snapshot(&dir, "mainnet", now - Duration::days(90), 9);

        let deleted = prune_snapshots_in(&dir, "testnet", None, Some(30), now).expect("prune");

        assert_eq!(deleted.len(), 1);
        assert!(other.exists(), "another network's snapshots are untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_prune_by_age_fails_before_deleting_on_a_corrupt_timestamp() {
        let dir = temp_dir("retain-bad-timestamp");
        let now = Utc::now();
        let corrupt = dir.join("testnet-2026-01-01T00-00-01+00-00.json");
        std::fs::write(&corrupt, snapshot_json("testnet", 1, "not-a-timestamp"))
            .expect("write snapshot");
        write_snapshot(&dir, "testnet", now, 2);

        let err = prune_snapshots_in(&dir, "testnet", None, Some(1), now)
            .expect_err("an unparseable timestamp fails the run");

        assert!(matches!(err, AppError::SnapshotParse(_)), "got: {err}");
        assert!(corrupt.exists(), "a failed run deletes nothing");
        assert_eq!(count_snapshots(&dir, "testnet"), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
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
