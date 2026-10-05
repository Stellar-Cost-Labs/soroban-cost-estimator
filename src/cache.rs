//! Estimate result caching (SQLite-backed).
//!
//! Stores past `estimate` results in a single SQLite database at
//! `~/.soroban-cost-estimator/cache.db`. The `config diff`
//! command cross-references cached estimates to tell the user which ones
//! are now stale due to network pricing changes.
//!
//! # Storage layout
//!
//! A single `estimates` table with a composite primary key
//! `(wasm_hash, function, args_hash)` plus secondary indexes for the lookup
//! patterns the CLI uses (`network` + timestamp listings, LRU eviction by
//! `last_accessed`). SQLite gives atomic transactions, indexed lookups, and
//! a single compact file — no per-entry inode overhead.
//!
//! # Schema versioning
//!
//! Every row records a `version` (serialized as `schema_version`). Rows
//! written by older releases are migrated forward by
//! [`migrate_to_latest`]; rows from a newer release are rejected with an
//! upgrade hint rather than silently misread.
//!
//! # LRU eviction
//!
//! [`save_estimate`] keeps the cache bounded: once the configured byte or
//! entry quota is exceeded, the least-recently-accessed rows are deleted
//! until usage drops below 90% of the quota. See [`CacheLimits`].

use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use rusqlite::Transaction;
use rusqlite::TransactionBehavior;
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
///
/// Version history:
/// * `1` — initial schema (`duration_ms`/`success` absent; the implicit
///   version of unversioned legacy entries).
/// * `2` — added `duration_ms` and `success`.
/// * `3` — added `last_accessed` for LRU eviction.
pub const CACHE_SCHEMA_VERSION: u32 = 3;

/// Implicit schema version of cache entries written before the `version`
/// field existed.
///
/// Those legacy entries have no `version` key in their JSON, so serde's
/// `default` fills in this value via [`default_schema_version`]. They are
/// the first schema version and require no transformation to reach the
/// current schema.
pub const INITIAL_SCHEMA_VERSION: u32 = 1;

/// Schema version that introduced `duration_ms` and `success`.
pub const DURATION_SCHEMA_VERSION: u32 = 2;

/// Schema version that introduced `last_accessed` (LRU eviction support).
pub const ACCESS_TRACKING_SCHEMA_VERSION: u32 = 3;

/// Fraction of the configured *entry* quota the cache is trimmed back to
/// after an eviction pass. Deleting down to 90% of the limit avoids an
/// eviction on every single save once the cache is full.
const EVICTION_TARGET_NUMERATOR: u64 = 9;
const EVICTION_TARGET_DENOMINATOR: u64 = 10;

/// Maximum number of eviction rounds in one pass.
///
/// A safety valve: each round deletes at least one row, so a well-formed
/// cache drains in far fewer rounds. The cap guarantees termination even if
/// the on-disk size cannot be measured on some platform.
const MAX_EVICTION_ROUNDS: usize = 100_000;

/// serde default for the `success` field — most estimates succeed.
fn default_true() -> bool {
    true
}

/// serde default for the `version` field, applied when an older (or hand
/// written) entry omits it. Legacy entries predating the version field are
/// treated as [`INITIAL_SCHEMA_VERSION`].
fn default_schema_version() -> u32 {
    INITIAL_SCHEMA_VERSION
}

/// Ledger I/O footprint of a simulated invocation.
///
/// Recorded alongside an estimate so a later run can diff I/O against it.
/// Only runs that captured the footprint store this (older entries leave it
/// absent), which is why it lives behind [`CachedEstimate::io`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IoFootprint {
    /// Ledger entries read.
    pub read_entries: u32,
    /// Ledger entries written.
    pub write_entries: u32,
    /// Bytes read from the ledger.
    pub read_bytes: u32,
    /// Bytes written to the ledger.
    pub write_bytes: u32,
}

/// A cached estimate result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEstimate {
    /// Schema version of this entry. Legacy entries default to
    /// [`INITIAL_SCHEMA_VERSION`] when the field is absent.
    ///
    /// Serialized as `schema_version`; the pre-rename `version` key is still
    /// accepted on deserialization for backward compatibility with cache
    /// exports written by earlier releases.
    #[serde(
        rename = "schema_version",
        alias = "version",
        default = "default_schema_version"
    )]
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
    /// Simulation wall-clock duration in milliseconds (`None` if unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Whether the simulation succeeded.
    #[serde(default = "default_true")]
    pub success: bool,
    /// Ledger I/O footprint recorded with this estimate, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io: Option<IoFootprint>,
}

