//! Estimate result caching (SQLite-backed).
//!
//! Stores past `estimate` results in a single SQLite database at
//! `~/.soroban-cost-estimator/cache.db`. The `config diff`
//! command cross-references cached estimates to tell the user which ones
//! are now stale due to network pricing changes.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::trace;
use tracing::warn;

use crate::error::AppError;
use crate::error::AppResult;

/// Check whether a rusqlite error is `SQLITE_BUSY`.
fn is_sqlite_busy(err: &rusqlite::Error) -> bool {
    err.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy)
}

/// Current cache-entry schema version.
///
/// Bump this whenever the on-disk `CachedEstimate` shape changes.
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

/// Optional filters for [`query_estimates`].
///
/// Every field is optional; a `None` field means "no filter on this axis".
/// All filters are combined with logical AND.
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    /// Case-insensitive substring match against the function name.
    pub function: Option<String>,
    /// Prefix match against the WASM SHA-256 hash (hex).
    pub wasm_hash: Option<String>,
    /// Inclusive lower bound on `total_stroops`.
    pub min_stroops: Option<i64>,
    /// Inclusive upper bound on `total_stroops`.
    pub max_stroops: Option<i64>,
    /// Inclusive lower bound on the estimate timestamp (ISO-8601).
    pub from: Option<String>,
    /// Inclusive upper bound on the estimate timestamp (ISO-8601).
    pub to: Option<String>,
}

/// Global lock for serializing cache writes.
///
/// SQLite WAL mode allows concurrent readers, but only one writer at a time.
/// Under heavy contention (e.g. multiple threads writing the same key),
/// `busy_timeout` alone may not prevent `SQLITE_BUSY`. This mutex ensures
/// writes are serialized at the application level.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Returns the base data directory path: `~/.soroban-cost-estimator`,
/// creating it if needed.
fn data_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::General("could not determine home directory".to_string()))?;
    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path to the SQLite cache database.
fn db_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("cache.db"))
}

/// Create the `estimates` table (and tune journal mode) on an already-open
/// connection if it does not exist yet.
///
/// Centralized so both the normal cache path and callers that open the
/// database directly (e.g. test helpers) create an identical schema.
pub fn ensure_cache_schema(conn: &Connection) -> AppResult<()> {
    // busy_timeout makes contending writers wait instead of failing with
    // SQLITE_BUSY. It is set first so the busy handler is installed before
    // any statement that can take a lock. The WAL journal-mode switch is the
    // one exception the busy handler does not cover (it needs exclusive
    // access), so it is handled separately below.
    conn.execute_batch(
        "PRAGMA busy_timeout=5000; \
         CREATE TABLE IF NOT EXISTS estimates (
            version          INTEGER NOT NULL,
            wasm_hash        TEXT NOT NULL,
            function         TEXT NOT NULL,
            args_hash        TEXT NOT NULL,
            network          TEXT NOT NULL,
            ledger           INTEGER NOT NULL,
            total_stroops    INTEGER NOT NULL,
            cpu_instructions INTEGER NOT NULL,
            memory_bytes     INTEGER NOT NULL,
            timestamp        TEXT NOT NULL,
            PRIMARY KEY (wasm_hash, function, args_hash)
        );",
    )?;
    enable_wal_if_possible(conn);
    Ok(())
}

/// Best-effort switch to WAL journal mode.
///
/// WAL lets concurrent readers and writers coexist; it is a performance
/// optimization and correctness never depends on it. Switching journal modes
/// requires exclusive access to the database file, and SQLite's busy handler
/// does **not** cover that particular lock, so a connection racing another
/// opener of a fresh database can see `SQLITE_BUSY` even with a long
/// `busy_timeout` set. Short-circuit when the file is already in WAL mode
/// (the common case after the first open), retry briefly during the initial
/// creation race, and otherwise continue with the file's current journal
/// mode (rollback) rather than failing the whole operation.
fn enable_wal_if_possible(conn: &Connection) {
    let Ok(mode) = conn.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0)) else {
        return;
    };
    if mode == "wal" {
        return;
    }
    for attempt in 0..5u32 {
        if conn.execute_batch("PRAGMA journal_mode=WAL;").is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(
            u64::from(attempt + 1) * 25,
        ));
    }
}

/// Open a connection to the cache database and ensure the schema exists.
fn open_db() -> AppResult<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)?;
    ensure_cache_schema(&conn)?;
    Ok(conn)
}

/// Retry limit and base delay for `SQLITE_BUSY` backoff.
const MAX_RETRIES: u32 = 5;
const BASE_RETRY_DELAY_MS: u64 = 10;

