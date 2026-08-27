use clap::Parser;
use soroban_cost_estimator::cache;
use soroban_cost_estimator::cli;
use soroban_cost_estimator::config_snapshot;
use soroban_cost_estimator::error;
use soroban_cost_estimator::report;
use soroban_cost_estimator::report::formatter::{JsonFormatter, ReportFormatter, TableFormatter};
use soroban_cost_estimator::rpc;
use soroban_cost_estimator::wasm;
use soroban_cost_estimator::xdr_helper;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = cli::Cli::parse();
    info!(command = ?args.command, "starting soroban-cost-estimator");

    if let Err(err) = run(args).await {
        error!(error = %err, "command failed");
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

async fn run(args: cli::Cli) -> error::AppResult<()> {
    match args.command {
        cli::Command::Estimate {
            wasm,
            network,
            rpc_url,
            r#fn,
            id,
            args,
            cache_ttl,
            json,
        } => {
            cmd_estimate(
                &wasm,
                &network,
                rpc_url.as_deref(),
                id.as_deref(),
                r#fn.as_deref(),
                &args,
                cache_ttl.as_deref(),
                json,
            )
            .await
        }
        cli::Command::EstimateAll {
            wasm,
            network,
            id,
            json,
        } => cmd_estimate_all(&wasm, &network, id.as_deref(), json).await,
        cli::Command::WasmInfo { wasm, json } => cmd_wasm_info(&wasm, json),
        cli::Command::Config { action } => match action {
            cli::ConfigAction::Snapshot { network, out, json } => {
                cmd_config_snapshot(&network, out.as_deref(), json).await
            }
            cli::ConfigAction::Diff { network, against } => {
                cmd_config_diff(&network, against.as_deref()).await
            }
            cli::ConfigAction::History { network } => cmd_config_history(&network),
            cli::ConfigAction::LastChanged { network } => cmd_config_last_changed(&network),
        },
        cli::Command::Cache { action } => match action {
            cli::CacheAction::Warm {
                wasm,
                network,
                id,
                json,
            } => cmd_cache_warm(&wasm, &network, id.as_deref(), json).await,
            cli::CacheAction::Verify => cmd_cache_verify(),
        },
        cli::Command::Watch { network, interval } => cmd_watch(&network, &interval).await,
    }
}

/// True when a simulation response carried neither cost data, nor
/// transaction data, nor a latest ledger — the signature of a misconfigured
/// request (bad `--id`, wrong network, or RPC schema drift), not a free
/// transaction.
fn missing_simulation_data(resp: &rpc::simulate::SimulateTransactionResponse) -> bool {
    resp.cost.is_none() && resp.transaction_data.is_none() && resp.latest_ledger.is_none()
}

/// Extract resource usage from a simulation response.
///
/// Legacy RPC versions report CPU/memory in a `cost` object; modern versions
/// dropped `cost` and carry the resources (CPU instructions, footprint entry
/// counts, disk read/write bytes) inside `transactionData` XDR. Memory bytes
/// are only reported by the legacy `cost` object — modern RPC responses do
/// not expose them, so they report 0.
///
/// Returns (cpu_insns, mem_bytes, read_entries, write_entries, read_bytes,
/// write_bytes). Entry/byte counts are u32 to match `CostReport`.
fn response_resources(
    response: &rpc::simulate::SimulateTransactionResponse,
) -> error::AppResult<(u64, u64, u32, u32, u32, u32)> {
    let resources = rpc::simulate::parse_transaction_data_resources(&response.transaction_data)?;
    let (cpu, mem) = match &response.cost {
        Some(cost) => (cost.cpu_insns, cost.mem_bytes),
        None => (resources.map(|r| r.cpu_insns).unwrap_or(0), 0),
    };
    let entries = |count: usize| u32::try_from(count).unwrap_or(u32::MAX);
    let bytes = |count: u64| u32::try_from(count).unwrap_or(u32::MAX);
    Ok((
        cpu,
        mem,
        resources.map(|r| entries(r.read_entries)).unwrap_or(0),
        resources.map(|r| entries(r.write_entries)).unwrap_or(0),
        resources.map(|r| bytes(r.read_bytes)).unwrap_or(0),
        resources.map(|r| bytes(r.write_bytes)).unwrap_or(0),
    ))
}

/// Fetch fee rates from the network config (compute + ledger cost + bandwidth).
///
/// Returns a `FeeRates` struct with raw config rates. These are passed to
/// `compute_fee_breakdown` which does the proper `(units * rate) / scale`
/// math to preserve precision.
///
/// If any of the three `ConfigSetting*` sources cannot be fetched or
/// decoded, its rate(s) fall back to 0 and a warning is printed to stderr
/// — a silent zero rate would understate the non-refundable fee, so it must
/// never pass unannounced.
async fn fetch_fee_rates(client: &rpc::client::RpcClient) -> report::fee_calc::FeeRates {
    use tracing::{debug, warn};

    let mut degraded: Vec<&'static str> = Vec::new();

    let raw_compute =
        rpc::config::fetch_config_setting(client, rpc::config::ConfigSettingId::ContractComputeV0)
            .await;

    let raw_ledger_cost = rpc::config::fetch_config_setting(
        client,
        rpc::config::ConfigSettingId::ContractLedgerCostV0,
    )
    .await;

    let raw_bandwidth = rpc::config::fetch_config_setting(
        client,
        rpc::config::ConfigSettingId::ContractBandwidthV0,
    )
    .await;

    // ConfigSettingContractComputeV0.fee_rate_per_instructions_increment
    // is stroops per 10,000 instructions (not per instruction).
    let compute_per_10k = match raw_compute {
        Ok(raw) => match xdr_helper::decode_config_entry_xdr(&raw.config_xdr) {
            Ok(stellar_xdr::ConfigSettingEntry::ContractComputeV0(s)) => {
                s.fee_rate_per_instructions_increment
            }
            _ => {
                degraded.push("ContractComputeV0");
                0
            }
        },
        Err(_) => {
            degraded.push("ContractComputeV0");
            0
        }
    };

    // ConfigSettingContractLedgerCostV0: per-entry read/write fees and the
    // per-KB disk read fee — all part of the non-refundable fee in
    // stellar-core's resource fee model.
    let (read_entry, write_entry, read_1kb) = match raw_ledger_cost {
        Ok(raw) => match xdr_helper::decode_config_entry_xdr(&raw.config_xdr) {
            Ok(stellar_xdr::ConfigSettingEntry::ContractLedgerCostV0(s)) => (
                s.fee_disk_read_ledger_entry,
                s.fee_write_ledger_entry,
                s.fee_disk_read1_kb,
            ),
            _ => {
                degraded.push("ContractLedgerCostV0");
                (0, 0, 0)
            }
        },
        Err(_) => {
            degraded.push("ContractLedgerCostV0");
            (0, 0, 0)
        }
    };

    // ConfigSettingContractBandwidthV0.fee_tx_size1_kb
    // is stroops per 1KB of tx size (not per byte).
    let bandwidth_per_kb = match raw_bandwidth {
        Ok(raw) => match xdr_helper::decode_config_entry_xdr(&raw.config_xdr) {
            Ok(stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(s)) => s.fee_tx_size1_kb,
            _ => {
                degraded.push("ContractBandwidthV0");
                0
            }
        },
        Err(_) => {
            degraded.push("ContractBandwidthV0");
            0
        }
    };

    if !degraded.is_empty() {
        warn!(sources = ?degraded, "fee rate source(s) unavailable — affected rate(s) set to 0");
        eprintln!(
            "Warning: fee rate source(s) {} unavailable — affected rate(s) set to 0 (non-refundable fee understated)",
            degraded.join(", ")
        );
    }

    let rates = report::fee_calc::FeeRates {
        fee_per_10k_insns: compute_per_10k,
        fee_per_read_entry: read_entry,
        fee_per_write_entry: write_entry,
        fee_per_read_1kb: read_1kb,
        fee_per_1kb: bandwidth_per_kb,
    };
    debug!(?rates, "fetched fee rates");
    rates
}

/// `estimate` command: simulate a single invocation and print cost report.
///
/// All RPC traffic (simulation and fee-rate fetches) goes through one
/// `RpcClient`, which deduplicates identical requests — the same method with
/// the same params — so a repeated WASM-upload envelope (when `--fn` is
/// omitted) or identical fee-rate fetches transmit at most once.
async fn cmd_estimate(
    wasm_path: &str,
    network: &str,
    rpc_url: Option<&str>,
    contract_id: Option<&str>,
    fn_name: Option<&str>,
    args: &[String],
    cache_ttl: Option<&str>,
    json_flag: bool,
) -> error::AppResult<()> {
    use sha2::Digest;
    use tracing::{Instrument, info_span};

    let span = info_span!(
        "cmd_estimate",
        wasm_path,
        network,
        fn = fn_name.unwrap_or("(upload)"),
        has_contract_id = contract_id.is_some(),
    );
    async {
        info!("loading WASM");
        let wasm_info = wasm::parser::load_wasm(std::path::Path::new(wasm_path))?;
        debug!(functions = wasm_info.functions.len(), has_spec = wasm_info.has_spec, "WASM loaded");

        let wasm_hash = hex::encode(sha2::Sha256::digest(&wasm_info.bytes));
        let function_name = fn_name.unwrap_or("(wasm upload)");

        // With --cache-ttl, reuse a still-fresh cached estimate and skip the
        // (expensive) simulation entirely.
        let ttl_secs = cache_ttl.map(parse_interval_secs);
        if let Some(fresh) =
            fresh_cached_estimate(&wasm_hash, &function_name, args, ttl_secs)?
        {
            let ttl_secs = ttl_secs.unwrap_or_default();
            info!(ttl_secs, function = %function_name, "cache hit — reusing fresh estimate");
            print_cached_estimate(&fresh, ttl_secs, json_flag);
            return Ok(());
        }

        let endpoint = rpc::client::resolve_endpoint(network, rpc_url)?;
        let client = rpc::client::RpcClient::new(&endpoint);

        let sc_vals: Vec<stellar_xdr::ScVal> = args
            .iter()
            .map(|a| xdr_helper::parse_arg_scval(a))
            .collect();
        debug!(arg_count = sc_vals.len(), "parsed arguments");

        let tx_xdr =
            xdr_helper::build_simulation_tx_envelope(&wasm_info.bytes, contract_id, fn_name, &sc_vals)?;
        let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_xdr);
        debug!(tx_xdr_len = tx_xdr.len(), "built simulation tx envelope");

        let response = rpc::simulate::simulate_transaction(&client, &tx_b64).await?;

        if missing_simulation_data(&response) {
            return Err(error::AppError::SimulationFailed(
                "simulation returned no cost data and no latest ledger — check --id, --fn, and the RPC endpoint".to_string(),
            ));
        }

        let (cpu_instructions, memory_bytes, read_entries, write_entries, read_bytes, write_bytes) =
            response_resources(&response)?;

        let latest_ledger: u32 = response
            .latest_ledger
            .and_then(|l| u32::try_from(l).ok())
            .unwrap_or(0);

        let total_fee_stroops = rpc::simulate::parse_resource_fee(&response.min_resource_fee)
            .unwrap_or(None)
            .or(rpc::simulate::parse_transaction_data_resource_fee(
                &response.transaction_data,
            )?)
            .unwrap_or(0);

        debug!(cpu_instructions, memory_bytes, latest_ledger, total_fee_stroops, "simulation complete");

        let fee_rates = fetch_fee_rates(&client).await;

        let fee = report::fee_calc::compute_fee_breakdown(
            total_fee_stroops,
            cpu_instructions,
            read_entries,
            write_entries,
            read_bytes,
            tx_xdr.len() as u32,
            fee_rates,
        );

        let report = report::cost_report::CostReport {
            function: function_name.to_string(),
            wasm_hash: wasm_hash.clone(),
            cpu_instructions,
            memory_bytes,
            tx_size: tx_xdr.len() as u32,
            read_entries,
            write_entries,
            read_bytes,
            write_bytes,
            fee: fee.clone(),
            ledger: latest_ledger,
            network: network.to_string(),
        };

        let _ = cache::save_estimate(
            &wasm_hash,
            function_name,
            args,
            network,
            latest_ledger,
            fee.total_stroops,
            cpu_instructions,
            memory_bytes,
        );
        info!(total_stroops = fee.total_stroops, total_xlm = %fee.total_xlm, "estimate complete");

        if json_flag {
            println!("{}", JsonFormatter.format(&report));
        } else {
            println!("{}", TableFormatter.format(&report));
        }

        Ok(())
    }
    .instrument(span)
    .await
}