/// Filter criteria for querying cached simulation estimates.
///
/// Every field is optional; a `None` field means "no filter on this axis".
/// All filters are combined with logical AND.
#[derive(Debug, Clone, Default)]
pub struct CacheFilter {
    /// Exact match against the function name.
    pub function: Option<String>,
    /// Prefix or exact match against the WASM SHA-256 hash (hex).
    pub wasm_hash: Option<String>,
    /// Exact match against the network name.
    pub network: Option<String>,
    /// Inclusive lower bound on total fee in stroops.
    pub min_fee: Option<i64>,
    /// Inclusive upper bound on total fee in stroops.
    pub max_fee: Option<i64>,
    /// Inclusive lower bound on the estimate timestamp (UTC).
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Inclusive upper bound on the estimate timestamp (UTC).
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

impl CacheFilter {
    /// Validate filter parameters, returning an error for invalid inputs.
    pub fn validate(&self) -> AppResult<()> {
        if let Some(net) = &self.network {
            crate::rpc::client::resolve_endpoint(net, None)?;
        }
        if let Some(f) = &self.function {
            if f.is_empty() {
                return Err(AppError::General(
                    "function name filter cannot be empty".to_string(),
                ));
            }
        }
        if let Some(hash) = &self.wasm_hash {
            let hash_trimmed = hash.trim();
            if hash_trimmed.is_empty()
                || hash_trimmed.len() > 64
                || !hash_trimmed.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(AppError::General(format!(
                    "invalid wasm hash filter {hash:?}: expected up to 64 hexadecimal characters"
                )));
            }
        }
        if let Some(min) = self.min_fee {
            if min < 0 {
                return Err(AppError::General(format!(
                    "min-fee cannot be negative: {min}"
                )));
            }
        }
        if let Some(max) = self.max_fee {
            if max < 0 {
                return Err(AppError::General(format!(
                    "max-fee cannot be negative: {max}"
                )));
            }
        }
        if let (Some(min), Some(max)) = (self.min_fee, self.max_fee) {
            if min > max {
                return Err(AppError::General(format!(
                    "invalid fee range: min-fee ({min}) cannot exceed max-fee ({max})"
                )));
            }
        }
        if let (Some(since), Some(to)) = (self.since, self.to) {
            if since > to {
                return Err(AppError::General(format!(
                    "invalid date range: since ({since}) cannot be after to ({to})"
                )));
            }
        }
        Ok(())
    }

    /// Check whether a cached estimate satisfies all filter criteria.
    pub fn matches(&self, entry: &CachedEstimate) -> bool {
        if let Some(f) = &self.function {
            if entry.function != *f {
                return false;
            }
        }
        if let Some(w) = &self.wasm_hash {
            let entry_hash = entry.wasm_hash.to_ascii_lowercase();
            let query_hash = w.trim().to_ascii_lowercase();
            if !entry_hash.starts_with(&query_hash) {
                return false;
            }
        }
        if let Some(net) = &self.network {
            if entry.network != *net {
                return false;
            }
        }
        if let Some(min) = self.min_fee {
            if entry.total_stroops < min {
                return false;
            }
        }
        if let Some(max) = self.max_fee {
            if entry.total_stroops > max {
                return false;
            }
        }
        if let Some(since) = &self.since {
            let Some(ts) = parse_entry_timestamp(&entry.timestamp) else {
                return false;
            };
            if ts < *since {
                return false;
            }
        }
        if let Some(to) = &self.to {
            let Some(ts) = parse_entry_timestamp(&entry.timestamp) else {
                return false;
            };
            if ts > *to {
                return false;
            }
        }
        true
    }
}

/// Optional filters for [`query_estimates`] (legacy filter type).
///
/// Prefer using [`CacheFilter`] with [`query_cache`].
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

impl From<QueryFilter> for CacheFilter {
    fn from(q: QueryFilter) -> Self {
        Self {
            function: q.function,
            wasm_hash: q.wasm_hash,
            network: None,
            min_fee: q.min_stroops,
            max_fee: q.max_stroops,
            since: q
                .from
                .as_deref()
                .and_then(|s| parse_since_timestamp(s).ok()),
            to: q.to.as_deref().and_then(|s| parse_to_timestamp(s).ok()),
        }
    }
}

/// Global lock for serializing cache writes.
///
/// SQLite WAL mode allows concurrent readers, but only one writer at a time.
/// Under heavy contention (e.g. multiple threads writing the same key),
/// `busy_timeout` alone may not prevent `SQLITE_BUSY`. This mutex ensures
/// writes are serialized at the application level.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Default on-disk cache byte quota (50 MB).
const DEFAULT_MAX_CACHE_BYTES: u64 = 50 * 1024 * 1024;

/// Default maximum number of cache rows (10,000).
const DEFAULT_MAX_CACHE_ENTRIES: usize = 10_000;

/// Quotas that bound the on-disk estimate cache.
///
/// Both limits are enforced together on every [`save_estimate`]: once either
/// is exceeded, the least-recently-accessed rows are deleted until usage
/// falls back under 90% of both quotas. A quota of `0` disables that axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    /// Maximum logical database size in bytes (`0` = unbounded).
    pub max_bytes: u64,
    /// Maximum number of stored estimates (`0` = unbounded).
    pub max_entries: usize,
}

impl CacheLimits {
    /// The shipped defaults: 50 MB / 10,000 entries.
    pub const DEFAULT: Self = Self {
        max_bytes: DEFAULT_MAX_CACHE_BYTES,
        max_entries: DEFAULT_MAX_CACHE_ENTRIES,
    };

    /// No limits at all — used by tests and callers that opt out.
    pub const UNBOUNDED: Self = Self {
        max_bytes: 0,
        max_entries: 0,
    };

