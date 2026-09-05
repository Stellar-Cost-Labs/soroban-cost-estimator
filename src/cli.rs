use clap::{Parser, Subcommand};
use soroban_spec_types::{ScVal, ScSpecTypeDef};

use crate::error::{AppError, AppResult};

/// Build version string with metadata from build.rs
fn build_version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " (",
        env!("GIT_HASH"),
        " ",
        env!("BUILD_DATE"),
        ")"
    )
}

/// Estimate Soroban contract resource costs with network config-drift tracking.
///
/// Wraps Stellar's `simulateTransaction` RPC and adds awareness of how the
/// network's resource-pricing configuration changes over time.
#[derive(Parser, Debug)]
#[command(name = "soroban-cost-estimator")]
#[command(version = build_version())]
#[command(about = "Estimate Soroban contract costs & track network pricing changes", long_about = None)]
pub struct Cli {
    /// Cap RPC requests at N per second (fixed-rate spacing; applies to
    /// every network call, e.g. batch runs like estimate-all). 0 disables.
    #[arg(long, global = true, value_name = "N")]
    pub rps: Option<u64>,

    /// HTTP request timeout for RPC calls, in seconds (applies to every
    /// network call).
    #[arg(long, global = true, value_name = "SECS", default_value_t = 30)]
    pub timeout: u64,

    /// Fallback RPC URL used when the primary endpoint is unreachable.
    #[arg(long, global = true, value_name = "URL")]
    pub rpc_fallback_url: Option<String>,

    /// Retry transient RPC failures up to N times (default 3), using
    /// exponential backoff (500ms, then doubled between attempts). 0
    /// disables retries entirely.
    #[arg(long, global = true, value_name = "N", default_value_t = 3)]
    pub max_retries: usize,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Simulate a single contract invocation and print the cost report.
    Estimate {
        /// Path to the compiled Soroban contract `.wasm` file.
        #[arg(long, short)]
        wasm: String,

        /// Network to simulate against.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Explicit RPC URL (overrides network-based resolution).
        #[arg(long)]
        rpc_url: Option<String>,

        /// Contract function name to invoke.
        #[arg(long)]
        r#fn: Option<String>,

        /// Deployed contract ID (64 hex chars) to invoke. Required when --fn is used.
        #[arg(long)]
        id: Option<String>,

        /// Function arguments as key=value pairs (value is coerced to the contract-spec type when available).
        #[arg(long = "arg", value_name = "KEY=VAL")]
        args: Vec<String>,

        /// Skip re-simulation when a cached estimate is still fresh
        /// (e.g. "30m", "1h", "7d"; bare value = seconds).
        #[arg(long, value_name = "DURATION")]
        cache_ttl: Option<String>,

        /// Output as JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,

        /// Output format: table (default), json, csv, or markdown.
        /// Overrides `--json` when both are supplied.
        #[arg(long, value_parser = ["table", "json", "csv", "markdown"])]
        format: Option<String>,

        /// Number of decimal places for XLM fee values (0..=18, default 7).
        #[arg(long, default_value_t = 7)]
        precision: u32,
    },

    /// Enumerate all public contract functions and estimate each one.
    EstimateAll {
        /// Path to the compiled Soroban contract `.wasm` file.
        #[arg(long, short)]
        wasm: String,

        /// Network to simulate against.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Deployed contract ID (TE8 hex chars) to invoke each function against.
        #[arg(long)]
        id: Option<String>,

        /// Output as JSON instead of a human-readable list.
        #[arg(long)]
        json: bool,

        /// Output format: table (default), json, csv, or markdown.
        /// Overrides `--json` when both are supplied.
        #[arg(long, value_parser = ["table", "json", "csv", "markdown"])]
        format: Option<String>,

        /// Number of decimal places for XLM fee values (0..=18, default 7).
        #[arg(long, default_value_t = 7)]
        precision: u32,
    },