/// `estimate-all` command: enumerate all functions and estimate each.
///
/// Every function shares a single deduplicating `RpcClient`. Batch runs that
/// hit the same request twice — the shared WASM-upload path when a function
/// envelope is built against an undeployed contract, or identical fee-rate
/// lookups — transmit each distinct `(method, params)` pair only once.
async fn cmd_estimate_all(
    wasm_path: &str,
    network: &str,
    contract_id: Option<&str>,
    json_flag: bool,
) -> error::AppResult<()> {
    use tracing::Instrument;
    use tracing::info_span;

    let span = info_span!("cmd_estimate_all", wasm_path, network);
    async {
        let wasm_info = wasm::parser::load_wasm(std::path::Path::new(wasm_path))?;
        let endpoint = rpc::client::resolve_endpoint(network, None)?;
        let client = rpc::client::RpcClient::new(&endpoint);

        if !json_flag {
            println!(
                "Enumerated {} function(s) in WASM:",
                wasm_info.functions.len()
            );
            for (i, fn_info) in wasm_info.functions.iter().enumerate() {
                println!("  {}. {}", i + 1, wasm::parser::format_function(fn_info));
            }
            println!();
            println!(
                "Contract spec: {}",
                if wasm_info.has_spec {
                    "present (typed params decoded from contractspecv0)"
                } else {
                    "absent (bare WASM exports only)"
                }
            );
            if contract_id.is_none() {
                println!(
                    "Note: pass --id <contract-id> to simulate each function against a deployed contract."
                );
            }
        }

        use sha2::Digest;
        let wasm_hash = hex::encode(sha2::Sha256::digest(&wasm_info.bytes));

        let mut json_results: Vec<serde_json::Value> = Vec::new();
        let total = wasm_info.functions.len();
        debug!(total, "enumerated functions");

        for (i, fn_info) in wasm_info.functions.iter().enumerate() {
            if !json_flag {
                println!("[{}/{}] {}", i + 1, total, fn_info.name);
            }
            let result = estimate_all_function(
                &client,
                &wasm_info,
                fn_info,
                contract_id,
                &wasm_hash,
                network,
                json_flag,
            )
            .await?;
            if let Some(value) = result {
                json_results.push(value);
            }
        }

        if json_flag {
            println!("{}", serde_json::to_string_pretty(&json_results)?);
        }

        Ok(())
    }
    .instrument(span)
    .await
}