    /// Whether both quotas are disabled.
    #[must_use]
    pub const fn is_unbounded(&self) -> bool {
        self.max_bytes == 0 && self.max_entries == 0
    }
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Process-wide cache quotas, overridable from the CLI
/// (`--max-cache-size-mb` / `--max-cache-entries`).
///
/// [`save_estimate`] reads this on every write. Tests that exercise eviction
/// use [`save_estimate_with_limits`] instead of mutating process globals, so
/// parallel tests never interfere with each other.
static CACHE_LIMITS: Mutex<CacheLimits> = Mutex::new(CacheLimits::DEFAULT);

/// Override the process-wide cache quotas.
pub fn set_cache_limits(limits: CacheLimits) -> AppResult<()> {
    let mut guard = CACHE_LIMITS
        .lock()
        .map_err(|e| AppError::General(format!("cache limits lock poisoned: {e}")))?;
    *guard = limits;
    debug!(
        max_bytes = limits.max_bytes,
        max_entries = limits.max_entries,
        "cache limits updated"
    );
    Ok(())
}

/// The current process-wide cache quotas.
pub fn cache_limits() -> CacheLimits {
    CACHE_LIMITS
        .lock()
        .map(|g| *g)
        .unwrap_or(CacheLimits::DEFAULT)
}

/// Returns the base data directory path: `~/.soroban-cost-estimator`,
/// creating it if needed.
fn data_dir() -> AppResult<PathBuf> {
    let dir = crate::paths::data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path to the SQLite cache database.
fn db_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("cache.db"))
}

/// Create the `estimates` table and its indexes (and tune journal mode) on an
/// already-open connection if it does not exist yet.
///
/// Centralized so both the normal cache path and callers that open the
/// database directly (e.g. test helpers) create an identical schema.
///
/// This is also the migration entry point for the *table* shape: databases
/// created by older releases are upgraded in place with
/// `ALTER TABLE ... ADD COLUMN` and new `CREATE INDEX` statements, so a
/// user's existing cache keeps working across upgrades.
pub fn ensure_cache_schema(conn: &Connection) -> AppResult<()> {
    // busy_timeout makes contending writers wait instead of failing with
    // SQLITE_BUSY. It is set first so the busy handler is installed before
    // any statement that can take a lock. The WAL journal-mode switch is the
    // one exception the busy handler does not cover (it needs exclusive
    // access), so it is handled separately below.
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;
    // Incremental auto-vacuum lets an eviction pass return freed pages to
    // the filesystem without a full, blocking VACUUM. It only takes effect
    // when set before the first table is created, which is the case for new
    // databases; existing databases keep their current setting.
    conn.execute_batch("PRAGMA auto_vacuum=INCREMENTAL;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS estimates (
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
            duration_ms      INTEGER,
            success          INTEGER NOT NULL DEFAULT 1,
            last_accessed    TEXT NOT NULL DEFAULT '',
            io_json          TEXT,
            PRIMARY KEY (wasm_hash, function, args_hash)
        );",
    )?;
    // Migrate a pre-v3 table (created before LRU access tracking) in place.
    ensure_column(conn, "last_accessed", "TEXT NOT NULL DEFAULT ''")?;
    // The ledger I/O footprint is additive and nullable, so adding it in
    // place keeps older databases readable without a schema-version bump.
    ensure_column(conn, "io_json", "TEXT")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_estimates_lookup
             ON estimates (wasm_hash, function);
         CREATE INDEX IF NOT EXISTS idx_estimates_network_created
             ON estimates (network, timestamp DESC);
         CREATE INDEX IF NOT EXISTS idx_estimates_key_net_created
             ON estimates (wasm_hash, function, network, timestamp DESC);
         CREATE INDEX IF NOT EXISTS idx_estimates_last_accessed
             ON estimates (last_accessed);",
    )?;
    enable_wal_if_possible(conn);
    Ok(())
}

/// Whether `estimates` already has a column named `name`.
fn column_exists(conn: &Connection, name: &str) -> AppResult<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(estimates)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let column: String = row.get(1)?;
        if column == name {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Add `name` to the `estimates` table when a pre-migration database lacks
/// it. A no-op on up-to-date databases.
fn ensure_column(conn: &Connection, name: &str, definition: &str) -> AppResult<()> {
    if column_exists(conn, name)? {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE estimates ADD COLUMN {name} {definition};"
    ))?;
    debug!(column = name, "added missing cache column");
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
        let _ = conn.execute_batch("PRAGMA synchronous=NORMAL;");
        return;
    }
    for attempt in 0..10u32 {
        if conn.execute_batch("PRAGMA journal_mode=WAL;").is_ok() {
            let _ = conn.execute_batch("PRAGMA synchronous=NORMAL;");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(
            u64::from(attempt + 1) * 50,
        ));
    }
}

/// Open a connection to the cache database, ensure the schema exists, and
/// import any pre-SQLite JSON cache files left behind by older releases.
fn open_db() -> AppResult<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)?;
    ensure_cache_schema(&conn)?;
    // Transparent, one-time upgrade from the old per-entry JSON cache.
    // The directory is removed once every file is imported, so subsequent
    // opens short-circuit on `!dir.is_dir()`.
    migrate_legacy_json_cache(&conn)?;
    Ok(conn)
}

/// Directory used by pre-SQLite releases for per-estimate JSON files:
/// `~/.soroban-cost-estimator/cache/`.
fn legacy_cache_dir() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("cache"))
}

/// A cache entry as written by the pre-SQLite JSON store.
///
/// Every field defaults, so a file written by any older release parses even
/// if it predates a field. The `version` key defaults to the initial schema
/// when absent (v0 → v1 migration).
#[derive(Debug, Deserialize)]
struct LegacyJsonEntry {
    #[serde(
        rename = "schema_version",
        alias = "version",
        default = "default_schema_version"
    )]
    version: u32,
    #[serde(default)]
    wasm_hash: String,
    #[serde(default)]
    function: String,
    #[serde(default)]
    args_hash: String,
    #[serde(default)]
    network: String,
    #[serde(default)]
    ledger: u32,
    #[serde(default)]
    total_stroops: i64,
    #[serde(default)]
    cpu_instructions: u64,
    #[serde(default)]
    memory_bytes: u64,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default = "default_true")]
    success: bool,
}

