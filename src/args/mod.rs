pub mod cache;
pub mod config;
pub mod estimate;
pub mod watch;

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
    Estimate(estimate::EstimateArgs),

    /// Enumerate all public contract functions and estimate each one.
    EstimateAll(estimate::EstimateAllArgs),

    /// Fetch and store a snapshot of the network's resource-pricing configuration.
    Config(config::ConfigArgs),

    /// Poll network config on an interval and print diffs when they appear.
    Watch(watch::WatchArgs),

    /// Inspect and manage the local estimate cache.
    Cache(cache::CacheArgs),
}