/// Estimates one exported function against the network, printing its result
/// (non-JSON mode) or returning its JSON record (JSON mode).
#[allow(clippy::too_many_lines)]
async fn estimate_all_function(
    client: &rpc::client::RpcClient,
    wasm_info: &wasm::parser::WasmInfo,
    fn_info: &wasm::parser::FunctionInfo,
    contract_id: Option<&str>,
    wasm_hash: &str,
    network: &str,
    json_flag: bool,
) -> error::AppResult<Option<serde_json::Value>> {
    use tracing::{Instrument, debug, info_span};

    let span =
        info_span!("estimate_all_function", fn = %fn_info.name, param_count = fn_info.param_count);
    async {
        if fn_info.param_count > 0 {
            let reason = format!("needs --fn/--arg ({} param(s))", fn_info.param_count);
            debug!(reason, "skipping function");
            if json_flag {
                return Ok(Some(serde_json::json!({
                    "function": fn_info.name,
                    "status": "skipped",
                    "reason": reason,
                })));
            }
            println!("── Estimating '{}' ── Skipped: {reason}", fn_info.name);
            return Ok(None);
        }

        let tx_xdr = match xdr_helper::build_simulation_tx_envelope(
            &wasm_info.bytes,
            contract_id,
            Some(fn_info.name.as_str()),
            &[],
        ) {
            Ok(tx) => tx,
            Err(e) => {
                debug!(error = %e, "tx construction failed");
                if json_flag {
                    return Ok(Some(serde_json::json!({
                        "function": fn_info.name,
                        "status": "skipped",
                        "reason": e.to_string(),
                    })));
                }
                eprintln!("── Estimating '{}' ── Skipped: {e}", fn_info.name);
                return Ok(None);
            }
        };
        let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_xdr);
        debug!(tx_xdr_len = tx_xdr.len(), "built tx envelope");

        match rpc::simulate::simulate_transaction(client, &tx_b64).await {
            Ok(resp) => {
                if missing_simulation_data(&resp) {
                    let msg = "simulation returned no cost data and no latest ledger — check --id and the RPC endpoint";
                    debug!(msg, "simulation missing data");
                    if json_flag {
                        return Ok(Some(serde_json::json!({
                            "function": fn_info.name,
                            "status": "error",
                            "error": msg,
                        })));
                    }
                    eprintln!("── Estimating '{}' ── Error: {msg}", fn_info.name);
                    return Ok(None);
                }

                let (cpu, mem, ..) = response_resources(&resp)?;
                let fee = rpc::simulate::parse_resource_fee(&resp.min_resource_fee)
                    .unwrap_or(None)
                    .or(rpc::simulate::parse_transaction_data_resource_fee(
                        &resp.transaction_data,
                    )?)
                    .unwrap_or(0);
                let xlm = report::fee_calc::stroops_to_xlm(fee);
                let ledger: u32 = resp
                    .latest_ledger
                    .and_then(|l| u32::try_from(l).ok())
                    .unwrap_or(0);

                debug!(cpu, mem, fee, ledger, "simulation complete");

                let _ = cache::save_estimate(
                    wasm_hash,
                    &fn_info.name,
                    &[],
                    network,
                    ledger,
                    fee,
                    cpu,
                    mem,
                );

                if json_flag {
                    Ok(Some(serde_json::json!({
                        "function": fn_info.name,
                        "status": "ok",
                        "cpu_instructions": cpu,
                        "memory_bytes": mem,
                        "fee_stroops": fee,
                        "fee_xlm": xlm,
                        "ledger": ledger,
                    })))
                } else {
                    println!(
                        "CPU: {cpu} insns | Mem: {mem} bytes | Fee: {fee} stroops ({xlm} XLM) | Ledger: {ledger}"
                    );
                    Ok(None)
                }
            }
            Err(e) => {
                debug!(error = %e, "simulation failed");
                if json_flag {
                    Ok(Some(serde_json::json!({
                        "function": fn_info.name,
                        "status": "error",
                        "error": e.to_string(),
                    })))
                } else {
                    eprintln!("Skipped — simulation failed: {e}");
                    Ok(None)
                }
            }
        }
    }
    .instrument(span)
    .await
}

