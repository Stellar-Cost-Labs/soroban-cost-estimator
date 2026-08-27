//! Estimate result caching.
//!
//! Stores past `estimate` results in `~/.soroban-cost-estimator/cache/`,
//! keyed by `wasm_hash-function_name-args_hash.json`. The `config diff`
//! command cross-references cached estimates to tell the user which ones
//! are now stale due to network pricing changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::error::{AppError, AppResult};

/// A cached estimate result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEstimate {
    /// SHA-256 hash of the WASM bytes (hex).
    pub wasm_hash: String,
    /// Contract function name (e.g. `"(wasm upload)"`).
    pub function: String,
    /// SHA-256 hash of the args JSON (hex).
    pub args_hash: String,
    /// Network the simulation ran against.
    pub network: String,
    /// Ledger sequence at the time of simulation.
    pub ledger: u32,
    /// Total fee in stroops.
    pub total_stroops: i64,
    /// CPU instructions consumed.
    pub cpu_instructions: u64,
    /// Memory bytes consumed.
    pub memory_bytes: u64,
    /// ISO-8601 timestamp of when the estimate was made.
    pub timestamp: String,
}

/// Returns the base data directory path: `~/.soroban-cost-estimator`,
/// creating it if needed.
fn data_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the cache directory path, creating it if needed.
fn cache_dir() -> AppResult<PathBuf> {
    let dir = data_dir()?.join("cache");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build a filename for a cached estimate.
fn cache_filename(wasm_hash: &str, function: &str, args_hash: &str) -> String {
    format!("{wasm_hash}-{function}-{args_hash}.json")
}

/// Save an estimate result to the cache.
///
/// # Arguments
/// * `wasm_hash` - SHA-256 hex of the WASM bytes.
/// * `function` - Function name (e.g. `"my_func"` or `"(wasm upload)"`).
/// * `args` - Raw `--arg` values (joined and hashed to form the key).
/// * `network` - Network name.
/// * `ledger` - Ledger sequence at simulation time.
/// * `total_stroops` - Total resource fee in stroops.
/// * `cpu_instructions` - CPU instructions consumed.
/// * `memory_bytes` - Memory bytes consumed.
///
/// # Network calls
/// None — pure file I/O.
pub fn save_estimate(
    wasm_hash: &str,
    function: &str,
    args: &[String],
    network: &str,
    ledger: u32,
    total_stroops: i64,
    cpu_instructions: u64,
    memory_bytes: u64,
) -> AppResult<()> {
    let args_hash = hash_args(args);
    let dir = cache_dir()?;
    let filename = cache_filename(wasm_hash, function, &args_hash);
    let path = dir.join(&filename);

    let cached = CachedEstimate {
        wasm_hash: wasm_hash.to_string(),
        function: function.to_string(),
        args_hash,
        network: network.to_string(),
        ledger,
        total_stroops,
        cpu_instructions,
        memory_bytes,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let json = serde_json::to_string_pretty(&cached)?;
    std::fs::write(&path, json)?;
    debug!(path = %path.display(), function, network, ledger, "estimate cached");
    Ok(())
}

/// Compute a hash of the args for use as a cache key.
fn hash_args(args: &[String]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for arg in args {
        hasher.update(arg.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Load a cached estimate, if one exists.
pub fn load_estimate(
    wasm_hash: &str,
    function: &str,
    args: &[String],
) -> AppResult<Option<CachedEstimate>> {
    let args_hash = hash_args(args);
    let dir = cache_dir()?;
    let filename = cache_filename(wasm_hash, function, &args_hash);
    let path = dir.join(&filename);

    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let cached: CachedEstimate =
        serde_json::from_str(&content).map_err(|e| AppError::SnapshotParse(e.to_string()))?;
    Ok(Some(cached))
}

/// Find all cached estimates for a given network.
///
/// Used by `config diff` to check which cached estimates are now stale
/// after a pricing change.
pub fn list_cached_estimates(network: &str) -> AppResult<Vec<CachedEstimate>> {
    let dir = cache_dir()?;
    let mut estimates = Vec::new();

    if !dir.exists() {
        return Ok(estimates);
    }

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cached) = serde_json::from_str::<CachedEstimate>(&content) {
                    if cached.network == network {
                        estimates.push(cached);
                    }
                }
            }
        }
    }

    trace!(network, count = estimates.len(), "listed cached estimates");
    Ok(estimates)
}

/// Integrity status of a single cache entry file.
#[derive(Debug, Clone)]
pub struct CacheEntryStatus {
    /// File name of the cache entry (e.g. `"abc123-my_func-def456.json"`).
    pub filename: String,
    /// Whether the file parsed as a valid `CachedEstimate`.
    pub valid: bool,
}

/// Verify the integrity of every entry in the estimate cache.
///
/// Reads each `.json` file in the cache directory and checks that it parses
/// as a valid [`CachedEstimate`]. Returns one status per entry, sorted by
/// filename. Files that are unreadable, invalid JSON, or missing required
/// fields are reported as not valid.
///
/// # Network calls
/// None — pure file I/O.
pub fn verify_cache() -> AppResult<Vec<CacheEntryStatus>> {
    let dir = cache_dir()?;
    let mut statuses = Vec::new();

    if !dir.exists() {
        return Ok(statuses);
    }

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let valid = std::fs::read_to_string(&path)
                .ok()
                .map(|content| serde_json::from_str::<CachedEstimate>(&content).is_ok())
                .unwrap_or(false);
            if !valid {
                warn!(filename, "corrupt cache entry");
            }
            statuses.push(CacheEntryStatus { filename, valid });
        }
    }

    statuses.sort_by(|a, b| a.filename.cmp(&b.filename));
    debug!(total = statuses.len(), "cache verification complete");
    Ok(statuses)
}

