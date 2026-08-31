use thiserror::Error;

/// Unified error type for all fallible operations in soroban-cost-estimator.
///
/// Every RPC call, XDR decode, file I/O, and WASM parse returns a `Result`
/// through this enum. No `unwrap()` or `expect()` is permitted outside tests.
#[derive(Error, Debug)]
pub enum AppError {
    // ── I/O ─────────────────────────────────────────────────────────
    #[error("failed to perform I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    // ── RPC ─────────────────────────────────────────────────────────
    #[error("failed to execute RPC: status {status} - {message}")]
    Rpc { status: i64, message: String },

    #[error("failed to send HTTP request: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to locate RPC endpoint: not configured for network {0}")]
    UnknownNetwork(String),

    // ── XDR ─────────────────────────────────────────────────────────
    #[error("failed to decode XDR: {0}")]
    XdrDecode(String),

    #[error("failed to encode XDR: {0}")]
    XdrEncode(String),

    // ── WASM ────────────────────────────────────────────────────────
    #[error("failed to parse WASM: {0}")]
    WasmParse(String),

    #[error("failed to validate WASM: {0}")]
    WasmValidation(String),

    #[error("argument type validation error: {0}")]
    TypeValidation(String),

    // ── Config Snapshot ─────────────────────────────────────────────
    #[error("failed to load snapshot: not found at {0}")]
    SnapshotNotFound(String),

    #[error("failed to parse snapshot: {0}")]
    SnapshotParse(String),

    #[error("failed to load snapshots: none available for network {0}")]
    NoSnapshots(String),

    // ── Simulation ──────────────────────────────────────────────────
    #[error("failed to simulate transaction: {0}")]
    SimulationFailed(String),

    #[error("failed to construct transaction: {0}")]
    TxConstruction(String),

    // ── Report ──────────────────────────────────────────────────────
    #[error("failed to calculate fee: {0}")]
    FeeCalc(String),

    // ── Config ──────────────────────────────────────────────────────
    #[error("failed to fetch config: {0}")]
    ConfigFetch(String),

    #[error("failed to retrieve config setting: {0} not found")]
    ConfigSettingNotFound(String),

    // ── Serialization ──────────────────────────────────────────────
    #[error("failed to process JSON: {0}")]
    Json(#[from] serde_json::Error),

    // ── Cache database ─────────────────────────────────────────────
    #[error("failed to access cache database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    // ── General ─────────────────────────────────────────────────────
    #[error("operation failed: {0}")]
    General(String),
}

/// Convenience alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;