/// `wasm-info` command: print WASM metadata without making any RPC calls.
///
/// Shows the exported functions, contract-spec presence, binary size, and
/// SHA-256 hash — everything "cheap" to derive from the file itself.
///
/// # Network calls
/// None — pure file I/O + parsing.
fn cmd_wasm_info(wasm_path: &str, json_flag: bool) -> error::AppResult<()> {
    use sha2::Digest;

    let wasm_info = wasm::parser::load_wasm(std::path::Path::new(wasm_path))?;
    let hash = hex::encode(sha2::Sha256::digest(&wasm_info.bytes));

    if json_flag {
        println!(
            "{}",
            serde_json::to_string_pretty(&wasm_info_json(wasm_path, &wasm_info, &hash))?
        );
        return Ok(());
    }

    println!("WASM info: {wasm_path}");
    println!("  Size:      {} bytes", wasm_info.bytes.len());
    println!("  SHA-256:   {hash}");
    println!("  Functions: {}", wasm_info.functions.len());
    for (i, fn_info) in wasm_info.functions.iter().enumerate() {
        println!("    [{}] {}", i + 1, wasm::parser::format_function(fn_info));
    }
    println!(
        "  Contract spec: {}",
        if wasm_info.has_spec {
            "present (typed params decoded from contractspecv0)"
        } else {
            "absent (bare WASM exports only)"
        }
    );
    Ok(())
}