/// Execute a SQLite write operation, retrying on `SQLITE_BUSY` with
/// exponential backoff.
fn execute_with_retry<F, T>(mut operation: F) -> AppResult<T>
where
    F: FnMut() -> Result<T, rusqlite::Error>,
{
    let mut delay = BASE_RETRY_DELAY_MS;
    for attempt in 0..MAX_RETRIES {
        match operation() {
            Ok(val) => return Ok(val),
            Err(e) if is_sqlite_busy(&e) && attempt < MAX_RETRIES - 1 => {
                warn!(
                    attempt,
                    delay_ms = delay,
                    "SQLITE_BUSY, retrying after backoff"
                );
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay *= 2;
            }
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
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
/// None — local SQLite I/O.
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

    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::General(format!("cache write lock poisoned: {e}")))?;
    let conn = open_db()?;
    execute_with_retry(|| {
        conn.execute(
            "INSERT INTO estimates \
             (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(wasm_hash, function, args_hash) DO UPDATE SET \
                version = excluded.version, \
                network = excluded.network, \
                ledger = excluded.ledger, \
                total_stroops = excluded.total_stroops, \
                cpu_instructions = excluded.cpu_instructions, \
                memory_bytes = excluded.memory_bytes, \
                timestamp = excluded.timestamp",
            rusqlite::params![
                CACHE_SCHEMA_VERSION as i64,
                wasm_hash,
                function,
                args_hash.as_str(),
                network,
                ledger as i64,
                total_stroops,
                cpu_instructions as i64,
                memory_bytes as i64,
                chrono::Utc::now().to_rfc3339(),
            ],
        )
    })?;

    debug!(function, network, ledger, "estimate cached (sqlite)");
    Ok(())
}

/// Reconstruct a [`CachedEstimate`] from a SQLite row.
fn estimate_from_row(row: &rusqlite::Row<'_>) -> Result<CachedEstimate, rusqlite::Error> {
    Ok(CachedEstimate {
        version: row.get(0)?,
        wasm_hash: row.get(1)?,
        function: row.get(2)?,
        args_hash: row.get(3)?,
        network: row.get(4)?,
        ledger: row.get::<_, i64>(5)? as u32,
        total_stroops: row.get(6)?,
        cpu_instructions: row.get::<_, i64>(7)? as u64,
        memory_bytes: row.get::<_, i64>(8)? as u64,
        timestamp: row.get(9)?,
    })
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

    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
         cpu_instructions, memory_bytes, timestamp \
         FROM estimates WHERE wasm_hash = ?1 AND function = ?2 AND args_hash = ?3",
    )?;

    let mut rows = stmt.query(rusqlite::params![wasm_hash, function, args_hash.as_str()])?;
    match rows.next()? {
        None => Ok(None),
        Some(row) => {
            let cached = estimate_from_row(row)?;
            let cached = migrate_to_latest(cached)?;
            Ok(Some(cached))
        }
    }
}

/// Whether a cached estimate is still fresh, i.e. its timestamp is within
/// `ttl` of now.
///
/// Entries whose timestamp cannot be parsed as RFC 3339 are treated as **not**
/// fresh: an unverifiable age must not be trusted, so the caller re-simulates
/// and overwrites the entry.
///
/// # Network calls
/// None — pure time comparison.
pub fn is_cache_entry_fresh(entry: &CachedEstimate, ttl: std::time::Duration) -> bool {
    let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&entry.timestamp) else {
        return false;
    };
    let ts = ts.with_timezone(&chrono::Utc);
    let Ok(ttl) = chrono::TimeDelta::from_std(ttl) else {
        return false;
    };
    chrono::Utc::now().signed_duration_since(ts) <= ttl
}

/// Load a cached estimate only if it is still fresh (within `ttl`).
///
/// Returns `Ok(None)` when no entry exists **or** when the entry has
/// expired — both mean "re-simulate".
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn load_fresh_estimate(
    wasm_hash: &str,
    function: &str,
    args: &[String],
    ttl: std::time::Duration,
) -> AppResult<Option<CachedEstimate>> {
    let Some(cached) = load_estimate(wasm_hash, function, args)? else {
        return Ok(None);
    };
    if is_cache_entry_fresh(&cached, ttl) {
        trace!(function, ttl_secs = ttl.as_secs(), "fresh cached estimate");
        Ok(Some(cached))
    } else {
        trace!(
            function,
            ttl_secs = ttl.as_secs(),
            timestamp = %cached.timestamp,
            "cached estimate expired"
        );
        Ok(None)
    }
}

/// Find all cached estimates for a given network.
///
/// Used by `config diff` to check which cached estimates are now stale
/// after a pricing change. Results are ordered newest-first.
pub fn list_cached_estimates(network: &str) -> AppResult<Vec<CachedEstimate>> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
         cpu_instructions, memory_bytes, timestamp \
         FROM estimates WHERE network = ?1 ORDER BY timestamp DESC",
    )?;

    let rows = stmt.query_map([network], estimate_from_row)?;

    let mut estimates = Vec::new();
    for row in rows {
        let cached = row?;
        // Skip entries we cannot safely migrate forward (e.g. written by a
        // newer tool); they do not belong in a network listing.
        if let Ok(cached) = migrate_to_latest(cached) {
            estimates.push(cached);
        }
    }

    trace!(network, count = estimates.len(), "listed cached estimates");
    Ok(estimates)
}

