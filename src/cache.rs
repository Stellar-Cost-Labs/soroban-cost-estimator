//! Estimate result caching.
//!
//! Stores past `estimate` results in `~/.soroban-cost-estimator/cache/`,
//! keyed by `wasm_hash-function_name-args_hash.json`. The `config diff`
//! command cross-references cached estimates to tell the user which ones
//! are now stale due to network pricing changes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Schema version written into (and required by) cache export files.
///
/// Bump this only for breaking changes to the export format; older files
/// with a different version are rejected with an informative error.
pub const CACHE_EXPORT_SCHEMA_VERSION: u32 = 1;

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

/// A versioned export file of cached estimates, shared between team members
/// or CI runners via `config cache import`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheExport {
    /// Export file schema version (must equal [`CACHE_EXPORT_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// ISO-8601 timestamp of when the export was created.
    pub exported_at: String,
    /// The exported cache entries.
    pub entries: Vec<CachedEstimate>,
}

/// Outcome summary of a `config cache import` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    /// Entries written to the cache (newly created or replaced).
    pub imported: usize,
    /// Entries left untouched because an existing entry was kept.
    pub skipped: usize,
}

/// Returns the cache directory path, creating it if needed.
fn cache_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    let dir = home.join(".soroban-cost-estimator").join("cache");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build a filename for a cached estimate.
///
/// The same key format is used by save, load, and import so an exported
/// entry always lands on its original cache path.
pub fn cache_filename(wasm_hash: &str, function: &str, args_hash: &str) -> String {
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

    Ok(estimates)
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

/// Import cached estimates from an export JSON file into the local cache.
///
/// Reads the export file, validates its schema version, and writes each
/// entry into the cache directory under its normal cache key. In merge mode
/// (`overwrite == false`) an existing entry is only replaced when the
/// imported entry carries a strictly newer timestamp; in overwrite mode
/// (`overwrite == true`) existing entries are always replaced. A local
/// entry that exists but cannot be parsed is treated as absent and
/// replaced.
///
/// # Arguments
/// * `file` - Path to the cache export JSON file.
/// * `overwrite` - `true` to replace existing entries unconditionally;
///   `false` to merge, keeping the newer of each pair.
///
/// # Network calls
/// None — pure file I/O.
pub fn import_cache(file: &Path, overwrite: bool) -> AppResult<ImportSummary> {
    let content = std::fs::read_to_string(file).map_err(|e| {
        AppError::CacheImport(format!(
            "could not read export file {}: {e}",
            file.display()
        ))
    })?;

    let export: CacheExport = serde_json::from_str(&content).map_err(|e| {
        AppError::CacheImport(format!(
            "corrupted cache export file {}: {e}",
            file.display()
        ))
    })?;

    if export.schema_version != CACHE_EXPORT_SCHEMA_VERSION {
        return Err(AppError::CacheImport(format!(
            "unsupported schema version {} in {} (expected {CACHE_EXPORT_SCHEMA_VERSION})",
            export.schema_version,
            file.display()
        )));
    }

    let dir = cache_dir()?;
    let mut imported = 0usize;
    let mut skipped = 0usize;

    for entry in &export.entries {
        let path = dir.join(cache_filename(
            &entry.wasm_hash,
            &entry.function,
            &entry.args_hash,
        ));

        let existing: Option<CachedEstimate> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok());

        let should_write = match (&existing, overwrite) {
            (None, _) | (Some(_), true) => true,
            (Some(existing), false) => timestamp_is_newer(&entry.timestamp, &existing.timestamp),
        };

        if should_write {
            let json = serde_json::to_string_pretty(entry)?;
            std::fs::write(&path, json)?;
            imported += 1;
        } else {
            skipped += 1;
        }
    }

    Ok(ImportSummary { imported, skipped })
}

/// Compare two RFC-3339 timestamps: is `incoming` strictly newer than
/// `existing`?
///
/// Falls back to lexicographic comparison when either string cannot be
/// parsed (RFC-3339 strings with the same format sort correctly as text).
fn timestamp_is_newer(incoming: &str, existing: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(incoming),
        chrono::DateTime::parse_from_rfc3339(existing),
    ) {
        (Ok(a), Ok(b)) => a > b,
        _ => incoming > existing,
    }
}