/// Builds the JSON representation of WASM metadata for `wasm-info --json`.
fn wasm_info_json(
    wasm_path: &str,
    wasm_info: &wasm::parser::WasmInfo,
    hash: &str,
) -> serde_json::Value {
    serde_json::json!({
        "path": wasm_path,
        "size": wasm_info.bytes.len(),
        "sha256": hash,
        "has_spec": wasm_info.has_spec,
        "functions": wasm_info.functions.iter().map(|f| {
            serde_json::json!({
                "name": f.name,
                "param_count": f.param_count,
                "result_count": f.result_count,
                "params": f.params.iter().map(|p| {
                    serde_json::json!({ "name": p.name, "type": p.type_name })
                }).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Fetches the current network config settings and builds a snapshot.
///
/// Shared by `config snapshot`, `config diff`, and `watch`.
///
/// # Network calls
/// Makes one batched `getLedgerEntries` RPC call.
async fn fetch_config_snapshot(
    network: &str,
) -> error::AppResult<config_snapshot::model::ConfigSnapshot> {
    use tracing::Instrument;
    use tracing::{debug, info_span};

    let span = info_span!("fetch_config_snapshot", network);
    async {
        let endpoint = rpc::client::resolve_endpoint(network, None)?;
        let client = rpc::client::RpcClient::new(&endpoint);
        debug!("fetching all config settings");
        let raw_entries = rpc::config::fetch_all_config_settings(&client).await?;
        debug!(entries = raw_entries.len(), "received config entries");

        let mut snapshot = xdr_helper::begin_snapshot(network, 0);
        for raw in &raw_entries {
            let config_entry = xdr_helper::decode_config_entry_xdr(&raw.config_xdr)?;
            xdr_helper::apply_config_entry(&mut snapshot, config_entry);
        }
        if let Some(latest) = raw_entries.iter().map(|e| e.last_modified_ledger).max() {
            snapshot.ledger = latest;
        }
        debug!(ledger = snapshot.ledger, "config snapshot built");
        Ok(snapshot)
    }
    .instrument(span)
    .await
}

/// Prints stale cached estimates for `network` relative to `ledger`, if any.
fn print_stale_estimates(network: &str, ledger: u32) {
    match cache::list_cached_estimates(network) {
        Ok(estimates) => {
            if !estimates.is_empty() {
                let stale = cache::find_stale_estimates(&estimates, ledger);
                if stale.is_empty() {
                    println!("  All cached estimates are current (ledger {ledger}).");
                } else {
                    println!(
                        "  {} cached estimate(s) from earlier ledger(s) — may be stale:",
                        stale.len()
                    );
                    for est in &stale {
                        println!(
                            "    - {} @ ledger {} (current: {})",
                            est.function, est.ledger, ledger
                        );
                    }
                }
            }
        }
        Err(e) => {
            println!("  Warning: could not check cache: {e}");
        }
    }
}

/// `config snapshot` command: fetch config settings and save snapshot.
async fn cmd_config_snapshot(
    network: &str,
    out_path: Option<&str>,
    json_flag: bool,
) -> error::AppResult<()> {
    use tracing::Instrument;
    use tracing::info_span;

    let span = info_span!("cmd_config_snapshot", network);
    async {
        info!("taking config snapshot");
        let snapshot = fetch_config_snapshot(network).await?;

        let path = config_snapshot::store::save_snapshot(&snapshot, out_path)?;
        info!(path = %path.display(), ledger = snapshot.ledger, "snapshot saved");

        if json_flag {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            return Ok(());
        }
        println!("Config snapshot saved to: {}", path.display());
        println!("Network: {}", snapshot.network);
        println!("Ledger:  {}", snapshot.ledger);
        println!("Time:    {}", snapshot.timestamp);
        Ok(())
    }
    .instrument(span)
    .await
}

/// True when a config diff signals a network protocol/config upgrade.
///
/// Pricing changes are the tool's proxy for "the network changed its
/// resource-pricing configuration under us" — e.g. what a protocol vote
/// produces — so they trigger the automatic post-upgrade snapshot.
fn upgrade_detected(diff: &config_snapshot::diff::ConfigDiff) -> bool {
    diff.has_pricing_changes
}

/// `config diff` command: compare current config against a snapshot.
async fn cmd_config_diff(network: &str, against_path: Option<&str>) -> error::AppResult<()> {
    use tracing::Instrument;
    use tracing::{debug, info_span};

    let span = info_span!("cmd_config_diff", network);
    async {
        let old_snapshot = match against_path {
            Some(path) => {
                debug!(path, "loading snapshot from path");
                config_snapshot::store::load_snapshot_from_path(path)?
            }
            None => {
                debug!("loading latest snapshot");
                config_snapshot::store::load_latest_snapshot(network)?
            }
        };

        let new_snapshot = fetch_config_snapshot(network).await?;

        let diff = config_snapshot::diff::diff_snapshots(&old_snapshot, &new_snapshot);
        debug!(
            change_count = diff.changes.len(),
            has_pricing = diff.has_pricing_changes,
            "diff computed"
        );
        println!("{}", config_snapshot::diff::format_diff(&diff));

        if upgrade_detected(&diff) {
            match config_snapshot::store::save_snapshot(&new_snapshot, None) {
                Ok(path) => {
                    info!(path = %path.display(), "auto-saved post-upgrade snapshot");
                    println!(
                        "  Protocol upgrade detected — new config auto-saved to {}",
                        path.display()
                    );
                }
                Err(e) => {
                    warn!(error = %e, "could not auto-save post-upgrade snapshot");
                    eprintln!("  Warning: could not auto-save post-upgrade snapshot: {e}");
                }
            }
        }

        print_stale_estimates(network, new_snapshot.ledger);

        if diff.has_pricing_changes {
            std::process::exit(1);
        }
        Ok(())
    }
    .instrument(span)
    .await
}

/// `config history` command: print the full chronological change log.
fn cmd_config_history(network: &str) -> error::AppResult<()> {
    let log = config_snapshot::history::load_change_log(network)?;
    println!(
        "{}",
        config_snapshot::history::format_change_log(network, &log)
    );
    Ok(())
}

/// `config last-changed` command: print when each setting last changed.
fn cmd_config_last_changed(network: &str) -> error::AppResult<()> {
    let log = config_snapshot::history::load_change_log(network)?;
    let last_changed = config_snapshot::history::last_changed_from_log(&log);
    println!(
        "{}",
        config_snapshot::history::format_last_changed(network, &last_changed)
    );
    Ok(())
}

/// Look up a cached estimate that is still fresh under `--cache-ttl`.
///
/// Returns `Ok(None)` when the flag is absent (nothing to do), no entry
/// exists, or the entry has expired — all of which mean "re-simulate".
///
/// # Network calls
/// None — pure file I/O.
fn fresh_cached_estimate(
    wasm_hash: &str,
    function: &str,
    args: &[String],
    ttl_secs: Option<u64>,
) -> error::AppResult<Option<cache::CachedEstimate>> {
    let Some(ttl_secs) = ttl_secs else {
        return Ok(None);
    };
    cache::load_fresh_estimate(
        wasm_hash,
        function,
        args,
        std::time::Duration::from_secs(ttl_secs),
    )
}

/// Print a fresh cached estimate that `estimate` is reusing instead of
/// re-simulating.
///
/// # Network calls
/// None — pure output.
fn print_cached_estimate(fresh: &cache::CachedEstimate, ttl_secs: u64, json_flag: bool) {
    if json_flag {
        println!(
            "{}",
            serde_json::json!({
                "cache": "hit",
                "function": fresh.function,
                "ledger": fresh.ledger,
                "total_stroops": fresh.total_stroops,
                "cpu_instructions": fresh.cpu_instructions,
                "memory_bytes": fresh.memory_bytes,
                "timestamp": fresh.timestamp,
            })
        );
    } else {
        println!(
            "Cache hit — estimate from {} is still fresh (TTL {ttl_secs}s); skipping simulation.",
            fresh.timestamp
        );
        println!(
            "  Total fee: {} stroops ({} XLM) | CPU: {} insns | Mem: {} bytes | Ledger: {}",
            fresh.total_stroops,
            report::fee_calc::stroops_to_xlm(fresh.total_stroops),
            fresh.cpu_instructions,
            fresh.memory_bytes,
            fresh.ledger,
        );
    }
}

/// Parse an interval like `3600`, `3600s`, `30m`, `1h`, or `1d` into seconds.
///
/// Defaults to one hour on unparseable input.
fn parse_interval_secs(interval: &str) -> u64 {
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
async fn shutdown_signal() -> error::AppResult<()> {
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
async fn watch_poll_once(network: &str, first: &mut bool) -> error::AppResult<()> {
    use tracing::{debug, warn};

    match fetch_config_snapshot(network).await {
        Ok(snapshot) => {
            if !*first {
                if let Ok(old_snapshot) = config_snapshot::store::load_latest_snapshot(network) {
                    let diff = config_snapshot::diff::diff_snapshots(&old_snapshot, &snapshot);
                    if !diff.changes.is_empty() {
                        debug!(change_count = diff.changes.len(), "config changes detected");
                        println!("{}", config_snapshot::diff::format_diff(&diff));
                    }

                    print_stale_estimates(network, snapshot.ledger);
                }
            }

            let _ = config_snapshot::store::save_snapshot(&snapshot, None);
            *first = false;
        }
        Err(e) => {
            warn!(error = %e, "failed to fetch config");
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
async fn cmd_watch(network: &str, interval: &str) -> error::AppResult<()> {
    use tracing::info;

    let interval_secs: u64 = parse_interval_secs(interval);

    info!(interval_secs, "starting watch");
    println!(
        "Watching {} for config changes every {}s... (Ctrl-C to stop)",
        network, interval_secs
    );

    let mut first = true;
    loop {
        tokio::select! {
            signal = shutdown_signal() => {
                signal?;
                info!("received stop signal");
                println!("Received stop signal — exiting cleanly.");
                return Ok(());
            }
            () = async {
                let _ = watch_poll_once(network, &mut first).await;
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            } => {}
        }
    }
}

/// `cache verify` command: check every cache entry parses as valid JSON.
///
/// Prints a summary line per corrupted entry and exits with code 1 when any
/// entry fails verification, so scripts can treat a corrupt cache as an
/// error. A healthy (or empty) cache exits 0.
///
/// # Network calls
/// None — pure file I/O.
fn cmd_cache_verify() -> error::AppResult<()> {
    use tracing::debug;

    let statuses = cache::verify_cache()?;
    debug!(total = statuses.len(), "verified cache entries");

    if statuses.is_empty() {
        println!("Cache is empty — nothing to verify.");
        return Ok(());
    }

    let total = statuses.len();
    let corrupt: Vec<&cache::CacheEntryStatus> = statuses.iter().filter(|s| !s.valid).collect();

    println!("Checked {total} cache entries.");

    if corrupt.is_empty() {
        println!("All cache entries are valid.");
    } else {
        println!(
            "{} of {total} cache entries failed verification:",
            corrupt.len()
        );
        for status in &corrupt {
            println!("  - {}", status.filename);
        }
        std::process::exit(1);
    }

    Ok(())
}

/// `cache warm` command: pre-populate cache by estimating every exported function.
async fn cmd_cache_warm(
    wasm_path: &str,
    network: &str,
    contract_id: Option<&str>,
    json_flag: bool,
) -> error::AppResult<()> {
    cmd_estimate_all(wasm_path, network, contract_id, json_flag).await
}

#[cfg(test)]
mod tests {
    use super::parse_interval_secs;
    use super::upgrade_detected;
    use super::wasm_info_json;
    use soroban_cost_estimator::config_snapshot::diff;
    use soroban_cost_estimator::config_snapshot::model::{
        ConfigSnapshot, ContractComputeV0, ContractLedgerCostV0,
    };
    use soroban_cost_estimator::wasm::parser::{FunctionInfo, ParamInfo, WasmInfo};

    fn snapshot_with_compute_fee(fee: i64) -> ConfigSnapshot {
        ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 100,
            contract_compute: Some(ContractComputeV0 {
                ledger_max_instructions: 1_000_000,
                tx_max_instructions: 100_000,
                fee_rate_per_instructions_increment: fee,
                tx_memory_limit: 41_943_040,
            }),
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
        }
    }

    fn snapshot_with_ledger_cost_fee(fee: i64) -> ConfigSnapshot {
        let mut snap = snapshot_with_compute_fee(5);
        snap.contract_ledger_cost = Some(ContractLedgerCostV0 {
            ledger_max_disk_read_entries: 1_000_000,
            ledger_max_disk_read_bytes: 1_000_000,
            ledger_max_write_ledger_entries: 1_000_000,
            ledger_max_write_bytes: 1_000_000,
            tx_max_disk_read_entries: 100,
            tx_max_disk_read_bytes: 1_000_000,
            tx_max_write_ledger_entries: 100,
            tx_max_write_bytes: 1_000_000,
            fee_disk_read_ledger_entry: fee,
            fee_write_ledger_entry: fee,
            fee_disk_read1_kb: fee,
            soroban_state_target_size_bytes: 1_000_000,
            rent_fee1_kb_soroban_state_size_low: fee,
            rent_fee1_kb_soroban_state_size_high: fee,
            soroban_state_rent_fee_growth_factor: 2000,
        });
        snap
    }

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

        // Fractional values are not supported by `u64` parsing, so they fall
        // back to the one-hour default before the unit multiplier is applied.
        assert_eq!(parse_interval_secs("1.5h"), 12_960_000);
        assert_eq!(parse_interval_secs("0.5m"), 216_000);
        assert_eq!(parse_interval_secs("2.25d"), 311_040_000);

        // Mixed case suffixes are normalized before parsing.
        assert_eq!(parse_interval_secs("45S"), 45);
        assert_eq!(parse_interval_secs("10M"), 600);
        assert_eq!(parse_interval_secs("2H"), 7_200);
        assert_eq!(parse_interval_secs("3D"), 259_200);

        // Boundary conditions: zero, u64 saturation, and leading zeros.
        assert_eq!(parse_interval_secs("0"), 0);
        assert_eq!(parse_interval_secs("0m"), 0);
        assert_eq!(parse_interval_secs("18446744073709551615"), u64::MAX);
        assert_eq!(parse_interval_secs("18446744073709551615s"), u64::MAX);
        assert_eq!(parse_interval_secs("18446744073709551615m"), u64::MAX);
        assert_eq!(parse_interval_secs("9999999999999999999999"), 3600);
        assert_eq!(parse_interval_secs("007"), 7);
    }

    #[test]
    fn test_upgrade_detected_on_pricing_change() {
        let old = snapshot_with_compute_fee(100);
        let new = snapshot_with_compute_fee(200);
        let diff = diff::diff_snapshots(&old, &new);

        assert!(diff.has_pricing_changes);
        assert!(upgrade_detected(&diff));
    }

    #[test]
    fn test_upgrade_detected_on_any_pricing_field_change() {
        let old = snapshot_with_ledger_cost_fee(1_000);
        let mut new = snapshot_with_ledger_cost_fee(1_000);
        if let Some(cost) = &mut new.contract_ledger_cost {
            cost.fee_write_ledger_entry = 2_000;
        }
        new.ledger = 200;
        let diff = diff::diff_snapshots(&old, &new);

        assert!(diff.has_pricing_changes);
        assert!(upgrade_detected(&diff));
    }

    #[test]
    fn test_no_upgrade_detected_without_changes() {
        let snap = snapshot_with_compute_fee(100);
        let diff = diff::diff_snapshots(&snap, &snap);

        assert!(!diff.has_pricing_changes);
        assert!(!upgrade_detected(&diff));
    }

    #[test]
    fn test_wasm_info_json_structure() {
        let info = WasmInfo {
            bytes: vec![0u8; 44],
            has_spec: true,
            functions: vec![FunctionInfo {
                name: "increment".to_string(),
                param_count: 1,
                result_count: 1,
                params: vec![ParamInfo {
                    name: "step".to_string(),
                    type_name: "I64".to_string(),
                }],
            }],
        };
        let value = wasm_info_json("/tmp/contract.wasm", &info, "deadbeef");

        assert_eq!(value["path"], "/tmp/contract.wasm");
        assert_eq!(value["size"], 44);
        assert_eq!(value["sha256"], "deadbeef");
        assert_eq!(value["has_spec"], true);
        assert_eq!(value["functions"][0]["name"], "increment");
        assert_eq!(value["functions"][0]["params"][0]["name"], "step");
        assert_eq!(value["functions"][0]["params"][0]["type"], "I64");
    }
}
