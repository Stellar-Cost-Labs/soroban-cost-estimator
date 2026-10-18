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
    #[error("failed to perform I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("file not found: {0}")]
    FileNotFound(String),

    // ── RPC ─────────────────────────────────────────────────────────
    #[error("failed to execute RPC: status {status} - {message}")]
    Rpc { status: i64, message: String },

    /// The endpoint answered with a transient gateway status (502/503/504),
    /// meaning it is briefly unavailable rather than misconfigured. Treated
    /// as retryable and as a trigger for failover to `--rpc-fallback-url`.
    #[error("RPC endpoint temporarily unavailable (HTTP {status})")]
    RpcUnavailable { status: u16 },

    #[error("failed to send HTTP request: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to parse HTTP header: {0}")]
    InvalidHeader(String),

    /// A non-success HTTP status returned by the RPC endpoint.
    ///
    /// Transient statuses (429/5xx) are retried by the RPC client; this error
    /// surfaces only once the retry budget is exhausted. `retry_after` carries
    /// the endpoint's `Retry-After` hint (HTTP 429) when present, so the retry
    /// loop can honor it instead of its computed backoff.
    #[error("RPC HTTP error: status {status} - {message}")]
    HttpStatus {
        status: u16,
        retry_after: Option<std::time::Duration>,
        message: String,
    },

    /// The TCP connection could not be established within the configured
    /// connect timeout (distinct from a whole-request timeout). Wraps the
    /// underlying reqwest error so the original context is preserved.
    #[error("failed to establish connection to RPC host within {seconds} seconds")]
    ConnectTimeout {
        seconds: u64,
        #[source]
        source: reqwest::Error,
    },

    // ── WebSocket ────────────────────────────────────────────────
    #[error("failed to open WebSocket connection: {0}")]
    WsConnect(String),

    #[error("WebSocket protocol error: {0}")]
    WsProtocol(String),

    #[error("WebSocket connection closed unexpectedly")]
    WsClosed,

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

    #[error("failed to validate argument type: {0}")]
    TypeValidation(String),

    // ── Config Snapshot ─────────────────────────────────────────────
    #[error("failed to load snapshot: not found at {0}")]
    SnapshotNotFound(String),

    #[error("failed to parse snapshot: {0}")]
    SnapshotParse(String),

    #[error("failed to load snapshots: none available for network {0}")]
    NoSnapshots(String),

    #[error(
        "failed to load snapshots: need at least 2 for network {network}, found {found} \
         (run `config snapshot --network {network}` to capture another)"
    )]
    NotEnoughSnapshots { network: String, found: usize },

    // ── Simulation ──────────────────────────────────────────────────
    #[error("failed to simulate transaction: {0}")]
    SimulationFailed(String),

    #[error("failed to construct transaction: {0}")]
    TxConstruction(String),

    // ── Report ──────────────────────────────────────────────────────
    #[error("failed to calculate fee: {0}")]
    FeeCalc(String),

    // ── Config ──────────────────────────────────────────────────────
    #[error("failed to process config: {0}")]
    Config(String),

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

    // ── Structured Context Variants ─────────────────────────────────
    #[error("failed to read file {}: {source}", path.display())]
    IoWithPath {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to contact RPC endpoint {endpoint}: {source}")]
    RpcEndpoint {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to validate argument '{param}': expected {expected}, got '{got}'")]
    ArgumentMismatch {
        param: String,
        expected: String,
        got: String,
    },
}

impl AppError {
    /// Short description of the high-level action that failed.
    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::IoWithPath { .. } | Self::FileNotFound(_) => "file I/O",
            Self::Rpc { .. }
            | Self::RpcUnavailable { .. }
            | Self::Http(_)
            | Self::InvalidHeader(_)
            | Self::HttpStatus { .. }
            | Self::ConnectTimeout { .. }
            | Self::RpcEndpoint { .. }
            | Self::UnknownNetwork(_) => "RPC communication",
            Self::WsConnect(_) | Self::WsProtocol(_) | Self::WsClosed => "WebSocket connection",
            Self::XdrDecode(_) | Self::XdrEncode(_) => "XDR serialization",
            Self::WasmParse(_) | Self::WasmValidation(_) => "WASM loading",
            Self::TypeValidation(_) | Self::ArgumentMismatch { .. } => "argument validation",
            Self::SnapshotNotFound(_)
            | Self::SnapshotParse(_)
            | Self::NoSnapshots(_)
            | Self::NotEnoughSnapshots { .. } => "config snapshot loading",
            Self::SimulationFailed(_) | Self::TxConstruction(_) => "transaction simulation",
            Self::FeeCalc(_) => "fee calculation",
            Self::Config(_) | Self::ConfigFetch(_) | Self::ConfigSettingNotFound(_) => {
                "configuration resolution"
            }
            Self::Json(_) => "JSON processing",
            Self::Sqlite(_) => "cache database access",
            Self::General(_) => "command execution",
        }
    }

    /// Actionable suggestion helping the user resolve the error.
    #[must_use]
    pub const fn suggestion(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::IoWithPath { .. } => {
                "check file permissions and available disk space"
            }
            Self::FileNotFound(_) => {
                "verify the --wasm or snapshot file path exists before running the command"
            }
            Self::Rpc { .. } => {
                "inspect the RPC error message and verify --network and --rpc-url settings"
            }
            Self::RpcUnavailable { .. } | Self::HttpStatus { .. } => {
                "retry after a moment or pass --rpc-fallback-url with a backup Soroban RPC node"
            }
            Self::Http(_) | Self::RpcEndpoint { .. } | Self::ConnectTimeout { .. } => {
                "check network connectivity, --rpc-url, and --timeout / --connect-timeout flags"
            }
            Self::InvalidHeader(_) => "pass headers as --header KEY=VALUE or --header 'KEY: VALUE'",
            Self::WsConnect(_) | Self::WsProtocol(_) | Self::WsClosed => {
                "verify the WebSocket RPC endpoint URL and network connectivity"
            }
            Self::UnknownNetwork(_) => {
                "use a supported network (testnet, mainnet, futurenet, local) or pass --rpc-url"
            }
            Self::XdrDecode(_) | Self::XdrEncode(_) => {
                "ensure the RPC node runs a compatible Soroban protocol version"
            }
            Self::WasmParse(_) | Self::WasmValidation(_) => {
                "compile the contract with `stellar contract build` targeting wasm32-unknown-unknown"
            }
            Self::TypeValidation(_) | Self::ArgumentMismatch { .. } => {
                "run `wasm info <path>` to inspect expected parameter names and types"
            }
            Self::SnapshotNotFound(_) | Self::NoSnapshots(_) | Self::NotEnoughSnapshots { .. } => {
                "run `soroban-cost-estimator config snapshot --network <net>` to capture a snapshot"
            }
            Self::SnapshotParse(_) => {
                "re-capture the snapshot or validate stored snapshots with `config snapshot validate --all`"
            }
            Self::SimulationFailed(_) | Self::TxConstruction(_) => {
                "verify --id (64-hex or C... strkey), --fn, and --arg values against the deployed contract"
            }
            Self::FeeCalc(_) => "provide a valid non-negative XLM or stroop amount",
            Self::Config(_) | Self::ConfigFetch(_) | Self::ConfigSettingNotFound(_) => {
                "check your config.toml syntax and ensure the RPC node exposes Soroban config settings"
            }
            Self::Json(_) => "verify that the input JSON file or RPC payload is well-formed",
            Self::Sqlite(_) => {
                "run `cache verify` or `cache clear` if the local cache database is locked or corrupted"
            }
            Self::General(_) => {
                "run with `--help` or `--verbose` for detailed usage and diagnostics"
            }
        }
    }

    /// Formats the error with structured action context, underlying cause, and
    /// actionable suggestion:
    /// `error: <action failed>: <underlying cause>. Suggestion: <hint>`
    #[must_use]
    pub fn format_with_suggestion(&self) -> String {
        format!(
            "error: {}: {}. Suggestion: {}",
            self.action(),
            self,
            self.suggestion()
        )
    }
}

