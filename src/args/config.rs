use clap::{Args, Subcommand};

/// Fetch and store a snapshot of the network's resource-pricing configuration.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Fetch all ConfigSetting entries and save a timestamped snapshot.
    Snapshot(SnapshotArgs),

    /// Diff the current network config against the most recent snapshot.
    Diff(DiffArgs),
}

#[derive(Args, Debug)]
pub struct SnapshotArgs {
    /// Network to fetch config from.
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/).
    #[arg(long)]
    pub out: Option<String>,

    /// Print the snapshot as JSON instead of the summary lines.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Network to compare against.
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Explicit snapshot path to compare against (defaults to latest).
    #[arg(long)]
    pub against: Option<String>,
}