/// Import cache entries written by the pre-SQLite JSON store, then retire
/// the old files.
///
/// Runs transparently on the first database open after an upgrade: each
/// `*.json` file in `~/.soroban-cost-estimator/cache/` is parsed, migrated to
/// the current schema, and inserted with `INSERT OR IGNORE` so a row already
/// present in SQLite always wins. Successfully imported files are deleted;
/// unreadable ones are renamed to `*.json.rejected` (kept, not retried). Once
/// the directory is empty it is removed, which makes subsequent opens a
/// single `is_dir()` check.
///
/// Returns the number of entries imported.
///
/// # Network calls
/// None — pure local file + SQLite I/O.
fn migrate_legacy_json_cache(conn: &Connection) -> AppResult<usize> {
    let dir = legacy_cache_dir()?;
    if !dir.is_dir() {
        return Ok(0);
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(dir = %dir.display(), error = %e, "could not read legacy cache directory");
            return Ok(0);
        }
    };

    let mut imported = 0usize;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("json") {
            continue;
        }
        match import_legacy_json_file(conn, &path) {
            Ok(true) => imported += 1,
            Ok(false) => {}
            // A locked or unreadable file must not break a cache read: keep
            // it and retry on a later open.
            Err(e) => {
                warn!(path = %path.display(), error = %e, "could not import legacy cache entry; keeping file");
            }
        }
    }

    if imported > 0 {
        debug!(imported, "imported legacy JSON cache entries into SQLite");
    }

    // Retire the directory once nothing importable is left. Any rejected or
    // unreadable files keep it alive so they remain discoverable.
    let remaining = std::fs::read_dir(&dir)
        .map(|it| it.filter_map(Result::ok).any(|e| e.path().is_file()))
        .unwrap_or(true);
    if !remaining {
        let _ = std::fs::remove_dir(&dir);
    }

    Ok(imported)
}

/// Import a single legacy JSON cache file into SQLite.
///
/// Returns `Ok(true)` when the entry was persisted and the file retired,
/// `Ok(false)` when the file was unparseable (renamed to `*.json.rejected`
/// so it is kept but not retried). A database error is propagated so the
/// caller can keep the file and try again later.
fn import_legacy_json_file(conn: &Connection, path: &Path) -> AppResult<bool> {
    let raw = std::fs::read_to_string(path)?;

    let parsed: LegacyJsonEntry = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(e) => {
            // Keep the file under a new extension so it is neither lost nor
            // retried on every future open.
            warn!(
                path = %path.display(),
                error = %e,
                "could not parse legacy cache file; renaming it"
            );
            let _ = std::fs::rename(path, path.with_extension("json.rejected"));
            return Ok(false);
        }
    };

    if parsed.wasm_hash.is_empty() {
        warn!(path = %path.display(), "legacy cache file has no wasm_hash; skipping");
        let _ = std::fs::rename(path, path.with_extension("json.rejected"));
        return Ok(false);
    }

    // Migrate first (which also rejects impossible future schemas), then
    // persist with the current version stamp. An entry from a newer tool is
    // left on disk untouched rather than discarded.
    let migrated = CachedEstimate {
        version: parsed.version,
        wasm_hash: parsed.wasm_hash,
        function: parsed.function,
        args_hash: parsed.args_hash,
        network: parsed.network,
        ledger: parsed.ledger,
        total_stroops: parsed.total_stroops,
        cpu_instructions: parsed.cpu_instructions,
        memory_bytes: parsed.memory_bytes,
        timestamp: parsed.timestamp.clone(),
        duration_ms: parsed.duration_ms,
        success: parsed.success,
        // A legacy JSON entry predates the I/O footprint, so there is none
        // to carry over.
        io: None,
    };
    let Ok(migrated) = migrate_to_latest(migrated) else {
        warn!(path = %path.display(), "legacy cache entry needs a newer tool; keeping file");
        return Ok(false);
    };

    conn.execute(
        "INSERT OR IGNORE INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
          cpu_instructions, memory_bytes, timestamp, duration_ms, success, last_accessed) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            CACHE_SCHEMA_VERSION as i64,
            migrated.wasm_hash,
            migrated.function,
            migrated.args_hash,
            migrated.network,
            migrated.ledger as i64,
            migrated.total_stroops,
            migrated.cpu_instructions as i64,
            migrated.memory_bytes as i64,
            migrated.timestamp,
            migrated.duration_ms.map(|v| v as i64),
            migrated.success as i64,
            migrated.timestamp,
        ],
    )?;

    std::fs::remove_file(path)?;
    Ok(true)
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

/// Save an estimate result to the cache using the process-wide
/// [`cache_limits`] for LRU eviction.
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
/// * `duration_ms` - Wall-clock duration of the simulation in milliseconds.
/// * `success` - Whether the simulation succeeded.
///
/// The ledger I/O footprint is not recorded here — use
/// [`save_estimate_with_io`] when it is available.
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
    duration_ms: Option<u64>,
    success: bool,
) -> AppResult<()> {
    save_estimate_with_limits(
        wasm_hash,
        function,
        args,
        network,
        ledger,
        total_stroops,
        cpu_instructions,
        memory_bytes,
        None,
        duration_ms,
        success,
        cache_limits(),
    )
}

