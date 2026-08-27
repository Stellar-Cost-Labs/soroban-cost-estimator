//! Estimate result caching.
//!
//! Stores past `estimate` results in `~/.soroban-cost-estimator/cache/`,
//! keyed by `wasm_hash-function_name-args_hash.json`. The `config diff`
//! command cross-references cached estimates to tell the user which ones
//! are now stale due to network pricing changes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::error::{AppError, AppResult};

/// Current cache-entry schema version.
///
/// Bump this whenever the on-disk `CachedEstimate` JSON shape changes.
/// Entries written by a version of the tool with an older schema are
/// migrated forward through [`migrate_to_latest`]; entries written by a
/// *newer* tool (version greater than this) are rejected rather than
/// silently misread.
pub const CACHE_SCHEMA_VERSION: u32 = 1;

/// Implicit schema version of cache entries written before the `version`
/// field existed.
///
/// Those legacy entries have no `version` key in their JSON, so serde's
/// `default` fills in this value via [`default_schema_version`]. They are
/// the first schema version and require no transformation to reach the
/// current schema.
pub const INITIAL_SCHEMA_VERSION: u32 = 1;

/// serde default for the `version` field, applied when an older (or hand
/// written) entry omits it. Legacy entries predating the version field are
/// treated as [`INITIAL_SCHEMA_VERSION`].
fn default_schema_version() -> u32 {
    INITIAL_SCHEMA_VERSION
}

/// A cached estimate result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEstimate {
    /// Schema version of this entry. Legacy entries default to
    /// [`INITIAL_SCHEMA_VERSION`] when the field is absent.
    #[serde(default = "default_schema_version")]
    pub version: u32,
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

/// Returns the cache directory path, creating it if needed.
fn cache_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    let dir = home.join(".soroban-cost-estimator").join("cache");
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
        version: CACHE_SCHEMA_VERSION,
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

/// Carry a cached estimate forward to the current schema version.
///
/// * `version < CACHE_SCHEMA_VERSION`: entries from older schemas are
///   migrated one step at a time toward the current schema. Currently the
///   initial and current schemas are identical, so this is the identity
///   transform; adding a schema change later means appending a migration
///   step here.
/// * `version == CACHE_SCHEMA_VERSION`: returned unchanged.
/// * `version > CACHE_SCHEMA_VERSION`: an entry written by a *newer* tool.
///   It cannot be safely read (or silently downgraded), so this returns an
///   error instead of misinterpreting fields.
///
/// # Network calls
/// None — pure transformation.
pub fn migrate_to_latest(cached: CachedEstimate) -> AppResult<CachedEstimate> {
    let mut migrated = cached;

    match migrated.version {
        v if v > CACHE_SCHEMA_VERSION => Err(AppError::General(format!(
            "cache entry schema v{v} is newer than supported v{CACHE_SCHEMA_VERSION}"
        ))),
        // Nothing below the current schema exists yet; future schema changes
        // add per-step migrations here, e.g. v1 -> v2.
        v if v < CACHE_SCHEMA_VERSION => {
            migrated.version = CACHE_SCHEMA_VERSION;
            Ok(migrated)
        }
        _ => Ok(migrated),
    }
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
    let cached = migrate_to_latest(cached)?;
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
    /// Schema version parsed from the entry, if it deserialized at all.
    pub version: Option<u32>,
    /// Whether the file parsed as a valid, readable `CachedEstimate`.
    /// Entries carrying a schema newer than the current one are not valid.
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

            // A file counts as valid when it both parses as a
            // `CachedEstimate` and carries a schema this tool can read.
            // Entries from the future (version > current) parse fine but are
            // not migratable to the current schema, so they are flagged.
            let (valid, version) = match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<CachedEstimate>(&content) {
                    Ok(parsed) => {
                        let version = Some(parsed.version);
                        let valid = migrate_to_latest(parsed).is_ok();
                        (valid, version)
                    }
                    Err(_) => (false, None),
                },
                Err(_) => (false, None),
            };

            if !valid {
                warn!(filename, "corrupt or unsupported cache entry");
            }
            statuses.push(CacheEntryStatus {
                filename,
                version,
                valid,
            });
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