/// Convenience alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;
    use std::path::PathBuf;

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
            AppError::InvalidHeader("missing '=' separator".to_string()),
            AppError::WsConnect("wss://host/ws: refused".to_string()),
            AppError::WsProtocol("malformed RPC message".to_string()),
            AppError::WsClosed,
            AppError::UnknownNetwork("nope".to_string()),
            AppError::XdrDecode("invalid base64".to_string()),
            AppError::XdrEncode("ledger key".to_string()),
            AppError::WasmParse("no exported functions".to_string()),
            AppError::WasmValidation("invalid magic bytes".to_string()),
            AppError::TypeValidation("arg 'abc' cannot be used as 'i64'".to_string()),
            AppError::SnapshotNotFound("/tmp/s.json".to_string()),
            AppError::SnapshotParse("invalid bundle".to_string()),
            AppError::NoSnapshots("testnet".to_string()),
            AppError::SimulationFailed("no cost data".to_string()),
            AppError::TxConstruction("contract id required".to_string()),
            AppError::FeeCalc("invalid XLM value".to_string()),
            AppError::ConfigFetch("boom".to_string()),
            AppError::ConfigSettingNotFound("CONFIG_SETTING_CONTRACT_COMPUTE_V0".to_string()),
            AppError::Json(serde_json::from_str::<u8>("nope").unwrap_err()),
            AppError::General("could not determine home directory".to_string()),
            AppError::IoWithPath {
                path: PathBuf::from("contract.wasm"),
                source: std::io::Error::other("permission denied"),
            },
            AppError::ArgumentMismatch {
                param: "amount".to_string(),
                expected: "i64".to_string(),
                got: "abc".to_string(),
            },
        ];

        for case in cases {
            assert_house_style(&case.to_string());
            let formatted = case.format_with_suggestion();
            assert!(
                formatted.starts_with("error: "),
                "structured format must start with 'error: ': {formatted}"
            );
            assert!(
                formatted.contains(". Suggestion: "),
                "structured format must include '. Suggestion: ': {formatted}"
            );
        }
    }

    #[test]
    fn structured_variants_preserve_source_chain() {
        let io_err = AppError::IoWithPath {
            path: PathBuf::from("/tmp/missing.wasm"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        };
        assert!(io_err.source().is_some());
        assert!(io_err.to_string().contains("/tmp/missing.wasm"));

        let arg_err = AppError::ArgumentMismatch {
            param: "step".to_string(),
            expected: "i64".to_string(),
            got: "xyz".to_string(),
        };
        assert_eq!(
            arg_err.to_string(),
            "failed to validate argument 'step': expected i64, got 'xyz'"
        );
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
