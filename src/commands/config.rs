use crate::args::config::{
    ConfigAction, ConfigArgs, DiffArgs, HistoryArgs, LastChangedArgs, SnapshotArgs,
};

pub async fn cmd_config(args: ConfigArgs) -> error::AppResult<()> {
    match args.action {
        ConfigAction::Snapshot(args) => cmd_config_snapshot(args).await,
        ConfigAction::Diff(args) => cmd_config_diff(args).await,
        ConfigAction::History(args) => cmd_config_history(&args),
        ConfigAction::LastChanged(args) => cmd_config_last_changed(&args),
    }
}
use crate::cache;
use crate::config_snapshot;
use crate::error;
use crate::rpc;
use crate::xdr_helper;

/// `config snapshot` command: fetch config settings and save snapshot.
pub async fn cmd_config_snapshot(args: SnapshotArgs) -> error::AppResult<()> {
    let endpoint = rpc::client::resolve_endpoint(&args.network, None)?;
    let client = rpc::client::RpcClient::new(&endpoint);
    let raw_entries = rpc::config::fetch_all_config_settings(&client).await?;

    let mut snapshot = xdr_helper::begin_snapshot(&args.network, 0);
    for raw in &raw_entries {
        let config_entry = xdr_helper::decode_config_entry_xdr(&raw.config_xdr)?;
        xdr_helper::apply_config_entry(&mut snapshot, config_entry);
    }
    if let Some(latest) = raw_entries.iter().map(|e| e.last_modified_ledger).max() {
        snapshot.ledger = latest;
    }

    let path = config_snapshot::store::save_snapshot(&snapshot, args.out.as_deref())?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }
    println!("Config snapshot saved to: {}", path.display());
    println!("Network: {}", snapshot.network);
    println!("Ledger:  {}", snapshot.ledger);
    println!("Time:    {}", snapshot.timestamp);
    Ok(())
}

/// `config diff` command: compare current config against a snapshot.
pub async fn cmd_config_diff(args: DiffArgs) -> error::AppResult<()> {
    let old_snapshot = match args.against.as_deref() {
        Some(path) => config_snapshot::store::load_snapshot_from_path(path)?,
        None => config_snapshot::store::load_latest_snapshot(&args.network)?,
    };

    let endpoint = rpc::client::resolve_endpoint(&args.network, None)?;
    let client = rpc::client::RpcClient::new(&endpoint);
    let raw_entries = rpc::config::fetch_all_config_settings(&client).await?;

    let mut new_snapshot = xdr_helper::begin_snapshot(&args.network, 0);
    for raw in &raw_entries {
        let config_entry = xdr_helper::decode_config_entry_xdr(&raw.config_xdr)?;
        xdr_helper::apply_config_entry(&mut new_snapshot, config_entry);
    }
    if let Some(latest) = raw_entries.iter().map(|e| e.last_modified_ledger).max() {
        new_snapshot.ledger = latest;
    }

    let diff = config_snapshot::diff::diff_snapshots(&old_snapshot, &new_snapshot);
    println!("{}", config_snapshot::diff::format_diff(&diff));

    if diff.has_pricing_changes {
        match config_snapshot::store::save_snapshot(&new_snapshot, None) {
            Ok(path) => {
                println!(
                    "  Protocol upgrade detected — new config auto-saved to {}",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("  Warning: could not auto-save post-upgrade snapshot: {e}");
            }
        }
    }

    // Check for stale cached estimates even when there are no pricing changes
    match cache::list_cached_estimates(&args.network) {
        Ok(estimates) => {
            if !estimates.is_empty() {
                let stale = cache::find_stale_estimates(&estimates, new_snapshot.ledger);
                if stale.is_empty() {
                    println!(
                        "  All cached estimates are current (ledger {}).",
                        new_snapshot.ledger
                    );
                } else {
                    println!(
                        "  {} cached estimate(s) from earlier ledger(s) — may be stale:",
                        stale.len()
                    );
                    for est in &stale {
                        println!(
                            "    - {} @ ledger {} (current: {})",
                            est.function, est.ledger, new_snapshot.ledger
                        );
                    }
                }
            }
        }
        Err(e) => {
            println!("  Warning: could not check cache: {e}");
        }
    }

    if diff.has_pricing_changes {
        std::process::exit(1);
    }
    Ok(())
}

/// `config history` command: print the full chronological change log.
pub fn cmd_config_history(args: &HistoryArgs) -> error::AppResult<()> {
    let log = config_snapshot::history::load_change_log(&args.network)?;
    println!(
        "{}",
        config_snapshot::history::format_change_log(&args.network, &log)
    );
    Ok(())
}

/// `config last-changed` command: print when each setting last changed.
pub fn cmd_config_last_changed(args: &LastChangedArgs) -> error::AppResult<()> {
    let log = config_snapshot::history::load_change_log(&args.network)?;
    let last_changed = config_snapshot::history::last_changed_from_log(&log);
    println!(
        "{}",
        config_snapshot::history::format_last_changed(&args.network, &last_changed)
    );
    Ok(())
}
