use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Csv,
    Markdown,
}

#[derive(Parser, Debug)]
#[command(name = "soroban-cost-estimator")]
#[command(about = "Estimate Soroban contract costs & track network pricing changes", long_about = None)]
pub struct Cli {
    /// Select output format for commands that produce structured output.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,

    /// Cap RPC requests at N per second. 0 disables.
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
    Estimate {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet")]
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
        /// Deprecated alias for `--format json`.
        #[arg(long)]
        json: bool,

        /// Output format: table (default), json, csv, or markdown.
        /// Overrides `--json` when both are supplied.
        #[arg(long, value_parser = ["table", "json", "csv", "markdown"])]
        format: Option<String>,
    },
    EstimateAll {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        json: bool,
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
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long, default_value = "1h")]
        interval: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    Verify,
    Warm {
        #[arg(long, short)]
        wasm: String,
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long)]
        id: Option<String>,
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
    Snapshot {
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Diff {
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long)]
        against: Option<String>,

        /// Print a single-line summary (counts of pricing/non-pricing changes)
        /// instead of the full diff. Useful for CI status lines.
        #[arg(long)]
        summary: bool,
    },
    History {
        #[arg(long, default_value = "testnet")]
        network: String,
    },
    LastChanged {
        #[arg(long, default_value = "testnet")]
        network: String,
    },
    Validate {
        #[arg(long, default_value = "testnet")]
        network: String,
    },
}