    /// Print WASM metadata (functions, contract spec, size, hash) without any RPC calls.
    WasmInfo {
        /// Path to the compiled Soroban contract `.wasm` file.
        #[arg(long, short)]
        wasm: String,

        /// Output as JSON instead of a human-readable listing.
        #[arg(long)]
        json: bool,
    },

    /// Fetch and store a snapshot of the network's resource-pricing configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage the local estimate cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Poll network config on an interval and print diffs when they appear.
    Watch {
        /// Network to watch.
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Polling interval (e.g. "30m", "1h").
        #[arg(long, default_value = "1h")]
        interval: String,
    },
}

impl Command {
    /// Validate and coerce `--arg` values against the contract spec.
    ///
    /// This should be called if `--fn` is specified. It loads the deployed
    /// contract's spec from the `.wasm` file and coerces each argument to the
    /// declared input type. If a function or input is not found, or a value
    /// does not match its expected type, an `AppError::TypeValidation` is
    /// returned.
    pub fn parse_typed_args(&self) -> AppResult<Vec<ScVal>> {
        match self {
            Command::Estimate { wasm, r#fn, args, .. } => {
                let Some(func_name) = r#fn.as_deref() else {
                    return Ok(Vec::new());
                };

                let spec = crate::wasm::parser::get_function_spec(wasm, func_name)?
                    .ok_or_else(|| {
                        AppError::General(format!(
                            "function '{}' not found in contract spec",
                            func_name
                        ))
                    })?;

                let mut vals = Vec::with_capacity(args.len());
                for arg in args {
                    let (key, value) = arg.split_once('=').ok_or_else(|| {
                        AppError::TypeValidation(format!(
                            "invalid argument '{}', expected KEY=VAL",
                            arg
                        ))
                    })?;

                    let input = spec
                        .inputs()
                        .iter()
                        .find(|input| input.name() == key)
                        .ok_or_else(|| {
                            AppError::TypeValidation(format!(
                                "unknown argument '{}' for function '{}'",
                                key, func_name
                            ))
                        })?;

                    let expected = match input.type_def() {
                        ScSpecTypeDef::Bool => "bool",
                        ScSpecTypeDef::I64 => "i64",
                        ScSpecTypeDef::U64 => "u64",
                        ScSpecTypeDef::String => "string",
                        ScSpecTypeDef::Symbol => "symbol",
                        _ => return Err(AppError::TypeValidation(format!(
                            "unsupported parameter type for '{}': {:?}",
                            key,
                            input.type_def()
                        ))),
                    };

                    vals.push(coerce_arg_value(value, expected, key)?);
                }
                Ok(vals)
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Export every cached estimate as a JSON array.
    Export {
        /// Write the JSON array to a file instead of standard output.
        #[arg(long, short)]
        out: Option<String>,
    },

    /// Check that every cached estimate is valid JSON and not corrupted.
    Verify,

    /// Pre-populate the cache by estimating every exported function.
    Warm {
        /// Path to the compiled Soroban contract `.wasm` file.
        #[arg(long, short)]
        wasm: String,

        /// Network to simulate against.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Deployed contract ID (64 hex chars) to invoke each function against.
        #[arg(long)]
        id: Option<String>,

        /// Output as JSON instead of a human-readable list.
        #[arg(long)]
        json: bool,
    },

    /// Query cached estimates with optional filters.
    Query {
        /// Network to filter by.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Filter by function name (case-insensitive substring match).
        #[arg(long)]
        function: Option<String>,

        /// Filter by WASM hash prefix.
        #[arg(long)]
        wasm_hash: Option<String>,

        /// Minimum total fee in stroops.
        #[arg(long, value_name = "STROOPS")]
        min_stroops: Option<i64>,

        /// Maximum total fee in stroops.
        #[arg(long, value_name = "STROOPS")]
        max_stroops: Option<i64>,

        /// Earliest timestamp (ISO-8601, e.g. "2024-06-01T00:00:00Z").
        #[arg(long, value_name = "TIMESTAMP")]
        from: Option<String>,

        /// Latest timestamp (ISO-8601, e.g. "2024-12-31T23:59:59Z").
        #[arg(long, value_name = "TIMESTAMP")]
        to: Option<String>,

        /// Output as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Fetch all ConfigSetting entries and save a timestamped snapshot.
    Snapshot {
        /// Network to fetch config from.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Explicit output path (defaults to ~/.soroban-cost-estimator/.snapshots/).
        #[arg(long)]
        out: Option<String>,

        /// Print the snapshot as JSON instead of the summary lines.
        #[arg(long)]
        json: bool,
    },

    /// List all saved config snapshots with their timestamp and ledger.
    List {
        /// Network whose snapshots to list.
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// Diff the current network config against the most recent snapshot.
    Diff {
        /// Network to compare against.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Explicit snapshot path to compare against (defaults to latest).
        #[arg(long)]
        against: Option<String>,

        /// Print a single-line summary (counts of pricing/non-pricing changes)
        /// instead of the full diff. Useful for CI status lines.
        #[arg(long)]
        summary: bool,
    },

    /// Show the full chronological change log across all stored snapshots.
    History {
        /// Network whose snapshot history to inspect.
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// Show when each config setting last changed.
    LastChanged {
        /// Network whose snapshots to inspect.
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// Validate all stored snapshots for integrity.
    Validate {
        /// Network whose snapshots to validate.
        #[arg(long, default_value = "testnet")]
        network: String,
    },
}

/// Coerce a raw CLI argument value into an `ScVal` of the given type.
fn coerce_arg_value(raw: &str, expected: &str, param_name: &str) -> AppResult<ScVal> {
    match expected {
        "bool" => raw
            .parse::<bool>()
            .map(ScVal::Bool)
            .map_err(|_| {
                AppError::TypeValidation(format!(
                    "argument '{}' expected bool, got '{}'",
                    param_name, raw
                ))
            }),
        "i64" => raw
            .parse::<i64>()
            .map(ScVal::I64)
            .map_err(|_| {
                AppError::TypeValidation(format!(
                    "argument '{}' expected i64, got '{}'",
                    param_name, raw
                ))
            }),
        "u64" => raw
            .parse::<u64>()
            .map(ScVal::U64)
            .map_err(|_| {
                AppError::TypeValidation(format!(
                    "argument '{}' expected u64, got '{}'",
                    param_name, raw
                ))
            }),
        "string" => Ok(ScVal::String(raw.to_string())),
        "symbol" => Ok(ScVal::Symbol(raw.to_string())),
        other => Err(AppError::TypeValidation(format!(
            "unsupported expected type '{}' for argument '{}'",
            other, param_name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_bool() {
        assert_eq!(
            coerce_arg_value("true", "bool", "flag").unwrap(),
            ScVal::Bool(true)
        );
        assert!(coerce_arg_value("notabool", "bool", "flag").is_err());
    }

    #[test]
    fn coerce_i64() {
        assert_eq!(coerce_arg_value("42", "i64", "n").unwrap(), ScVal::I64(42));
        assert!(coerce_arg_value("abc", "i64", "n").is_err());
    }

    #[test]
    fn coerce_u64() {
        assert_eq!(coerce_arg_value("42", "u64", "n").unwrap(), ScVal::U64(42));
        assert!(coerce_arg_value("-1", "u64", "n").is_err());
    }

    #[test]
    fn coerce_string() {
        assert_eq!(
            coerce_arg_value("hello", "string", "s").unwrap(),
            ScVal::String("hello".to_string())
        );
    }

    #[test]
    fn coerce_symbol() {
        assert_eq!(
            coerce_arg_value("hello", "symbol", "s").unwrap(),
            ScVal::Symbol("hello".to_string())
        );
    }

    #[test]
    fn invalid_value_mentions_param_and_expected_type() {
        let err = coerce_arg_value("abc", "i64", "step").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("step"));
        assert!(msg.contains("i64"));
    }
}