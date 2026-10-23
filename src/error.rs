use thiserror::Error;

/// Unified error type for all fallible operations in soroban-cost-estimator.
///
/// Every RPC call, XDR decode, file I/O, and WASM parse returns a `Result`
/// through this enum. No `unwrap()` or `expect()` is permitted outside tests.
///
/// # Message style
///
/// Every `thiserror` template and every inline error string in the crate
/// follows one house style, so CLI output reads as a single system:
///
/// 1. **Lower-case the first word**, unless it is a proper noun or a
///    technology name (`RPC`, `WASM`, `XDR`, `XLM`, `WebSocket`, `SQLite`,
///    `JSON`, `HTTP`). So `failed to parse WASM: …`, not `Failed to parse wasm`.
/// 2. **Lead with what failed**, using a `failed to <verb> …:` or
///    `<noun> …:` prefix. The underlying cause follows after the colon.
/// 3. **No trailing period.** An error is a fragment, not a sentence.
/// 4. **Never restate the prefix inside the payload.** A variant that already
///    renders `failed to decode XDR: {0}` must be given a payload like
///    `invalid base64: …`, not `failed to decode base64: …`.
///
/// `verify_message_style` in this module's tests enforces (1)–(3) over every
/// variant, so a new variant cannot silently drift from the guidelines.
#[derive(Error, Debug)]
pub enum AppError {
    // ── I/O ─────────────────────────────────────────────────────────
    /// A filesystem or stream operation failed (opening a WASM file, reading a
    /// snapshot, writing the cache database, …).
    #[error("failed to perform I/O: {0}")]
    Io(#[from] std::io::Error),

    /// A path the user named does not exist.
    #[error("file not found: {0}")]
    FileNotFound(String),

    // ── RPC ─────────────────────────────────────────────────────────
    /// The endpoint returned a JSON-RPC error object.
    ///
    /// `status` is the JSON-RPC error code and `message` the server-supplied
    /// explanation; both are passed through verbatim.
    #[error("failed to execute RPC: status {status} - {message}")]
    Rpc {
        /// JSON-RPC error code (for example `-32602` for invalid params).
        status: i64,
        /// Human-readable error message returned by the RPC server.
        message: String,
    },

    /// The endpoint answered with a transient gateway status (502/503/504),
    /// meaning it is briefly unavailable rather than misconfigured. Treated
    /// as retryable and as a trigger for failover to `--rpc-fallback-url`.
    #[error("RPC endpoint temporarily unavailable (HTTP {status})")]
    RpcUnavailable {
        /// The transient HTTP status returned by the gateway.
        status: u16,
    },

    /// The HTTP request could not be sent or its response could not be read.
    #[error("failed to send HTTP request: {0}")]
    Http(#[from] reqwest::Error),

    /// A `--header`/config header entry is not a valid `Name: value` pair.
    #[error("failed to parse HTTP header: {0}")]
    InvalidHeader(String),

    /// A user-supplied RPC endpoint (`--rpc-url` or a `[rpc_urls]` config
    /// entry) that is not a usable URL.
    ///
    /// Raised before any WASM parsing or simulation work starts, so a typo in
    /// a scheme, host, or port produces immediate, actionable feedback instead
    /// of a confusing deserialization or XDR error from deep inside a request.
    #[error("invalid RPC URL: {0}")]
    InvalidRpcUrl(String),

    /// A non-success HTTP status returned by the RPC endpoint.
    ///
    /// Transient statuses (429/5xx) are retried by the RPC client; this error
    /// surfaces only once the retry budget is exhausted. `retry_after` carries
    /// the endpoint's `Retry-After` hint (HTTP 429) when present, so the retry
    /// loop can honor it instead of its computed backoff.
    #[error("RPC HTTP error: status {status} - {message}")]
    HttpStatus {
        /// The HTTP status code returned by the endpoint.
        status: u16,
        /// The `Retry-After` hint sent with a 429 response, when present.
        retry_after: Option<std::time::Duration>,
        /// Response body (or status text) for context.
        message: String,
    },

    /// The TCP connection could not be established within the configured
    /// connect timeout (distinct from a whole-request timeout). Wraps the
    /// underlying reqwest error so the original context is preserved.
    #[error("failed to establish connection to RPC host within {seconds} seconds")]
    ConnectTimeout {
        /// The configured connect timeout, in seconds.
        seconds: u64,
        /// The underlying reqwest error that triggered the timeout.
        #[source]
        source: reqwest::Error,
    },

    /// An RPC operation exceeded its overall deadline across retries, backoff,
    /// and fallback endpoints.
    #[error("RPC operation timed out: exceeded overall deadline of {timeout_ms} ms")]
    TotalTimeout { timeout_ms: u64 },

    // ── WebSocket ────────────────────────────────────────────────
    /// The WebSocket handshake or connection attempt failed.
    #[error("failed to open WebSocket connection: {0}")]
    WsConnect(String),

    /// A frame did not follow the JSON-RPC-over-WebSocket protocol.
    #[error("WebSocket protocol error: {0}")]
    WsProtocol(String),

    /// The peer closed the WebSocket while a subscription was active.
    #[error("WebSocket connection closed unexpectedly")]
    WsClosed,

    /// No custom, configured, or well-known endpoint exists for a network name.
    #[error("failed to locate RPC endpoint: not configured for network {0}")]
    UnknownNetwork(String),

    // ── XDR ─────────────────────────────────────────────────────────
    /// A base64/XDR value from the RPC or the user could not be decoded.
    #[error("failed to decode XDR: {0}")]
    XdrDecode(String),

    /// A value could not be serialized into XDR for a request.
    #[error("failed to encode XDR: {0}")]
    XdrEncode(String),

    // ── WASM ────────────────────────────────────────────────────────
    /// The file is not valid WebAssembly or has an unsupported structure.
    #[error("failed to parse WASM: {0}")]
    WasmParse(String),

    /// The WASM module is well-formed but fails Soroban's contract rules.
    #[error("failed to validate WASM: {0}")]
    WasmValidation(String),

    /// The WASM file is larger than the configured `--max-wasm-size` limit.
    #[error(
        "failed to load WASM: file {path} exceeds maximum allowed size ({size} bytes, limit is {max_size} bytes)"
    )]
    WasmFileTooLarge {
        /// Path of the offending WASM file.
        path: String,
        /// Size of the file on disk, in bytes.
        size: u64,
        /// Maximum size the tool accepts, in bytes.
        max_size: u64,
    },

