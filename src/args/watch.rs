use clap::Args;

/// Poll network config on an interval and print diffs when they appear.
#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Network to watch.
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Polling interval (e.g. "30m", "1h").
    #[arg(long, default_value = "1h")]
    pub interval: String,
}