/// Save an estimate together with its ledger I/O footprint.
///
/// `save_estimate` records only the metrics the cache has always stored; this
/// variant additionally persists the read/write entry and byte counts, so a
/// later run can diff I/O against it (`estimate --compare`).
///
/// # Network calls
/// None — local SQLite I/O.
#[allow(clippy::too_many_arguments)]
pub fn save_estimate_with_io(
    wasm_hash: &str,
    function: &str,
    args: &[String],
    network: &str,
    ledger: u32,
    total_stroops: i64,
    cpu_instructions: u64,
    memory_bytes: u64,
    io: IoFootprint,
    duration_ms: Option<u64>,
    success: bool,
) -> AppResult<()> {
    save_estimate_with_limits(
        wasm_hash,
        function,
        args,
        network,
        ledger,
        total_stroops,
        cpu_instructions,
        memory_bytes,
        Some(io),
        duration_ms,
        success,
        cache_limits(),
    )
}

/// Save an estimate result to the cache with an explicit eviction quota.
///
/// Identical to [`save_estimate`] except that the caller supplies the
/// [`CacheLimits`] to enforce, which keeps eviction tests independent of the
/// process-wide configuration. The insert and the eviction pass run in a
/// single `BEGIN IMMEDIATE` transaction, so a crash can never leave the
/// cache over quota, and concurrent writers serialize on SQLite's write
/// lock (the in-process `WRITE_LOCK` additionally serializes threads).
#[allow(clippy::too_many_arguments)]
pub fn save_estimate_with_limits(
    wasm_hash: &str,
    function: &str,
    args: &[String],
    network: &str,
    ledger: u32,
    total_stroops: i64,
    cpu_instructions: u64,
    memory_bytes: u64,
    io: Option<IoFootprint>,
    duration_ms: Option<u64>,
    success: bool,
    limits: CacheLimits,
) -> AppResult<()> {
    let args_hash = hash_args(args);
    let io_json = match io {
        Some(io) => Some(serde_json::to_string(&io)?),
        None => None,
    };

    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::General(format!("cache write lock poisoned: {e}")))?;
    let mut conn = open_db()?;
    let now = chrono::Utc::now().to_rfc3339();

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp, duration_ms, success, last_accessed, io_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
         ON CONFLICT(wasm_hash, function, args_hash) DO UPDATE SET \
            version = excluded.version, \
            network = excluded.network, \
            ledger = excluded.ledger, \
            total_stroops = excluded.total_stroops, \
            cpu_instructions = excluded.cpu_instructions, \
            memory_bytes = excluded.memory_bytes, \
            timestamp = excluded.timestamp, \
            duration_ms = excluded.duration_ms, \
            success = excluded.success, \
            last_accessed = excluded.last_accessed, \
            io_json = excluded.io_json",
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
            now,
            duration_ms.map(|v| v as i64),
            success as i64,
            now,
            io_json,
        ],
    )?;

    let evicted = enforce_limits_in_tx(&tx, limits)?;
    tx.commit()?;

    if evicted > 0 {
        info_eviction(evicted, limits);
    }

    debug!(function, network, ledger, "estimate cached (sqlite)");
    Ok(())
}

/// Log an eviction pass at info level so users notice disk pressure.
fn info_eviction(evicted: usize, limits: CacheLimits) {
    tracing::info!(
        evicted,
        max_bytes = limits.max_bytes,
        max_entries = limits.max_entries,
        "cache quota reached — evicted least-recently-accessed estimates"
    );
}

/// Enforce `limits` inside an open write transaction.
///
/// Returns the number of rows evicted. Rows are deleted oldest-`last_accessed`
/// first until the cache fits both quotas: the entry quota is trimmed to 90%
/// of its limit (headroom so eviction does not run on every subsequent save)
/// and the byte quota is satisfied exactly, since live page counts move in
/// page-sized steps rather than rows.
///
/// The on-disk size is measured as live database pages
/// (`page_count - freelist_count`) times the page size, so deleting rows
/// immediately reduces the measured size even before the pages are returned
/// to the filesystem.
fn enforce_limits_in_tx(tx: &Transaction<'_>, limits: CacheLimits) -> AppResult<usize> {
    // `Transaction` derefs to `Connection`, so the shared size helper works
    // for both transaction-scoped and plain-connection callers.
    if limits.is_unbounded() {
        return Ok(0);
    }

    let target_entries = limits.max_entries * EVICTION_TARGET_NUMERATOR as usize
        / EVICTION_TARGET_DENOMINATOR as usize;
    let mut evicted = 0usize;

    for _ in 0..MAX_EVICTION_ROUNDS {
        let entries: usize =
            tx.query_row("SELECT COUNT(*) FROM estimates", [], |row| row.get(0))?;
        let bytes = live_db_bytes(tx)?;

        let over_entries = limits.max_entries > 0 && entries > limits.max_entries;
        let over_bytes = limits.max_bytes > 0 && bytes > limits.max_bytes;
        if !over_entries && !over_bytes {
            break;
        }

        // A byte quota below SQLite's structural floor (page 1 plus one root
        // page per table/index) can never be met, and evicting the last
        // entry cannot free any space either — stop rather than wipe the
        // cache chasing an impossible target.
        if over_bytes && !over_entries && entries <= 1 {
            break;
        }

        // Delete a batch of the oldest rows each round: enough to make
        // progress toward the quota, small enough to avoid over-evicting a
        // large cache.
        let batch = eviction_batch(entries, bytes, limits, target_entries);
        let removed = tx.execute(
            "DELETE FROM estimates WHERE rowid IN (\
                 SELECT rowid FROM estimates \
                 ORDER BY last_accessed ASC, timestamp ASC \
                 LIMIT ?1\
             )",
            [batch as i64],
        )?;
        evicted += removed;
        if removed == 0 {
            // Nothing left to delete; the quota cannot be satisfied further
            // (e.g. a single entry larger than the byte quota).
            break;
        }
    }

    if evicted > 0 {
        // Return freed pages to the filesystem where incremental auto-vacuum
        // is enabled; best-effort, since it is a space optimization only.
        let _ = tx.execute_batch("PRAGMA incremental_vacuum;");
    }

    Ok(evicted)
}

