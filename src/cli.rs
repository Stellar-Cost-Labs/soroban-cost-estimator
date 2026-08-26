use clap::Parser;

use crate::args::{Cli, Command, config::ConfigAction};
use crate::commands;
use crate::error;

/// Parse CLI arguments and run the corresponding command.
pub async fn run() -> error::AppResult<()> {
    let args = Cli::parse();

    match args.command {
        Command::Estimate(cmd_args) => {
            commands::estimate::cmd_estimate(cmd_args).await
        }
        Command::EstimateAll(cmd_args) => {
            commands::estimate::cmd_estimate_all(cmd_args).await
        }
        Command::Config(cmd_args) => match cmd_args.action {
            ConfigAction::Snapshot(action_args) => {
                commands::config::cmd_config_snapshot(action_args).await
            }
            ConfigAction::Diff(action_args) => {
                commands::config::cmd_config_diff(action_args).await
            }
        },
        Command::Watch(cmd_args) => {
            commands::watch::cmd_watch(cmd_args).await
        }
    }
}
