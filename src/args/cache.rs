use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub action: CacheAction,
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Check that every cached estimate is valid JSON and not corrupted.
    Verify,
}
