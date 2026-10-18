//! Cost estimation CLI argument structs (`estimate` and `estimate-all`).

use std::path::PathBuf;

use clap::Args;

use super::rpc::NetworkValueParser;

/// Arguments for the `estimate` subcommand.
#[derive(Args, Clone, Debug)]
pub struct EstimateArgs {
    /// Path to a compiled Soroban contract `.wasm` file. Repeat the flag
    /// to evaluate multiple contracts in a single batch run.
    #[arg(long, short, value_name = "PATH", required_unless_present = "wasm_dir")]
    pub wasm: Vec<PathBuf>,

    /// Directory of `.wasm` files to evaluate as a batch (non-recursive).
    #[arg(long, value_name = "DIR")]
    pub wasm_dir: Option<PathBuf>,

    /// Network to simulate against.
    #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
    pub network: String,
    #[arg(long)]
    pub rpc_url: Option<String>,
    #[arg(long)]
    pub r#fn: Option<String>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long = "arg", value_name = "KEY=VAL")]
    pub args: Vec<String>,

    /// Prompt interactively for the function and its arguments, using
    /// the contract spec for names and types. Explicit `--fn`/`--arg`
    /// values take precedence; the prompt only fills in the gaps.
    #[arg(long, short = 'i')]
    pub interactive: bool,

    /// Skip re-simulation when a cached estimate is still fresh
    /// (e.g. "30m", "1h", "7d"; bare value = seconds).
    #[arg(long, value_name = "DURATION")]
    pub cache_ttl: Option<String>,

    /// Show the cost difference against the previous cached estimate for
    /// the same function and arguments (CPU, memory, ledger I/O, and fee).
    #[arg(long)]
    pub compare: bool,

    /// Wipe this network's cached estimates before running the
    /// simulation (e.g. after upgrading the tool or a network upgrade).
    #[arg(long)]
    pub clear_cache: bool,

    /// Bypass the estimate cache entirely: never read a cached estimate
    /// (including under `--cache-ttl`) and never write the fresh result
    /// back to disk.
    #[arg(long)]
    pub no_cache: bool,

    /// Compare against up to 5 previous cached runs of the same function
    /// and render a cost-trend table (red = regression, green = improvement).
    /// Adds a `history` array to `--json` output when requested.
    #[arg(long)]
    pub history: bool,

    /// Output as JSON instead of a human-readable table.
    #[arg(long)]
    pub json: bool,

    /// Automatically save a new config snapshot if network pricing
    /// configuration has changed since the last snapshot.
    #[arg(long)]
    pub auto_snapshot: bool,

    /// Compare two WASM builds and print a side-by-side cost diff.
    /// Requires `--wasm-new`.
    #[arg(long, requires = "wasm_new")]
    pub diff: bool,

    /// The "new" WASM build to compare against when `--diff` is set.
    /// The `--wasm` file is treated as the baseline ("old") build.
    #[arg(long, value_name = "PATH")]
    pub wasm_new: Option<String>,

    /// Watch the WASM file for rebuilds and re-estimate on every change,
    /// printing a header with the timestamp and the fee change versus the
    /// previous build (Ctrl-C stops watching and exits with code 0).
    #[arg(long)]
    pub watch: bool,

    /// Parse WASM, validate arguments, and print the planned simulation
    /// payload without contacting the network. Useful for air-gapped
    /// environments or local contract verification.
    #[arg(long)]
    pub dry_run: bool,

    /// Project costs for batch invocations (comma-separated counts, e.g. "100,1000,10000").
    #[arg(
        long,
        value_name = "COUNTS",
        num_args = 0..=1,
        default_missing_value = "100,1000,10000"
    )]
    pub project: Option<String>,

    /// Number of times to repeat the simulation (1..=100, default 1).
    /// Collects latency statistics (min, max, mean, stddev) and checks
    /// fee/CPU consistency across runs.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub repeat: u32,
}

/// Arguments for the `estimate-all` subcommand.
#[derive(Args, Clone, Debug)]
pub struct EstimateAllArgs {
    /// Path to a compiled Soroban contract `.wasm` file. Repeat the flag
    /// to estimate multiple contracts in a single batch run.
    #[arg(long, short, value_name = "PATH", required_unless_present = "wasm_dir")]
    pub wasm: Vec<PathBuf>,

    /// Directory of `.wasm` files to estimate as a batch (non-recursive).
    #[arg(long, value_name = "DIR")]
    pub wasm_dir: Option<PathBuf>,

    /// Network to simulate against.
    #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
    pub network: String,

    /// Explicit RPC URL (overrides network-based resolution).
    #[arg(long)]
    pub rpc_url: Option<String>,

    /// Deployed contract ID (64 hex chars) to invoke each function against.
    #[arg(long)]
    pub id: Option<String>,

    /// Bypass the estimate cache entirely: never read cached estimates
    /// and never write fresh results back to disk.
    #[arg(long)]
    pub no_cache: bool,

    /// Restrict estimation to these function names (repeatable). When
    /// omitted, every exported function is estimated.
    #[arg(long = "fn", value_name = "NAME")]
    pub fn_names: Vec<String>,

    #[arg(long)]
    pub json: bool,

    /// Automatically save a new config snapshot if network pricing
    /// configuration has changed since the last snapshot.
    #[arg(long)]
    pub auto_snapshot: bool,
}