/// How many rows to evict in one round.
///
/// The batch is sized to make real progress: when an entry-count quota is in
/// play it aims straight for the 90% target, and when only the byte quota is
/// exceeded it deletes roughly the number of entries whose average size adds
/// up to the excess. Both are capped so the cache is never emptied by a
/// byte-driven pass.
fn eviction_batch(entries: usize, bytes: u64, limits: CacheLimits, target_entries: usize) -> usize {
    if limits.max_entries > 0 && target_entries > 0 && entries > target_entries {
        return entries - target_entries;
    }
    if limits.max_bytes > 0 && bytes > limits.max_bytes {
        let excess = bytes - limits.max_bytes;
        let per_entry = (bytes / entries.max(1) as u64).max(1);
        let from_bytes = usize::try_from(excess / per_entry).unwrap_or(1).max(1);
        // Never delete the final entry: dropping it cannot free space.
        return from_bytes.min(entries.saturating_sub(1)).max(1);
    }
    1
}

/// Live (non-free) size of the database in bytes.
///
/// Uses `page_count - freelist_count` rather than the filesystem size so the
/// measurement drops as soon as rows are deleted, even before SQLite reuses
/// or returns the freed pages. This is also the number [`evict_lru`] compares
/// against `CacheLimits::max_bytes`.
fn live_db_bytes(conn: &Connection) -> AppResult<u64> {
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let free_pages: i64 = conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let live_pages = (page_count - free_pages).max(0);
    Ok((live_pages as u64).saturating_mul(page_size as u64))
}

/// Evict least-recently-accessed entries so the cache fits the process-wide
/// quotas. Returns the number of rows deleted.
///
/// Backs the `cache prune` subcommand: unlike [`save_estimate`], it can be
/// invoked on demand against an already-over-quota cache.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn evict_lru() -> AppResult<usize> {
    let limits = cache_limits();
    if limits.is_unbounded() {
        return Ok(0);
    }
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::General(format!("cache write lock poisoned: {e}")))?;
    let mut conn = open_db()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let evicted = enforce_limits_in_tx(&tx, limits)?;
    tx.commit()?;
    if evicted > 0 {
        info_eviction(evicted, limits);
    }
    Ok(evicted)
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
        duration_ms: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
        success: row.get::<_, i64>(11)? != 0,
        // A footprint that cannot be decoded is treated as absent: better to
        // omit I/O from a comparison than to fail the whole read.
        io: row
            .get::<_, Option<String>>(12)?
            .and_then(|raw| serde_json::from_str(&raw).ok()),
    })
}

/// Carry a cached estimate forward to the current schema version.
///
/// * `version < CACHE_SCHEMA_VERSION`: entries from older schemas are
///   migrated forward one step at a time (see [`migrate_one_step`]) until
///   they match the current schema.
/// * `version == CACHE_SCHEMA_VERSION`: returned unchanged.
/// * `version > CACHE_SCHEMA_VERSION`: an entry written by a *newer* tool.
///   It cannot be safely read (or silently downgraded), so this returns an
///   error naming the entry's version and telling the user to upgrade.
///
/// # Network calls
/// None — pure transformation.
pub fn migrate_to_latest(cached: CachedEstimate) -> AppResult<CachedEstimate> {
    let mut migrated = cached;

    if migrated.version > CACHE_SCHEMA_VERSION {
        return Err(AppError::General(format!(
            "cache entry schema v{} is newer than supported v{CACHE_SCHEMA_VERSION}; \
             upgrade soroban-cost-estimator to read this entry, or run `cache clear` to discard it",
            migrated.version
        )));
    }

    // Unversioned legacy entries (schema 0) are treated as the initial
    // schema before the step-wise migration runs.
    if migrated.version < INITIAL_SCHEMA_VERSION {
        migrated.version = INITIAL_SCHEMA_VERSION;
    }

    while migrated.version < CACHE_SCHEMA_VERSION {
        migrated = migrate_one_step(migrated);
    }

    Ok(migrated)
}