/// Check which cached estimates are now stale (simulated at an earlier ledger).
///
/// Returns a list of cached estimates that were made before `current_ledger`.
pub fn find_stale_estimates(
    estimates: &[CachedEstimate],
    current_ledger: u32,
) -> Vec<&CachedEstimate> {
    estimates
        .iter()
        .filter(|e| e.ledger < current_ledger)
        .collect()
}

/// Last-observed identity of a WASM file: its SHA-256 hash and modification
/// time. Used to detect when a contract was recompiled or replaced so the
/// stale cache entries from the previous build can be dropped automatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WasmFileRecord {
    /// SHA-256 hash of the WASM bytes (hex), as last observed.
    wasm_hash: String,
    /// File mtime, in nanoseconds since the Unix epoch.
    mtime_nanos: u64,
}

/// Registry mapping a canonical WASM file path to its last-observed identity.
type WasmRegistry = HashMap<String, WasmFileRecord>;

/// Path to the on-disk registry of WASM file identities.
///
/// Lives in the data directory (not the cache directory) so that the cache
/// directory stays a flat list of `CachedEstimate` JSON files.
fn registry_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("wasm-files.json"))
}

/// Load the WASM file registry, or an empty one if it does not exist yet.
fn load_registry() -> AppResult<WasmRegistry> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&path)?;
    // A malformed registry (e.g. hand-edited) degrades to empty rather than
    // failing the command; the next invalidation pass will rebuild it.
    let registry: WasmRegistry = serde_json::from_str(&content).unwrap_or_default();
    Ok(registry)
}

/// Persist the WASM file registry to disk.
fn save_registry(registry: &WasmRegistry) -> AppResult<()> {
    let path = registry_path()?;
    let json = serde_json::to_string_pretty(registry)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Remove every cached estimate produced from the given WASM hash.
///
/// Returns the number of cache files removed. Used by
/// [`invalidate_if_wasm_changed`] to drop entries from a previous build once
/// the WASM file has changed.
///
/// # Network calls
/// None — pure file I/O.
pub fn remove_cached_estimates_for_wasm(wasm_hash: &str) -> AppResult<usize> {
    let dir = cache_dir()?;
    let mut removed = 0;

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cached) = serde_json::from_str::<CachedEstimate>(&content) {
                    if cached.wasm_hash == wasm_hash {
                        std::fs::remove_file(&path)?;
                        removed += 1;
                    }
                }
            }
        }
    }

    Ok(removed)
}

/// Invalidate cache entries when a WASM file's mtime or hash has changed.
///
/// Called before estimates are saved for a freshly loaded WASM file. It
/// compares the file's current hash and modification time against the last
/// observed values in the registry; if either differs (the contract was
/// recompiled or replaced), every cache entry keyed to the previous hash is
/// removed so the new build's estimates start clean.
///
/// Returns `true` when stale entries were removed, `false` otherwise.
///
/// # Network calls
/// None — pure file I/O.
pub fn invalidate_if_wasm_changed(wasm_path: &Path, current_hash: &str) -> AppResult<bool> {
    let mtime_nanos = wasm_file_mtime_nanos(wasm_path)?;
    let key = std::fs::canonicalize(wasm_path)
        .unwrap_or_else(|_| wasm_path.to_path_buf())
        .to_string_lossy()
        .to_string();

    let mut registry = load_registry()?;
    let changed = match registry.get(&key) {
        Some(prev) if prev.wasm_hash != current_hash || prev.mtime_nanos != mtime_nanos => {
            remove_cached_estimates_for_wasm(&prev.wasm_hash)?;
            true
        }
        _ => false,
    };

    registry.insert(
        key,
        WasmFileRecord {
            wasm_hash: current_hash.to_string(),
            mtime_nanos,
        },
    );
    save_registry(&registry)?;

    Ok(changed)
}

/// Read a file's modification time as nanoseconds since the Unix epoch.
///
/// Falls back to `0` when the platform cannot report a modification time,
/// rather than failing the whole command.
fn wasm_file_mtime_nanos(wasm_path: &Path) -> AppResult<u64> {
    let metadata = std::fs::metadata(wasm_path)?;
    let modified = metadata.modified()?;
    let nanos = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Ok(nanos)
}