/// Parse an ISO-8601 timestamp into a UTC `DateTime`.
fn parse_ts(s: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .map_err(|e| AppError::General(format!("invalid timestamp {s:?}: {e}")))?;
    Ok(dt.with_timezone(&chrono::Utc))
}

/// Query cached estimates for `network`, applying the optional filters in
/// [`QueryFilter`].
///
/// Results are returned newest-first (by `timestamp`). The filters are:
/// * `function` — case-insensitive substring match
/// * `wasm_hash` — prefix match
/// * `min_stroops` / `max_stroops` — inclusive `total_stroops` range
/// * `from` / `to` — inclusive timestamp range (ISO-8601)
///
/// # Network calls
/// None — pure file I/O.
pub fn query_estimates(network: &str, filter: &QueryFilter) -> AppResult<Vec<CachedEstimate>> {
    let mut estimates = list_cached_estimates(network)?;

    // Newest-first ordering.
    estimates.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let from_ts = match &filter.from {
        Some(s) => Some(parse_ts(s)?),
        None => None,
    };
    let to_ts = match &filter.to {
        Some(s) => Some(parse_ts(s)?),
        None => None,
    };

    let filtered: Vec<CachedEstimate> = estimates
        .into_iter()
        .filter(|e| {
            if let Some(f) = &filter.function {
                let f = f.to_lowercase();
                if !e.function.to_lowercase().contains(f.as_str()) {
                    return false;
                }
            }
            if let Some(w) = &filter.wasm_hash {
                if !e.wasm_hash.starts_with(w.as_str()) {
                    return false;
                }
            }
            if let Some(min) = filter.min_stroops {
                if e.total_stroops < min {
                    return false;
                }
            }
            if let Some(max) = filter.max_stroops {
                if e.total_stroops > max {
                    return false;
                }
            }
            if let Some(from) = &from_ts {
                let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&e.timestamp) else {
                    return false;
                };
                if ts.with_timezone(&chrono::Utc) < *from {
                    return false;
                }
            }
            if let Some(to) = &to_ts {
                let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&e.timestamp) else {
                    return false;
                };
                if ts.with_timezone(&chrono::Utc) > *to {
                    return false;
                }
            }
            true
        })
        .collect();

    trace!(network, count = filtered.len(), "queried cached estimates");
    Ok(filtered)
}

/// Integrity status of a single cache entry.
#[derive(Debug, Clone)]
pub struct CacheEntryStatus {
    /// Synthesized identity of the cache entry
    /// (e.g. `"abc123-my_func-def456.json"`).
    pub filename: String,
    /// Schema version parsed from the entry.
    pub version: Option<u32>,
    /// Whether the entry parsed as a valid, readable `CachedEstimate`.
    /// Entries carrying a schema newer than the current one are not valid.
    pub valid: bool,
}

/// Verify the integrity of every entry in the estimate cache.
///
/// Reads each row in the SQLite database and checks that it parses as a valid
/// [`CachedEstimate`] and carries a schema this tool can read. Entries from the
/// future (version > current) parse fine but are not migratable to the current
/// schema, so they are flagged.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn verify_cache() -> AppResult<Vec<CacheEntryStatus>> {
    let conn = open_db()?;
    let mut stmt = conn.prepare("SELECT version, wasm_hash, function, args_hash FROM estimates")?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut statuses = Vec::new();
    for row in rows {
        let (version, wasm_hash, function, args_hash) = row?;
        let filename = format!("{wasm_hash}-{function}-{args_hash}.json");

        // A row counts as valid when it both parses as a `CachedEstimate` and
        // carries a schema this tool can read. Entries from the future
        // (version > current) parse fine but are not migratable, so they are
        // flagged.
        let cached = CachedEstimate {
            version,
            wasm_hash,
            function,
            args_hash,
            network: String::new(),
            ledger: 0,
            total_stroops: 0,
            cpu_instructions: 0,
            memory_bytes: 0,
            timestamp: String::new(),
        };
        let valid = migrate_to_latest(cached).is_ok();

        if !valid {
            warn!(filename, "corrupt or unsupported cache entry");
        }

        statuses.push(CacheEntryStatus {
            filename,
            version: Some(version),
            valid,
        });
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
type WasmRegistry = std::collections::HashMap<String, WasmFileRecord>;

/// Path to the on-disk registry of WASM file identities.
///
/// Lives in the data directory (not the cache database) so that WASM identity
/// tracking stays independent of estimate storage.
fn registry_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("wasm-files.json"))
}

/// Load the WASM file registry, or an empty one if it does not exist yet.
fn load_registry() -> AppResult<WasmRegistry> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
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
/// Returns the number of cache rows removed. Used by
/// [`invalidate_if_wasm_changed`] to drop entries from a previous build once
/// the WASM file has changed.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn remove_cached_estimates_for_wasm(wasm_hash: &str) -> AppResult<usize> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::General(format!("cache write lock poisoned: {e}")))?;
    let conn = open_db()?;
    let removed = execute_with_retry(|| {
        conn.execute("DELETE FROM estimates WHERE wasm_hash = ?1", [wasm_hash])
    })?;
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
