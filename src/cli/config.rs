//! Configuration snapshot and cache CLI subcommand definitions.

use std::path::PathBuf;

use clap::Subcommand;

use super::rpc::NetworkValueParser;

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Export cached estimates as a versioned JSON document (schema
    /// version, export timestamp, and estimate records) for backup or
    /// sharing across workstations.
    Export {
        /// Write the JSON export to a file instead of standard output.
        #[arg(long, short)]
        out: Option<String>,

        /// Only export estimates recorded for this network (default: all
        /// networks).
        #[arg(long)]
        network: Option<String>,
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

/// Sub-actions under `config snapshot` that operate on snapshots already on
/// disk rather than fetching a new one. Each carries its own `--network`
/// rather than inheriting the parent command's.
#[derive(Subcommand, Debug)]
pub enum SnapshotAction {
    /// List saved snapshots: filename, network, timestamp, ledger sequence
    /// and protocol version.
    List {
        /// Only list snapshots for this network (default: testnet).
        #[arg(long, default_value = "testnet", conflicts_with = "all")]
        network: String,

        /// List snapshots for every network.
        #[arg(long)]
        all: bool,

        /// Output a JSON array instead of a table.
        #[arg(long)]
        json: bool,
    },

    /// Delete stored snapshots older than a number of days.
    ///
    /// The newest snapshot is always kept, however old it is.
    Prune {
        /// Network whose snapshots should be pruned.
        #[arg(long, default_value = "testnet")]
        network: String,

        /// Delete snapshots recorded more than this many days ago.
        #[arg(long, value_name = "DAYS")]
        older_than: u32,

        /// Output as JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },

    /// Delete a saved snapshot file, or purge every snapshot older than N days.
    Delete {
        /// Snapshot filename (or path) to delete.
        #[arg(value_name = "FILENAME")]
        filename: Option<String>,

        /// Delete snapshots older than this many days.
        #[arg(long, value_name = "DAYS")]
        older_than: Option<u64>,

        /// Show which files would be removed without deleting anything.
        #[arg(long)]
        dry_run: bool,

        /// Skip the confirmation prompt (required in non-interactive sessions).
        #[arg(long, short = 'y')]
        yes: bool,

        /// Restrict `--older-than` to a single network's snapshots.
        #[arg(long, value_name = "NETWORK")]
        network: Option<String>,
    },

    /// Compare two saved snapshot files offline, without any network calls.
    Diff {
        /// First (older) snapshot file to compare.
        #[arg(value_name = "SNAPSHOT_A")]
        file_a: PathBuf,

        /// Second (newer) snapshot file to compare.
        #[arg(value_name = "SNAPSHOT_B")]
        file_b: PathBuf,

        /// Output as JSON instead of a human-readable diff.
        #[arg(long)]
        json: bool,
    },

    /// Validate a snapshot file or every stored snapshot.
    Validate {
        /// Explicit snapshot file to validate.
        #[arg(value_name = "PATH", required_unless_present = "all")]
        path: Option<PathBuf>,

        /// Validate every JSON snapshot in the snapshots directory.
        #[arg(long, conflicts_with = "path")]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Fetch all ConfigSetting entries and save a timestamped snapshot.
    ///
    /// Subcommands (`list`, `prune`) manage snapshots already on disk instead
    /// of fetching a new one, so they are mutually exclusive with this
    /// command's own flags.
    #[command(args_conflicts_with_subcommands = true)]
    Snapshot {
        #[command(subcommand)]
        action: Option<SnapshotAction>,
        #[arg(long, default_value = "testnet")]
        network: String,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        json: bool,

        /// Keep only the N most recent snapshots for the network, deleting
        /// older ones once the new snapshot is safely on disk.
        #[arg(long, value_name = "COUNT")]
        retain: Option<usize>,
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

        /// Force exit code 0 even when pricing changes are detected.
        /// Useful for informative CI reports that must not fail the build.
        /// Takes precedence over `--fail-on-any-change`.
        #[arg(long)]
        ignore_pricing_exit: bool,

        /// Exit with code 1 when any config setting changed, even
        /// non-pricing settings (caps, limits, window sizes). By default
        /// only pricing changes trigger a non-zero exit.
        #[arg(long)]
        fail_on_any_change: bool,
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
