use crate::args::watch::WatchArgs;
use crate::cache;
use crate::config_snapshot;
use crate::error;
use crate::rpc;
use crate::xdr_helper;

/// Parse an interval like `3600`, `3600s`, `30m`, `1h`, or `1d` into seconds.
///
/// Defaults to one hour on unparseable input.
pub fn parse_interval_secs(interval: &str) -> u64 {
    let trimmed = interval.trim().to_ascii_lowercase();
    let (num_part, mult) = match trimmed.chars().last() {
        Some('s') => (&trimmed[..trimmed.len() - 1], 1u64),
        Some('m') => (&trimmed[..trimmed.len() - 1], 60u64),
        Some('h') => (&trimmed[..trimmed.len() - 1], 3600u64),
        Some('d') => (&trimmed[..trimmed.len() - 1], 86_400u64),
        _ => (&trimmed[..], 1u64),
    };
    num_part.parse::<u64>().unwrap_or(3600).saturating_mul(mult)
}

/// Resolves when the process receives SIGINT (Ctrl-C) or SIGTERM, so a
/// long-running command can stop gracefully.
///
/// # Network calls
/// None — waits on OS signals.
pub async fn shutdown_signal() -> error::AppResult<()> {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    Ok(())
}

/// Runs one `watch` poll cycle: fetch the network config, diff it against
/// the previous snapshot, print changes and stale-estimate info, then save
/// the new snapshot.
///
/// # Network calls
/// Makes one batched `getLedgerEntries` RPC call.
pub async fn watch_poll_once(network: &str, first: &mut bool) -> error::AppResult<()> {
    let endpoint = rpc::client::resolve_endpoint(network, None)?;
    let client = rpc::client::RpcClient::new(&endpoint);

    match rpc::config::fetch_all_config_settings(&client).await {
        Ok(raw_entries) => {
            let mut snapshot = xdr_helper::begin_snapshot(network, 0);
            for raw in &raw_entries {
                if let Ok(config_entry) = xdr_helper::decode_config_entry_xdr(&raw.config_xdr) {
                    xdr_helper::apply_config_entry(&mut snapshot, config_entry);
                }
            }
            if let Some(latest) = raw_entries.iter().map(|e| e.last_modified_ledger).max() {
                snapshot.ledger = latest;
            }

            if !*first {
                if let Ok(old_snapshot) = config_snapshot::store::load_latest_snapshot(network) {
                    let diff = config_snapshot::diff::diff_snapshots(&old_snapshot, &snapshot);
                    if !diff.changes.is_empty() {
                        println!("{}", config_snapshot::diff::format_diff(&diff));
                    }

                    // Check for stale cached estimates even when there are no pricing changes
                    if let Ok(estimates) = cache::list_cached_estimates(network) {
                        if !estimates.is_empty() {
                            let stale = cache::find_stale_estimates(&estimates, snapshot.ledger);
                            if stale.is_empty() {
                                println!(
                                    "  All cached estimates are current (ledger {}).",
                                    snapshot.ledger
                                );
                            } else {
                                println!(
                                    "  {} cached estimate(s) from earlier ledger(s) — may be stale:",
                                    stale.len()
                                );
                                for est in &stale {
                                    println!(
                                        "    - {} @ ledger {} (current: {})",
                                        est.function, est.ledger, snapshot.ledger
                                    );
                                }
                            }
                        }
                    }
                }
            }

            let _ = config_snapshot::store::save_snapshot(&snapshot, None);
            *first = false;
        }
        Err(e) => {
            eprintln!("Warning: failed to fetch config: {e}");
        }
    }
    Ok(())
}

/// `watch` command: poll network config and print diffs.
///
/// Polls immediately, then on `interval`, until SIGINT (Ctrl-C) or SIGTERM
/// is received — then exits cleanly with code 0. The in-flight poll is
/// cancelled rather than writing a partial snapshot.
pub async fn cmd_watch(args: WatchArgs) -> error::AppResult<()> {
    let interval_secs: u64 = parse_interval_secs(&args.interval);

    println!(
        "Watching {} for config changes every {}s... (Ctrl-C to stop)",
        args.network, interval_secs
    );

    let mut first = true;
    loop {
        tokio::select! {
            signal = shutdown_signal() => {
                signal?;
                println!("Received stop signal — exiting cleanly.");
                return Ok(());
            }
            () = async {
                let _ = watch_poll_once(&args.network, &mut first).await;
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_interval_secs;

    #[test]
    fn test_parse_interval_secs() {
        assert_eq!(parse_interval_secs("3600"), 3600);
        assert_eq!(parse_interval_secs("3600s"), 3600);
        assert_eq!(parse_interval_secs("30m"), 1800);
        assert_eq!(parse_interval_secs("1h"), 3600);
        assert_eq!(parse_interval_secs("1d"), 86_400);
        assert_eq!(parse_interval_secs(" 5M "), 300);
        // Unparseable input falls back to the one-hour default.
        assert_eq!(parse_interval_secs(""), 3600);
        assert_eq!(parse_interval_secs("s"), 3600);
        assert_eq!(parse_interval_secs("10ss"), 3600);
        assert_eq!(parse_interval_secs("garbage"), 3600);
    }
}
