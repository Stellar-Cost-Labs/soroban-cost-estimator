use clap::{Parser, Subcommand};

/// Estimate Soroban contract resource costs with network config-drift tracking.
///
/// Wraps Stellar's `simulateTransaction` RPC and adds awareness of how the
/// network's resource-pricing configuration changes over time.
#[derive(Parser, Debug)]
#[command(name = "soroban-cost-estimator")]
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

        /// Function arguments as key=value pairs (value is type-inferred).
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

        /// Deployed contract ID (64 hex chars) to invoke each function against.
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

#[derive(Subcommand, Debug)]
pub enum CacheAction {
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

        /// Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/).
        #[arg(long)]
        out: Option<String>,

        /// Print the snapshot as JSON instead of the summary lines.
        #[arg(long)]
        json: bool,
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
        /// Network whose snapshot history to inspect.
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
