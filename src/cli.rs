use clap::builder::{PossibleValue, TypedValueParser};
use clap::{Parser, Subcommand, ValueEnum};

/// Value parser for `--network` that advertises the supported network names to
/// shell completion generators and `--help` output while still accepting any
/// value at runtime, preserving the existing `AppError`-based handling of
/// unknown networks (issue #270).
#[derive(Clone)]
struct NetworkValueParser;

impl TypedValueParser for NetworkValueParser {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        value.to_str().map(str::to_owned).ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidUtf8,
                "invalid UTF-8: network value must be valid UTF-8",
            )
        })
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            ["testnet", "mainnet", "futurenet", "local"]
                .iter()
                .map(|&name| PossibleValue::new(name)),
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Csv,
    Markdown,
}

impl OutputFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Markdown => "markdown",
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    /// Optional TOML config file path. Defaults to ~/.config/soroban-cost-estimator/config.toml.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<String>,

    /// Select output format for commands that produce structured output (table, json, csv, markdown).
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,

    /// Cap RPC requests at N per second. 0 disables.
    #[arg(long, global = true, value_name = "N")]
    pub rps: Option<u64>,

    /// HTTP request timeout for RPC calls, in seconds (applies to every
    /// network call).
    #[arg(long, global = true, value_name = "SECS", default_value_t = 30)]
    pub timeout: u64,

    /// Enable debug-level logging, including full RPC request payloads and
    /// response summaries.
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Custom HTTP header to send with every RPC request, e.g.
    /// `--header "X-API-Key: secret"`. Repeatable for multiple headers.
    #[arg(long = "header", value_name = "KEY: VALUE", global = true)]
    pub headers: Vec<String>,

    /// Fallback RPC URL used when the primary endpoint is unreachable or
    /// returns a transient gateway error (HTTP 502/503/504).
    #[arg(long, global = true, value_name = "URL")]
    pub rpc_fallback_url: Option<String>,

    /// Retry transient RPC failures up to N times (default 3), using
    /// exponential backoff (500ms, then doubled between attempts). 0
    /// disables retries entirely.
    #[arg(long, global = true, value_name = "N", default_value_t = 3)]
    pub max_retries: usize,

    /// Control ANSI color formatting in terminal output.
    #[arg(long, global = true, default_value = "auto")]
    pub color: clap::ColorChoice,
    /// Print WASM structure information to stderr.
    #[arg(long, global = true)]
    pub wasm_info: bool,

    /// Maximum on-disk estimate cache size, in megabytes. When exceeded,
    /// the least-recently-accessed estimates are evicted down to 90% of the
    /// limit. 0 disables the byte quota.
    #[arg(long, global = true, value_name = "MB", default_value_t = 50)]
    pub max_cache_size_mb: u64,

    /// Maximum number of cached estimates. When exceeded, the
    /// least-recently-accessed estimates are evicted down to 90% of the
    /// limit. 0 disables the entry quota.
    #[arg(long, global = true, value_name = "N", default_value_t = 10_000)]
    pub max_cache_entries: usize,

    /// Suppress non-essential output, including the fee-distribution chart.
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// Number of decimal places shown for XLM fee values (0..=7, default 7).
    ///
    /// Stellar amounts are denominated in stroops (1 XLM = 10,000,000
    /// stroops, i.e. 7 decimals). Lower values give shorter, currency-style
    /// displays; 7 keeps full stroop fidelity.
    #[arg(
        long,
        global = true,
        value_name = "N",
        default_value_t = crate::report::fee_calc::DEFAULT_PRECISION,
        value_parser = clap::value_parser!(u32).range(0..=7),
    )]
    pub precision: u32,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Estimate {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
        #[arg(long)]
        rpc_url: Option<String>,
        #[arg(long)]
        r#fn: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long = "arg", value_name = "KEY=VAL")]
        args: Vec<String>,
        #[arg(long, value_name = "DURATION")]
        cache_ttl: Option<String>,

        /// Wipe this network's cached estimates before running the
        /// simulation (e.g. after upgrading the tool or a network upgrade).
        #[arg(long)]
        clear_cache: bool,

        /// Output as JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,

        /// Automatically save a new config snapshot if network pricing
        /// configuration has changed since the last snapshot.
        #[arg(long)]
        auto_snapshot: bool,

        /// Compare two WASM builds and print a side-by-side cost diff.
        /// Requires `--wasm-new`.
        #[arg(long, requires = "wasm_new")]
        diff: bool,

        /// The "new" WASM build to compare against when `--diff` is set.
        /// The `--wasm` file is treated as the baseline ("old") build.
        #[arg(long, value_name = "PATH")]
        wasm_new: Option<String>,

        /// Watch the WASM file for rebuilds and re-estimate on every change,
        /// printing a header with the timestamp and the fee change versus the
        /// previous build (Ctrl-C stops watching and exits with code 0).
        #[arg(long)]
        watch: bool,

        /// Parse WASM, validate arguments, and print the planned simulation
        /// payload without contacting the network. Useful for air-gapped
        /// environments or local contract verification.
        #[arg(long)]
        dry_run: bool,

        /// Project costs for batch invocations (comma-separated counts, e.g. "100,1000,10000").
        #[arg(
            long,
            value_name = "COUNTS",
            num_args = 0..=1,
            default_missing_value = "100,1000,10000"
        )]
        project: Option<String>,
    },
    EstimateAll {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,

        /// Explicit RPC URL (overrides network-based resolution).
        #[arg(long)]
        rpc_url: Option<String>,

        /// Deployed contract ID (64 hex chars) to invoke each function against.
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        json: bool,

        /// Automatically save a new config snapshot if network pricing
        /// configuration has changed since the last snapshot.
        #[arg(long)]
        auto_snapshot: bool,
    },
    WasmInfo {
        #[arg(long, short)]
        wasm: String,
        #[arg(long)]
        json: bool,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    Watch {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
        #[arg(long, default_value = "1h")]
        interval: String,
        /// Percentage threshold for flagging significant changes (e.g. 10 for 10%).
        #[arg(long, value_name = "N")]
        threshold_percent: Option<f64>,
    },

    /// Generate shell completion scripts for Bash, Zsh, Fish, PowerShell, and Elvish.
    Completions {
        /// Target shell for completion script generation.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Export every cached estimate as a JSON array.
    Export {
        /// Write the JSON array to a file instead of standard output.
        #[arg(long, short)]
        out: Option<String>,
    },

    /// List every cached estimate for a network (newest first).
    List {
        /// Network whose cached estimates to list.
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,

        /// Output the full cached-estimate records as a JSON array.
        #[arg(long)]
        json: bool,
    },

    /// Check that every cached estimate is valid JSON and not corrupted.
    Verify,

    /// Delete every cached estimate recorded for a network.
    Clear {
        /// Network whose cached estimates to delete.
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
    },

    /// Pre-populate the cache by estimating every exported function.
    Warm {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,

        /// Explicit RPC URL (overrides network-based resolution).
        #[arg(long)]
        rpc_url: Option<String>,

        /// Deployed contract ID (64 hex chars) to invoke each function against.
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// Evict least-recently-accessed estimates until the cache fits its
    /// configured quota (`--max-cache-size-mb` / `--max-cache-entries`).
    Prune,

    /// Query cached estimates with optional filters.
    Query {
        /// Network to filter by.
        #[arg(long)]
        network: Option<String>,

        /// Filter by function name (--function, --fn).
        #[arg(long = "fn", visible_alias = "function")]
        r#fn: Option<String>,

        /// Filter by WASM hash.
        #[arg(long)]
        wasm_hash: Option<String>,

        /// Minimum total fee in stroops (--min-stroops, --min-fee).
        #[arg(long = "min-fee", visible_alias = "min-stroops", value_name = "FEE")]
        min_fee: Option<i64>,

        /// Maximum total fee in stroops (--max-stroops, --max-fee).
        #[arg(long = "max-fee", visible_alias = "max-stroops", value_name = "FEE")]
        max_fee: Option<i64>,

        /// Earliest timestamp or date (--from, --since).
        #[arg(long = "since", visible_alias = "from", value_name = "DATE/TIME")]
        since: Option<String>,

        /// Latest timestamp (ISO-8601, e.g. "2024-12-31T23:59:59Z").
        #[arg(long, value_name = "TIMESTAMP")]
        to: Option<String>,

        /// Output as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },

    /// Show cache health overview: total entries, disk usage, and age.
    Stats {
        /// Output as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Snapshot {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        json: bool,
    },

    /// List all saved config snapshots with their timestamp and ledger.
    List {
        /// Network whose snapshots to list.
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
    },

    /// Diff the current network config against the most recent snapshot.
    Diff {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
        #[arg(long)]
        against: Option<String>,

        /// Diff the two most recent on-disk snapshots against each other
        /// instead of the live network. Never contacts the RPC endpoint.
        #[arg(long, conflicts_with = "against")]
        against_previous: bool,
        /// Hide non-pricing changes and display only fee-rate adjustments.
        #[arg(long)]
        pricing_only: bool,

        /// Percentage threshold for flagging significant changes (e.g. 10 for 10%).
        #[arg(long, value_name = "N")]
        threshold_percent: Option<f64>,

        /// Print a single-line summary (counts of pricing/non-pricing changes)
        /// instead of the full diff. Useful for CI status lines.
        #[arg(long)]
        summary: bool,

        /// Output as JSON instead of a human-readable diff.
        #[arg(long)]
        json: bool,
    },
    History {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
    },
    LastChanged {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
    },
    Validate {
        #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
        network: String,
    },

    /// Export network snapshots to a bundle file.
    Export {
        /// Network to export snapshots for.
        #[arg(long)]
        network: Option<String>,

        /// Output file path for the snapshot bundle.
        #[arg(long)]
        output: String,
    },

    /// Import network snapshots from a bundle file.
    Import {
        /// Path to the snapshot bundle file.
        bundle: String,
    },

    /// Query or manage the estimate cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

pub static COLOR_CHOICE: AtomicU8 = AtomicU8::new(0);

/// Global `--quiet` state, mirroring [`COLOR_CHOICE`]. Set once from the
/// parsed CLI so deeply nested helpers (e.g. chart rendering) can consult it
/// without threading a flag through every call site.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Record whether `--quiet` was passed.
pub fn init_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

/// Whether the user asked for quiet output.
#[must_use]
pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Terminal width to render the fee bar chart at, or `None` when the chart
/// must be suppressed.
///
/// The chart is disabled in `--quiet` mode, when stdout is not a TTY (piped
/// or redirected output), and when the terminal is narrower than
/// `crate::report::cost_report::MIN_CHART_WIDTH` columns. Column count is read
/// from `COLUMNS` when set and otherwise assumed to be
/// `crate::report::cost_report::DEFAULT_CHART_WIDTH`.
#[must_use]
pub fn chart_width() -> Option<usize> {
    use std::io::IsTerminal;

    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok());
    chart_width_for(is_quiet(), std::io::stdout().is_terminal(), columns)
}

/// Pure decision function behind [`chart_width`]: given the `--quiet` flag,
/// whether stdout is a TTY, and the reported terminal width, decide whether to
/// render the fee bar chart and at what width.
///
/// Split out from [`chart_width`] so the gating rules (quiet, non-TTY, and
/// minimum width) can be unit tested without a real terminal.
#[must_use]
pub fn chart_width_for(quiet: bool, is_tty: bool, columns: Option<usize>) -> Option<usize> {
    use crate::report::cost_report::{DEFAULT_CHART_WIDTH, MIN_CHART_WIDTH};

    if quiet || !is_tty {
        return None;
    }
    match columns {
        Some(width) if width >= MIN_CHART_WIDTH => Some(width),
        Some(_) => None,
        None => Some(DEFAULT_CHART_WIDTH),
    }
}

pub fn init_color(choice: clap::ColorChoice) {
    let val = match choice {
        clap::ColorChoice::Auto => 0,
        clap::ColorChoice::Always => 1,
        clap::ColorChoice::Never => 2,
    };
    COLOR_CHOICE.store(val, Ordering::Relaxed);
}

pub fn should_colorize() -> bool {
    match COLOR_CHOICE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            use std::io::IsTerminal;
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::cost_report::DEFAULT_CHART_WIDTH;

    #[test]
    fn test_chart_width_for_gating() {
        // Suppressed in quiet mode and for piped/non-TTY output.
        assert_eq!(chart_width_for(true, true, Some(120)), None);
        assert_eq!(chart_width_for(false, false, Some(120)), None);
        // Suppressed on terminals narrower than the minimum.
        assert_eq!(chart_width_for(false, true, Some(79)), None);
        // Rendered at the minimum width and scaled to the real width.
        assert_eq!(chart_width_for(false, true, Some(80)), Some(80));
        assert_eq!(chart_width_for(false, true, Some(120)), Some(120));
        // Unknown width falls back to the default assumption.
        assert_eq!(
            chart_width_for(false, true, None),
            Some(DEFAULT_CHART_WIDTH)
        );
    }

    /// AC for #270: generating a completion script for every supported shell
    /// must produce valid, non-empty output without panicking.
    #[test]
    fn test_completion_script_generation_all_shells() {
        use clap::CommandFactory;

        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();

        for shell in [
            clap_complete::Shell::Bash,
            clap_complete::Shell::Zsh,
            clap_complete::Shell::Fish,
            clap_complete::Shell::PowerShell,
            clap_complete::Shell::Elvish,
        ] {
            let mut buffer: Vec<u8> = Vec::new();
            clap_complete::generate(shell, &mut cmd, &bin_name, &mut buffer);
            let script = String::from_utf8(buffer).expect("completion script should be UTF-8");
            assert!(!script.is_empty(), "empty completion script for {shell:?}");
            assert!(
                script.contains(&bin_name),
                "completion script for {shell:?} should reference {bin_name}"
            );
        }
    }
}
