use thiserror::Error;

/// Unified error type for all fallible operations in soroban-cost-estimator.
///
/// Every RPC call, XDR decode, file I/O, and WASM parse returns a `Result`
/// through this enum. No `unwrap()` or `expect()` is permitted outside tests.
#[derive(Error, Debug)]
pub enum AppError {
    // ── I/O ─────────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    FileNotFound(String),

    // ── RPC ─────────────────────────────────────────────────────────
    #[error("RPC error (status {status}): {message}")]
    Rpc { status: i64, message: String },

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("RPC endpoint not configured for network: {0}")]
    UnknownNetwork(String),

    // ── XDR ─────────────────────────────────────────────────────────
    #[error("XDR decode error: {0}")]
    XdrDecode(String),

    #[error("XDR encode error: {0}")]
    XdrEncode(String),

    // ── WASM ────────────────────────────────────────────────────────
    #[error("WASM parse error: {0}")]
    WasmParse(String),

    #[error("WASM validation error: {0}")]
    WasmValidation(String),

    // ── Config Snapshot ─────────────────────────────────────────────
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("Snapshot parse error: {0}")]
    SnapshotParse(String),

    #[error("No snapshots available for network: {0}")]
    NoSnapshots(String),

    // ── Simulation ──────────────────────────────────────────────────
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),

    #[error("Transaction construction error: {0}")]
    TxConstruction(String),

    // ── Report ──────────────────────────────────────────────────────
    #[error("Fee calculation error: {0}")]
    FeeCalc(String),

    // ── Config ──────────────────────────────────────────────────────
    #[error("Config fetch error: {0}")]
    ConfigFetch(String),

    #[error("Config setting not found: {0}")]
    ConfigSettingNotFound(String),

    // ── Serialization ──────────────────────────────────────────────
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ── General ─────────────────────────────────────────────────────
    #[error("{0}")]
    General(String),
}

/// Convenience alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;