/// Apply the single schema transition from `migrated.version` to the next
/// version. Each schema bump appends one arm here.
fn migrate_one_step(migrated: CachedEstimate) -> CachedEstimate {
    match migrated.version {
        // v1 → v2: `duration_ms` and `success` were added. Older entries
        // predate both, so they get conservative defaults (unknown duration,
        // and a successful simulation — the only outcome that was cached).
        v if v < DURATION_SCHEMA_VERSION => CachedEstimate {
            duration_ms: None,
            success: true,
            version: DURATION_SCHEMA_VERSION,
            ..migrated
        },
        // v2 → v3: `last_accessed` was added purely for LRU eviction. It is
        // not part of `CachedEstimate`, so no field transformation is needed
        // (the column defaults to `timestamp` semantics for old rows).
        v if v < ACCESS_TRACKING_SCHEMA_VERSION => CachedEstimate {
            version: ACCESS_TRACKING_SCHEMA_VERSION,
            ..migrated
        },
        _ => migrated,
    }
}

/// Record that an entry was just read, for LRU eviction ordering.
///
/// Best-effort by design: a failed touch (e.g. the database is locked by
/// another process) must never fail a cache read. The row simply keeps its
/// previous access time and becomes a slightly earlier eviction candidate.
fn touch_last_accessed(conn: &Connection, wasm_hash: &str, function: &str, args_hash: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        "UPDATE estimates SET last_accessed = ?1 \
         WHERE wasm_hash = ?2 AND function = ?3 AND args_hash = ?4",
        rusqlite::params![now, wasm_hash, function, args_hash],
    );
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
    let cached = {
        let mut stmt = conn.prepare(
            "SELECT version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
             cpu_instructions, memory_bytes, timestamp, duration_ms, success, io_json \
             FROM estimates WHERE wasm_hash = ?1 AND function = ?2 AND args_hash = ?3",
        )?;
        let mut rows = stmt.query(rusqlite::params![wasm_hash, function, args_hash.as_str()])?;
        match rows.next()? {
            None => None,
            Some(row) => Some(migrate_to_latest(estimate_from_row(row)?)?),
        }
    };

    if cached.is_some() {
        // Mark the hit as most-recently-used so LRU eviction protects it.
        touch_last_accessed(&conn, wasm_hash, function, &args_hash);
    }

    Ok(cached)
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
         cpu_instructions, memory_bytes, timestamp, duration_ms, success, io_json \
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

/// Parse a timestamp from a cached estimate entry.
fn parse_entry_timestamp(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(naive_dt.and_utc());
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive_dt.and_utc());
    }
    None
}

/// Parse an ISO-8601 / RFC3339 timestamp or YYYY-MM-DD date into a UTC `DateTime`.
pub fn parse_since_timestamp(s: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive_dt.and_utc());
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(naive_dt.and_utc());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_dt = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| AppError::General(format!("invalid date {s:?}")))?;
        return Ok(naive_dt.and_utc());
    }
    Err(AppError::General(format!(
        "invalid timestamp or date {s:?}: expected RFC3339 (e.g. 2026-01-01T00:00:00Z) or YYYY-MM-DD (e.g. 2026-01-01)"
    )))
}

/// Parse an ISO-8601 / RFC3339 timestamp or YYYY-MM-DD date into a UTC `DateTime` for upper bound.
pub fn parse_to_timestamp(s: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(naive_dt.and_utc());
    }
    if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(naive_dt.and_utc());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive_dt = date
            .and_hms_opt(23, 59, 59)
            .ok_or_else(|| AppError::General(format!("invalid date {s:?}")))?;
        return Ok(naive_dt.and_utc());
    }
    Err(AppError::General(format!(
        "invalid timestamp or date {s:?}: expected RFC3339 (e.g. 2026-01-01T23:59:59Z) or YYYY-MM-DD (e.g. 2026-01-01)"
    )))
}

/// Find all cached estimates across all networks, ordered newest-first.
pub fn list_all_cached_estimates() -> AppResult<Vec<CachedEstimate>> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
         cpu_instructions, memory_bytes, timestamp, duration_ms, success, io_json \
         FROM estimates ORDER BY timestamp DESC",
    )?;

    let rows = stmt.query_map([], estimate_from_row)?;

    let mut estimates = Vec::new();
    for row in rows {
        let cached = row?;
        if let Ok(cached) = migrate_to_latest(cached) {
            estimates.push(cached);
        }
    }

    trace!(count = estimates.len(), "listed all cached estimates");
    Ok(estimates)
}

/// Query cached estimates applying the optional filters in [`CacheFilter`].
///
/// If no filters are provided, returns all cached estimates.
/// Results are returned newest-first (by `timestamp`).
///
/// # Network calls
/// None — pure local SQLite I/O.
pub fn query_cache(filter: &CacheFilter) -> AppResult<Vec<CachedEstimate>> {
    filter.validate()?;

    let mut estimates = match &filter.network {
        Some(net) => list_cached_estimates(net)?,
        None => list_all_cached_estimates()?,
    };

    // Newest-first ordering.
    estimates.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let filtered: Vec<CachedEstimate> = estimates
        .into_iter()
        .filter(|e| filter.matches(e))
        .collect();

    trace!(count = filtered.len(), "queried cached estimates");
    Ok(filtered)
}

/// Query cached estimates for `network`, applying the optional filters in
/// [`QueryFilter`].
///
/// # Network calls
/// None — pure file I/O.
pub fn query_estimates(network: &str, filter: &QueryFilter) -> AppResult<Vec<CachedEstimate>> {
    let mut cache_filter = CacheFilter::from(filter.clone());
    cache_filter.network = Some(network.to_string());
    query_cache(&cache_filter)
}

