use clap::{Parser, Subcommand};

/// Estimate Soroban contract resource costs with network config-drift tracking.
///
/// Wraps Stellar's `simulateTransaction` RPC and adds awareness of how the
/// network's resource-pricing configuration changes over time.
#[derive(Parser, Debug)]
#[command(name = "soroban-cost-estimator")]
#[command(about = "Estimate Soroban contract costs & track network pricing changes", long_about = None)]
pub struct Cli {
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
