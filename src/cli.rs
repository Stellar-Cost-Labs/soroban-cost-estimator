use clap::Parser;

use crate::args::config::ConfigAction;
use crate::args::{Cli, Command};
use crate::commands;
use crate::error;
use tracing::info;

/// Parse CLI arguments and run the corresponding command.
pub async fn run() -> error::AppResult<()> {
    let args = Cli::parse();
    info!(command = ?args.command, "starting soroban-cost-estimator");

    match args.command {
        Command::Estimate(cmd_args) => commands::estimate::cmd_estimate(cmd_args).await,
        Command::EstimateAll(cmd_args) => commands::estimate::cmd_estimate_all(cmd_args).await,
        Command::Config(cmd_args) => match cmd_args.action {
            ConfigAction::Snapshot(action_args) => {
                commands::config::cmd_config_snapshot(action_args).await
            }
            ConfigAction::Diff(action_args) => commands::config::cmd_config_diff(action_args).await,
            ConfigAction::History(action_args) => commands::config::cmd_config_history(&action_args),
            ConfigAction::LastChanged(action_args) => {
                commands::config::cmd_config_last_changed(&action_args)
            }
        },
        Command::Watch(cmd_args) => commands::watch::cmd_watch(cmd_args).await,
        Command::Cache(cmd_args) => commands::cache::cmd_cache(&cmd_args.action),
    }
}