/// Export every cached estimate as a deterministic, JSON-serializable list.
///
/// All rows are read from the SQLite cache, migrated to the current schema,
/// and sorted by (wasm_hash, function, args_hash) so repeated exports are
/// stable. A malformed or unsupported entry returns an error rather than
/// producing an incomplete backup.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn export_cached_estimates() -> AppResult<Vec<CachedEstimate>> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT version, wasm_hash, function, args_hash, network, ledger, total_stroops, \
         cpu_instructions, memory_bytes, timestamp, duration_ms, success, io_json \
         FROM estimates ORDER BY wasm_hash, function, args_hash",
    )?;

    let rows = stmt.query_map([], estimate_from_row)?;

    let mut estimates = Vec::new();
    for row in rows {
        let cached = row?;
        estimates.push(migrate_to_latest(cached)?);
    }

    debug!(count = estimates.len(), "exported cached estimates");
    Ok(estimates)
}

/// Schema version of the `cache export` file format.
///
/// Independent from [`CACHE_SCHEMA_VERSION`] (which versions individual
/// cache *entries*): this versions the *export envelope* so backups stay
/// readable as the envelope gains fields.
pub const CACHE_EXPORT_SCHEMA_VERSION: u32 = 1;

/// A portable, self-describing dump of cached estimates.
///
/// Written by `cache export` for backup, sharing across workstations, and
/// archiving. The envelope carries everything needed to interpret the
/// records without the exporting tool's help.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheExport {
    /// Version of the export envelope format ([`CACHE_EXPORT_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// RFC-3339 timestamp of when the export was created.
    pub exported_at: String,
    /// Network the export was filtered to, or `None` for all networks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// The exported estimates in deterministic
    /// `(wasm_hash, function, args_hash)` order.
    pub estimates: Vec<CachedEstimate>,
}

/// Build a versioned [`CacheExport`] of cached estimates.
///
/// * `network` - Only include estimates recorded for this network, or `None`
///   for every network. Unknown networks simply yield an empty list.
///
/// Like [`export_cached_estimates`], a malformed or unsupported entry
/// returns an error rather than producing an incomplete backup.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn export_cache(network: Option<&str>) -> AppResult<CacheExport> {
    let estimates: Vec<CachedEstimate> = export_cached_estimates()?
        .into_iter()
        .filter(|e| match network {
            Some(n) => e.network == n,
            None => true,
        })
        .collect();

    debug!(
        count = estimates.len(),
        network, "built cache export envelope"
    );
    Ok(CacheExport {
        schema_version: CACHE_EXPORT_SCHEMA_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        network: network.map(str::to_string),
        estimates,
    })
}

/// Integrity status of a single cache entry file.
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
            duration_ms: None,
            success: true,
            io: None,
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

/// Aggregate statistics for the estimate cache.
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    /// Total number of cached estimates across all networks.
    pub total_entries: usize,
    /// Disk usage of the SQLite database file in bytes.
    pub disk_bytes: u64,
    /// Live (non-free) database size in bytes — the value compared against
    /// the `--max-cache-size-mb` quota for LRU eviction. Always `<=`
    /// `disk_bytes`.
    pub live_bytes: u64,
    /// Timestamp of the oldest entry (ISO-8601), if any.
    pub oldest_entry: Option<String>,
    /// Timestamp of the newest entry (ISO-8601), if any.
    pub newest_entry: Option<String>,
    /// Per-network breakdown: (network, count).
    pub per_network: Vec<(String, usize)>,
}

/// Compute aggregate cache statistics.
///
/// Reads the SQLite database metadata (file size) and queries the
/// `estimates` table for total count, timestamp bounds, and a
/// `GROUP BY network` breakdown.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn cache_stats() -> AppResult<CacheStats> {
    let path = db_path()?;
    let disk_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let conn = open_db()?;

    // Live database size, as measured by the eviction pass.
    let live_bytes = live_db_bytes(&conn)?;

    // Total count.
    let total_entries: usize =
        conn.query_row("SELECT COUNT(*) FROM estimates", [], |row| row.get(0))?;

    // Oldest and newest timestamps.
    let oldest_entry: Option<String> = conn
        .query_row("SELECT MIN(timestamp) FROM estimates", [], |row| row.get(0))
        .ok();
    let newest_entry: Option<String> = conn
        .query_row("SELECT MAX(timestamp) FROM estimates", [], |row| row.get(0))
        .ok();

    // Per-network breakdown.
    let mut stmt = conn.prepare(
        "SELECT network, COUNT(*) FROM estimates GROUP BY network ORDER BY COUNT(*) DESC",
    )?;
    let per_network: Vec<(String, usize)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    debug!(
        total_entries,
        disk_bytes, live_bytes, "cache stats computed"
    );
    Ok(CacheStats {
        total_entries,
        disk_bytes,
        live_bytes,
        oldest_entry,
        newest_entry,
        per_network,
    })
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

/// Delete every cached estimate recorded for the given network.
///
/// Returns the number of rows deleted. Entries for other networks are left
/// untouched. This is the single shared implementation behind both the
/// `cache clear` subcommand and the `estimate --clear-cache` flag, so the
/// two paths behave identically.
///
/// # Network calls
/// None — pure SQLite I/O.
pub fn clear_cache(network: &str) -> AppResult<usize> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::General(format!("cache write lock poisoned: {e}")))?;
    let conn = open_db()?;
    let removed =
        execute_with_retry(|| conn.execute("DELETE FROM estimates WHERE network = ?1", [network]))?;
    debug!(network, removed, "cache cleared");
    Ok(removed)
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