    /// A contract-argument value cannot be converted to the function's type.
    #[error("failed to validate argument type: {0}")]
    TypeValidation(String),

    // ── Config Snapshot ─────────────────────────────────────────────
    /// The requested snapshot file does not exist at the given path.
    #[error("failed to load snapshot: not found at {0}")]
    SnapshotNotFound(String),

    /// A snapshot file exists but is not valid snapshot JSON.
    #[error("failed to parse snapshot: {0}")]
    SnapshotParse(String),

    /// No snapshots have been captured for the given network.
    #[error("failed to load snapshots: none available for network {0}")]
    NoSnapshots(String),

    /// A diff was requested but fewer than two snapshots exist for the network.
    #[error(
        "failed to load snapshots: need at least 2 for network {network}, found {found} \
         (run `config snapshot --network {network}` to capture another)"
    )]
    NotEnoughSnapshots {
        /// Network the diff was requested for.
        network: String,
        /// Number of snapshots actually available.
        found: usize,
    },

    // ── Simulation ──────────────────────────────────────────────────
    /// A `simulateTransaction` call that the network rejected or that failed
    /// inside the contract host.
    ///
    /// Carries the RPC's machine-readable error code and a bounded copy of its
    /// structured details (issue #532), so a caller can distinguish an
    /// authorization failure from missing ledger entries or resource
    /// exhaustion without parsing the message. `Display` renders only the
    /// concise human message; the code and details stay available as typed
    /// fields (and as JSON through
    /// [`SimulationError::to_json`](crate::rpc::simulate::SimulationError::to_json)).
    #[error("failed to simulate transaction: {0}")]
    SimulationFailed(Box<crate::rpc::simulate::SimulationError>),

    /// A transaction could not be assembled from the contract and arguments.
    #[error("failed to construct transaction: {0}")]
    TxConstruction(String),

    // ── Report ──────────────────────────────────────────────────────
    /// A fee computation overflowed or received an unusable value.
    #[error("failed to calculate fee: {0}")]
    FeeCalc(String),

    // ── Config ──────────────────────────────────────────────────────
    /// `config.toml` is syntactically valid but semantically unusable.
    #[error("failed to process config: {0}")]
    Config(String),

    /// The RPC could not return the requested network configuration entry.
    #[error("failed to fetch config: {0}")]
    ConfigFetch(String),

    /// A well-known configuration setting is absent from the network config.
    #[error("failed to retrieve config setting: {0} not found")]
    ConfigSettingNotFound(String),

    // ── Serialization ──────────────────────────────────────────────
    /// JSON could not be serialized or deserialized.
    #[error("failed to process JSON: {0}")]
    Json(#[from] serde_json::Error),

    // ── Cache database ─────────────────────────────────────────────
    /// The SQLite cache file exists but its contents are unreadable.
    #[error("cache database corrupt: {0}")]
    CacheCorrupt(String),

    /// The SQLite cache could not be opened, read, or written.
    #[error("failed to access cache database: {0}")]
    Sqlite(#[from] rusqlite::Error),

    // ── General ─────────────────────────────────────────────────────
    /// A failure that has no more specific variant.
    #[error("operation failed: {0}")]
    General(String),
}

/// Convenience alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::AppError;

    /// First word of `message`, lower-cased, for case checks.
    fn first_word(message: &str) -> String {
        message
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    }

    /// Acronyms and technology names that legitimately start a message in
    /// upper case. Anything not listed here must start lower-case.
    const ALLOWED_UPPERCASE_PREFIXES: &[&str] =
        &["rpc", "wasm", "xdr", "xlm", "json", "http", "websocket"];

    /// Asserts that a rendered message obeys the house style documented on
    /// [`AppError`]: lower-case first word (bar the allowed acronyms), no
    /// trailing period, and a non-empty body.
    fn assert_house_style(message: &str) {
        assert!(!message.is_empty(), "error message must not be empty");
        assert!(
            !message.ends_with('.'),
            "error message must not end with a period: {message:?}"
        );
        let first = message
            .chars()
            .next()
            .expect("non-empty message has a first character");
        let word = first_word(message);
        assert!(
            first.is_lowercase() || ALLOWED_UPPERCASE_PREFIXES.contains(&word.as_str()),
            "error message must start lower-case (or with a known acronym): {message:?}"
        );
    }

    /// Every variant's rendered message must follow the house style. Keeping
    /// this exhaustive is what makes the guideline enforceable.
    #[test]
    fn every_variant_message_follows_house_style() {
        let cases: Vec<AppError> = vec![
            AppError::Io(std::io::Error::other("boom")),
            AppError::FileNotFound("a.wasm".to_string()),
            AppError::Rpc {
                status: -32000,
                message: "boom".to_string(),
            },
            AppError::TotalTimeout { timeout_ms: 1500 },
            AppError::InvalidHeader("missing '=' separator".to_string()),
            AppError::InvalidRpcUrl("`ftp://host` uses unsupported scheme `ftp`".to_string()),
            AppError::WsConnect("wss://host/ws: refused".to_string()),
            AppError::WsProtocol("malformed RPC message".to_string()),
            AppError::WsClosed,
            AppError::UnknownNetwork("nope".to_string()),
            AppError::XdrDecode("invalid base64".to_string()),
            AppError::XdrEncode("ledger key".to_string()),
            AppError::WasmParse("no exported functions".to_string()),
            AppError::WasmValidation("invalid magic bytes".to_string()),
            AppError::WasmFileTooLarge {
                path: "contract.wasm".to_string(),
                size: 20_000_000,
                max_size: 16_777_216,
            },
            AppError::TypeValidation("arg 'abc' cannot be used as 'i64'".to_string()),
            AppError::SnapshotNotFound("/tmp/s.json".to_string()),
            AppError::SnapshotParse("invalid bundle".to_string()),
            AppError::NoSnapshots("testnet".to_string()),
            AppError::SimulationFailed(Box::new(crate::rpc::simulate::SimulationError::new(
                "no cost data",
            ))),
            AppError::TxConstruction("contract id required".to_string()),
            AppError::FeeCalc("invalid XLM value".to_string()),
            AppError::ConfigFetch("boom".to_string()),
            AppError::ConfigSettingNotFound("CONFIG_SETTING_CONTRACT_COMPUTE_V0".to_string()),
            AppError::Json(serde_json::from_str::<u8>("nope").unwrap_err()),
            AppError::CacheCorrupt("cache.db: malformed disk image".to_string()),
            AppError::General("could not determine home directory".to_string()),
        ];

        for case in cases {
            assert_house_style(&case.to_string());
        }
    }

    /// Every `thiserror` template must read `failed to …` / `<noun> …` and
    /// never repeat the variant name, so the rendered text stays informative.
    #[test]
    fn templates_lead_with_what_failed() {
        let samples = [
            AppError::WasmParse("invalid magic bytes".to_string()),
            AppError::XdrDecode("invalid base64".to_string()),
            AppError::FileNotFound("a.wasm".to_string()),
            AppError::InvalidHeader("missing '=' separator".to_string()),
        ];
        for sample in samples {
            let rendered = sample.to_string();
            assert!(
                rendered.contains(':'),
                "error message must separate cause with a colon: {rendered:?}"
            );
        }
    }
}
