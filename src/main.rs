use clap::Parser;
use comfy_table::Cell;
use comfy_table::Table;
use soroban_cost_estimator::cache;
use soroban_cost_estimator::cli;
use soroban_cost_estimator::config_snapshot;
use soroban_cost_estimator::error;
use soroban_cost_estimator::report;
use soroban_cost_estimator::report::formatter::{
    ReportFormatter, TableFormatter, formatter_by_name,
};
use soroban_cost_estimator::rpc;
use soroban_cost_estimator::wasm;
use soroban_cost_estimator::xdr_helper;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

/// Status of a single function in an `estimate-all` run.
///
/// Serialized as a lowercase string (`ok` / `skipped` / `error`) so the JSON
/// array stays uniform across every function regardless of outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EstimateAllStatus {
    Ok,
    Skipped,
    Error,
}

/// A well-typed, serializable result for one function in an `estimate-all`
/// run.
///
/// Every entry carries the same fields, so `estimate-all --json` always emits
/// a uniform array: `status` is always present, while `reason`/`error` and the
/// resource/fee blocks are populated only for the relevant statuses (empty
/// fields are omitted via `skip_serializing_if`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EstimateAllResult {
    /// Name of the contract function that was (or would be) simulated.
    pub function: String,
    /// Outcome of estimating this function.
    pub status: EstimateAllStatus,
    /// Why the function was skipped (present only when `status` is `skipped`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Error message (present only when `status` is `error`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_instructions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_entries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_entries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<report::fee_calc::FeeBreakdown>,
    /// Resource-limit warnings for this function (#322). Always present in
    /// JSON output (an empty array when nothing is near a protocol limit).
    #[serde(default)]
    pub warnings: Vec<report::cost_report::ResourceWarning>,
}

impl EstimateAllResult {
    /// Build a `skipped` result (function could not be simulated).
    fn skipped(function: &str, reason: impl Into<String>) -> Self {
        Self {
            function: function.to_string(),
            status: EstimateAllStatus::Skipped,
            reason: Some(reason.into()),
            error: None,
            wasm_hash: None,
            network: None,
            ledger: None,
            cpu_instructions: None,
            memory_bytes: None,
            read_entries: None,
            write_entries: None,
            read_bytes: None,
            write_bytes: None,
            tx_size: None,
            fee: None,
            warnings: Vec::new(),
        }
    }

    /// Build an `error` result (simulation failed).
    fn errored(function: &str, error: impl Into<String>) -> Self {
        Self {
            function: function.to_string(),
            status: EstimateAllStatus::Error,
            reason: None,
            error: Some(error.into()),
            wasm_hash: None,
            network: None,
            ledger: None,
            cpu_instructions: None,
            memory_bytes: None,
            read_entries: None,
            write_entries: None,
            read_bytes: None,
            write_bytes: None,
            tx_size: None,
            fee: None,
            warnings: Vec::new(),
        }
    }
}

#[tokio::main]
async fn main() {
    let args = cli::Cli::parse();

    let default_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level)),
        )
        .init();

    info!(command = ?args.command, verbose = args.verbose, "starting soroban-cost-estimator");

    if let Err(err) = run(args).await {
        error!(error = %err, "command failed");
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
async fn run(args: cli::Cli) -> error::AppResult<()> {
    let rps = args.rps;
    let timeout = args.timeout;
    let max_retries = args.max_retries;
    let fallback = args.rpc_fallback_url.as_deref();
    let headers = args.headers;
    match args.command {
        cli::Command::Estimate {
            wasm,
            network,
            rpc_url,
            r#fn,
            id,
            args,
            cache_ttl,
            clear_cache,
            history,
            json,
            format,
            precision,
        } => {
            // `--format` wins when both it and the legacy `--json` flag are
            // supplied; otherwise fall back to the JSON/table defaults.
            let format = format.unwrap_or_else(|| if json { "json" } else { "table" }.to_string());
            cmd_estimate(
                &wasm,
                &network,
                rpc_url.as_deref(),
                fallback,
                id.as_deref(),
                r#fn.as_deref(),
                &args,
                cache_ttl.as_deref(),
                clear_cache,
                history,
                &format,
                rps,
                timeout,
                max_retries,
                precision,
                &headers,
            )
            .await
        }
        cli::Command::EstimateAll {
            wasm,
            network,
            rpc_url,
            id,
            json,
            format,
            precision,
        } => {
            let format = format.unwrap_or_else(|| if json { "json" } else { "table" }.to_string());
            cmd_estimate_all(
                &wasm,
                &network,
                rpc_url.as_deref(),
                fallback,
                id.as_deref(),
                &format,
                rps,
                timeout,
                max_retries,
                precision,
                &headers,
            )
            .await
        }
        cli::Command::WasmInfo { wasm, json } => cmd_wasm_info(&wasm, json),
        cli::Command::Config { action } => match action {
            cli::ConfigAction::Snapshot { network, out, json } => {
                cmd_config_snapshot(
                    &network,
                    fallback,
                    out.as_deref(),
                    json,
                    rps,
                    timeout,
                    max_retries,
                    &headers,
                )
                .await
            }
            cli::ConfigAction::List { network } => cmd_config_snapshot_list(&network),
            cli::ConfigAction::Diff {
                network,
                against,
                summary,
                json,
            } => {
                cmd_config_diff(
                    &network,
                    fallback,
                    against.as_deref(),
                    summary,
                    json,
                    rps,
                    timeout,
                    max_retries,
                    &headers,
                )
                .await
            }
            cli::ConfigAction::History { network } => cmd_config_history(&network),
            cli::ConfigAction::LastChanged { network } => cmd_config_last_changed(&network),
            cli::ConfigAction::Validate { network } => cmd_config_validate(&network),
        },
        cli::Command::Cache { action } => match action {
            cli::CacheAction::Export { out } => cmd_cache_export(out.as_deref()),
            cli::CacheAction::Warm {
                wasm,
                network,
                rpc_url,
                id,
                json,
            } => {
                cmd_cache_warm(
                    &wasm,
                    &network,
                    rpc_url.as_deref(),
                    fallback,
                    id.as_deref(),
                    json,
                    rps,
                    timeout,
                    max_retries,
                    &headers,
                )
                .await
            }
            cli::CacheAction::Verify => cmd_cache_verify(),
            cli::CacheAction::Clear { network } => cmd_cache_clear(&network),
            cli::CacheAction::Query {
                network,
                function,
                wasm_hash,
                min_stroops,
                max_stroops,
                from,
                to,
                json,
            } => cmd_cache_query(
                &network,
                function.as_deref(),
                wasm_hash.as_deref(),
                min_stroops,
                max_stroops,
                from.as_deref(),
                to.as_deref(),
                json,
            ),
        },
        cli::Command::Watch { network, interval } => {
            cmd_watch(
                &network,
                fallback,
                &interval,
                rps,
                timeout,
                max_retries,
                &headers,
            )
            .await
        }
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

/// Fetch the network's resource-limit configuration for the 80% warnings
/// (#322).
///
/// Pulls `ConfigSettingContractComputeV0`, `ContractLedgerCostV0`, and
/// `ContractBandwidthV0` — the same entries `fetch_fee_rates` already reads.
/// The RPC client deduplicates identical `(method, params)` requests, so on
/// the `estimate` path this adds no additional network round-trips. Missing
/// entries leave the corresponding limits as `None` and are simply skipped.
async fn fetch_network_config(
    client: &rpc::client::RpcClient,
) -> report::cost_report::NetworkConfig {
    let mut snapshot = xdr_helper::begin_snapshot("", 0);
    for setting in [
        rpc::config::ConfigSettingId::ContractComputeV0,
        rpc::config::ConfigSettingId::ContractLedgerCostV0,
        rpc::config::ConfigSettingId::ContractBandwidthV0,
    ] {
        if let Ok(raw) = rpc::config::fetch_config_setting(client, setting).await {
            if let Ok(entry) = xdr_helper::decode_config_entry_xdr(&raw.config_xdr) {
                xdr_helper::apply_config_entry(&mut snapshot, entry);
            }
        }
    }
    report::cost_report::NetworkConfig::from_snapshot(&snapshot)
}

/// `estimate` command: simulate a single invocation and print cost report.
///
/// All RPC traffic (simulation and fee-rate fetches) goes through one
/// `RpcClient`, which deduplicates identical requests — the same method with
/// the same params — so a repeated WASM-upload envelope (when `--fn` is
/// omitted) or identical fee-rate fetches transmit at most once.
#[allow(clippy::too_many_lines)]
async fn cmd_estimate(
    wasm_path: &str,
    network: &str,
    rpc_url: Option<&str>,
    rpc_fallback_url: Option<&str>,
    contract_id: Option<&str>,
    fn_name: Option<&str>,
    args: &[String],
    cache_ttl: Option<&str>,
    clear_cache: bool,
    history: bool,
    format: &str,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    precision: u32,
    extra_headers: &[String],
) -> error::AppResult<()> {
    let json_flag = format == "json";
    let table_mode = format == "table";
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
        // With `--clear-cache`, wipe every cached estimate for this network
        // before anything else runs, so the `--cache-ttl` lookup and the
        // simulation below both start from an empty slate. Human-readable
        // table output gets the announcement on stdout; machine formats
        // (json/csv/markdown) keep their stdout clean and use stderr.
        if clear_cache {
            let cleared = cache::clear_cache(network)?;
            let message = format!("Cleared {cleared} cached estimate(s) for {network}.");
            if table_mode {
                println!("{message}");
            } else {
                eprintln!("{message}");
            }
        }

        info!("loading WASM");
        let wasm_info = wasm::parser::load_wasm(std::path::Path::new(wasm_path))?;
        debug!(functions = wasm_info.functions.len(), has_spec = wasm_info.has_spec, "WASM loaded");

        let wasm_hash = hex::encode(sha2::Sha256::digest(&wasm_info.bytes));
        let function_name = fn_name.unwrap_or("(wasm upload)");

        // Show the hash before anything else — the user can verify they are
        // simulating the intended file before any RPC traffic is sent.
        // Only the human-readable table mode gets this header; machine
        // formats (json/csv/markdown) emit their own self-contained output.
        if table_mode {
            println!("WASM SHA-256: {wasm_hash}");
        }

        // With --cache-ttl, reuse a still-fresh cached estimate and skip the
        // (expensive) simulation entirely.
        let ttl_secs = cache_ttl.map(parse_interval_secs);
        if let Some(fresh) =
            fresh_cached_estimate(&wasm_hash, &function_name, args, ttl_secs)?
        {
            let ttl_secs = ttl_secs.unwrap_or_default();
            info!(ttl_secs, function = %function_name, "cache hit — reusing fresh estimate");
            print_cached_estimate(&fresh, ttl_secs, json_flag, precision);
            return Ok(());
        }

        let endpoint = rpc::client::resolve_endpoint(network, rpc_url)?;
        let client = rpc::client::RpcClient::with_fallback_headers(
            &endpoint,
            rpc_fallback_url,
            rps,
            std::time::Duration::from_secs(timeout),
            max_retries,
            extra_headers,
        );

        let sc_vals: Vec<stellar_xdr::ScVal> = args
            .iter()
            .map(|a| xdr_helper::parse_arg_scval(a))
            .collect();
        debug!(arg_count = sc_vals.len(), "parsed arguments");

        let tx_xdr =
            xdr_helper::build_simulation_tx_envelope(&wasm_info.bytes, contract_id, fn_name, &sc_vals)?;

        xdr_helper::validate_args_against_spec(fn_name, args, &wasm_info.functions)?;
        debug!(arg_count = args.len(), "validated arguments against contract spec");

        let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_xdr);
        debug!(tx_xdr_len = tx_xdr.len(), "built simulation tx envelope");

        // Fail fast on a misconfigured --rpc-url or down node (#55): validate
        // the endpoint is reachable and healthy before running any simulation.
        // Local argument errors above are reported first; this guards the
        // (potentially expensive) simulateTransaction call itself.
        client.health_check().await?;

        // Time the simulateTransaction round-trip so the report can flag
        // slow RPC endpoints. Includes any retries performed by the client.
        let rpc_start = std::time::Instant::now();
        let response = rpc::simulate::simulate_transaction(&client, &tx_b64).await?;
        let rpc_latency_ms = rpc_start.elapsed().as_millis() as u64;

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
            precision,
        );

        let mut report = report::cost_report::CostReport {
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
            rpc_latency_ms,
            rates: Some(fee_rates),
            warnings: Vec::new(),
            history: None,
        };

        // Warn when any resource approaches the network's protocol limits
        // (#322). The config entries are already fetched for the fee rates;
        // the RPC client deduplicates those requests so this is free.
        let network_config = fetch_network_config(&client).await;
        report.warnings = report::cost_report::check_resource_limits(&report, &network_config);
        if !report.warnings.is_empty() {
            warn!(
                count = report.warnings.len(),
                "resource usage approaching network limits"
            );
        }

        // Load up to 5 previous runs for the trend table (#321). This must
        // happen before `save_estimate` so the current run is not counted as
        // its own history entry.
        if history {
            let runs = cache::load_estimate_history(
                &wasm_hash,
                function_name,
                cache::DEFAULT_HISTORY_LIMIT,
            )?;
            let historical: Vec<report::cost_report::HistoricalRun> = runs
                .iter()
                .map(|run| report::cost_report::HistoricalRun {
                    timestamp: run.timestamp.clone(),
                    ledger: run.ledger,
                    cpu_instructions: run.cpu_instructions,
                    total_stroops: run.total_stroops,
                })
                .collect();
            let entries = report::cost_report::build_history_entries(
                report.fee.total_stroops,
                precision,
                &historical,
            );
            info!(entries = entries.len(), "loaded cost history");
            report.history = Some(entries);
        }

        let _ = cache::save_estimate(
            &wasm_hash,
            function_name,
            args,
            network,
            latest_ledger,
            fee.total_stroops,
            cpu_instructions,
            memory_bytes,
            Some(rpc_latency_ms),
            true,
        );
        info!(total_stroops = fee.total_stroops, total_xlm = %fee.total_xlm, "estimate complete");

        match formatter_by_name(format) {
            Some(formatter) => println!("{}", formatter.format(&report)),
            None => println!("{}", TableFormatter.format(&report)),
        }

        Ok(())
    }
    .instrument(span)
    .await
}

/// Header row for `estimate-all --format csv` (#324).
const ESTIMATE_ALL_CSV_HEADER: &str = "Function,Status,CPU_Instructions,RAM_Bytes,Read_Entries,Write_Entries,Read_Bytes,Write_Bytes,Total_Fee_Stroops,Total_Fee_XLM";

/// Escape a single CSV field per RFC 4180.
///
/// A field is wrapped in double-quotes when it contains a comma, a
/// double-quote, or a line break; embedded double-quotes are doubled. Plain
/// values are emitted verbatim.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Machine-readable status label for a result (`ok` / `skipped` / `error`).
fn status_label(result: &EstimateAllResult) -> &'static str {
    match result.status {
        EstimateAllStatus::Ok => "ok",
        EstimateAllStatus::Skipped => "skipped",
        EstimateAllStatus::Error => "error",
    }
}

/// Render `estimate-all` results as RFC 4180-compliant CSV (#324).
///
/// The column order is fixed and documented by [`ESTIMATE_ALL_CSV_HEADER`].
/// Functions that were skipped or failed emit `0`/empty resource and fee
/// fields, so the row count always matches the enumerated function count.
/// The returned string ends with a trailing newline.
#[must_use]
pub fn format_estimate_all_csv(results: &[EstimateAllResult]) -> String {
    let mut out = String::from(ESTIMATE_ALL_CSV_HEADER);
    out.push('\n');
    for result in results {
        let fee = result.fee.as_ref();
        let fields = [
            csv_escape(&result.function),
            status_label(result).to_string(),
            result.cpu_instructions.unwrap_or(0).to_string(),
            result.memory_bytes.unwrap_or(0).to_string(),
            result.read_entries.unwrap_or(0).to_string(),
            result.write_entries.unwrap_or(0).to_string(),
            result.read_bytes.unwrap_or(0).to_string(),
            result.write_bytes.unwrap_or(0).to_string(),
            fee.map(|f| f.total_stroops).unwrap_or(0).to_string(),
            csv_escape(fee.map(|f| f.total_xlm.as_str()).unwrap_or("")),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// Escape a value for inclusion in a Markdown table cell.
///
/// Pipes would otherwise split the cell, and line breaks would break the row,
/// so both are neutralised.
fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// Render `estimate-all` results as a GitHub-flavored Markdown document (#325).
///
/// Emits a compact `| Function | CPU | Fee (XLM) |` summary table, a run
/// summary with fee statistics, and one collapsible `<details>` section per
/// function with the full resource and fee breakdown — suitable for posting
/// as an automated pull-request comment. The whole document is plain text (no
/// terminal escape codes), so it renders natively on GitHub.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn format_estimate_all_markdown(results: &[EstimateAllResult]) -> String {
    let ok = results
        .iter()
        .filter(|r| r.status == EstimateAllStatus::Ok)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.status == EstimateAllStatus::Skipped)
        .count();
    let errored = results
        .iter()
        .filter(|r| r.status == EstimateAllStatus::Error)
        .count();
    let fees: Vec<i64> = results
        .iter()
        .filter_map(|r| r.fee.as_ref().map(|f| f.total_stroops))
        .collect();

    let mut out = String::from("## Cost Estimate Summary\n\n");
    out.push_str(&format!(
        "- **Functions:** {} ({} ok, {} skipped, {} error)\n",
        results.len(),
        ok,
        skipped,
        errored
    ));
    if let Some(range) = report::fee_calc::fee_range(&fees) {
        out.push_str(&format!(
            "- **Fees (stroops):** min {} · avg {} · max {}\n",
            range.min_stroops, range.avg_stroops, range.max_stroops
        ));
    }

    // Compact summary table: exactly the columns called out in #325.
    out.push_str("\n| Function | CPU | Fee (XLM) |\n| --- | --- | --- |\n");
    for result in results {
        let cpu = result
            .cpu_instructions
            .map_or_else(|| "—".to_string(), |v| v.to_string());
        let fee = result
            .fee
            .as_ref()
            .map_or_else(|| "—".to_string(), |f| f.total_xlm.clone());
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            markdown_escape(&result.function),
            cpu,
            fee
        ));
    }

    // Collapsible per-function detail.
    out.push_str("\n### Per-function details\n\n");
    for result in results {
        out.push_str(&format!(
            "<details>\n<summary><code>{}</code> — {}</summary>\n\n",
            markdown_escape(&result.function),
            status_label(result)
        ));
        match result.status {
            EstimateAllStatus::Ok => {
                out.push_str("| Metric | Value |\n| --- | --- |\n");
                out.push_str(&format!(
                    "| CPU Instructions | {} |\n",
                    result.cpu_instructions.unwrap_or(0)
                ));
                out.push_str(&format!(
                    "| Memory Bytes | {} |\n",
                    result.memory_bytes.unwrap_or(0)
                ));
                out.push_str(&format!(
                    "| Read Entries | {} |\n",
                    result.read_entries.unwrap_or(0)
                ));
                out.push_str(&format!(
                    "| Write Entries | {} |\n",
                    result.write_entries.unwrap_or(0)
                ));
                out.push_str(&format!(
                    "| Read Bytes | {} |\n",
                    result.read_bytes.unwrap_or(0)
                ));
                out.push_str(&format!(
                    "| Write Bytes | {} |\n",
                    result.write_bytes.unwrap_or(0)
                ));
                out.push_str(&format!("| Ledger | {} |\n", result.ledger.unwrap_or(0)));
                if let Some(fee) = &result.fee {
                    out.push_str(&format!("| Total Fee | {} stroops |\n", fee.total_stroops));
                    out.push_str(&format!("| Total Fee | {} XLM |\n", fee.total_xlm));
                    out.push_str(&format!(
                        "| Non-refundable | {} stroops |\n",
                        fee.non_refundable_stroops
                    ));
                    out.push_str(&format!(
                        "| Refundable | {} stroops |\n",
                        fee.refundable_stroops
                    ));
                }
                if !result.warnings.is_empty() {
                    out.push_str("\n> **Resource limit warnings**\n");
                    for warning in &result.warnings {
                        out.push_str(&format!("> - {}\n", warning.message));
                    }
                }
            }
            EstimateAllStatus::Skipped => {
                if let Some(reason) = &result.reason {
                    out.push_str(&format!("Skipped: {}\n", markdown_escape(reason)));
                }
            }
            EstimateAllStatus::Error => {
                if let Some(error) = &result.error {
                    out.push_str(&format!("Error: {}\n", markdown_escape(error)));
                }
            }
        }
        out.push_str("\n</details>\n\n");
    }

    out
}

/// `estimate-all` command: enumerate all functions and estimate each.
///
/// Every function shares a single deduplicating `RpcClient`. Batch runs that
/// hit the same request twice — the shared WASM-upload path when a function
/// envelope is built against an undeployed contract, or identical fee-rate
/// lookups — transmit each distinct `(method, params)` pair only once.
#[allow(clippy::too_many_lines)]
async fn cmd_estimate_all(
    wasm_path: &str,
    network: &str,
    rpc_url: Option<&str>,
    rpc_fallback_url: Option<&str>,
    contract_id: Option<&str>,
    format: &str,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    precision: u32,
    extra_headers: &[String],
) -> error::AppResult<()> {
    use tracing::Instrument;
    use tracing::info_span;

    let span = info_span!("cmd_estimate_all", wasm_path, network);
    async {
        let wasm_info = wasm::parser::load_wasm(std::path::Path::new(wasm_path))?;

        // Confirm the exact file being estimated up front — printed before any
        // endpoint resolution or simulation, so the hash is visible even when
        // the network cannot be reached.
        use sha2::Digest;
        let wasm_hash = hex::encode(sha2::Sha256::digest(&wasm_info.bytes));

        // `table` is the only human-readable format; everything else must emit
        // self-contained machine output with no progress noise on stdout.
        let text_mode = format == "table";
        let quiet = format != "table";
        if text_mode {
            println!("WASM SHA-256: {wasm_hash}");
            println!();
            println!("{}", wasm::parser::format_module_metadata(&wasm_info));
            println!();
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
            println!("{}", wasm::parser::format_contract_meta(&wasm_info.contract_meta));
            if contract_id.is_none() {
                println!(
                    "Note: pass --id <contract-id> to simulate each function against a deployed contract."
                );
            }
        }

        let endpoint = rpc::client::resolve_endpoint(network, rpc_url)?;
        let client = rpc::client::RpcClient::with_fallback_headers(
            &endpoint,
            rpc_fallback_url,
            rps,
            std::time::Duration::from_secs(timeout),
            max_retries,
            extra_headers,
        );

        // Validate the RPC endpoint is reachable before running a full batch
        // of simulations (#55): fail fast up front rather than after each
        // function's simulation times out.
        client.health_check().await?;

        // Fee rates itemize the per-function breakdown for JSON/CSV/Markdown.
        // They share the same three config requests as the resource limits
        // below, so the deduplicating client transmits them only once.
        let fee_rates = if quiet {
            Some(fetch_fee_rates(&client).await)
        } else {
            None
        };

        // Network resource limits for the 80% warnings (#322), fetched once
        // for the whole batch.
        let network_config = fetch_network_config(&client).await;

        let mut results: Vec<EstimateAllResult> = Vec::new();
        let total = wasm_info.functions.len();
        debug!(total, "enumerated functions");

        for (i, fn_info) in wasm_info.functions.iter().enumerate() {
            if text_mode {
                println!("[{}/{}] {}", i + 1, total, fn_info.name);
            }
            let result = estimate_all_function(
                &client,
                &wasm_info,
                fn_info,
                contract_id,
                &wasm_hash,
                network,
                quiet,
                fee_rates.as_ref(),
                &network_config,
                precision,
            )
            .await?;
            results.push(result);
        }

        // Aggregate fee range across every successfully estimated function
        // (#223): min/max/average in stroops (and XLM) for the whole batch.
        // Machine formats embed their own summary instead.
        let fees: Vec<i64> = results
            .iter()
            .filter_map(|r| r.fee.as_ref().map(|f| f.total_stroops))
            .collect();
        emit_fee_range_summary(&fees, quiet, precision);

        match format {
            "json" => println!("{}", serde_json::to_string_pretty(&results)?),
            "csv" => print!("{}", format_estimate_all_csv(&results)),
            "markdown" => print!("{}", format_estimate_all_markdown(&results)),
            _ => {}
        }

        Ok(())
    }
    .instrument(span)
    .await
}

/// Emit the aggregate fee-range summary for an `estimate-all` batch (#223).
///
/// In human mode it is printed as three lines. The fee range is intentionally
/// omitted from the structured JSON array (which already contains a per-function
/// `fee` record); callers can derive min/max/average from those records.
fn emit_fee_range_summary(fees: &[i64], json_flag: bool, precision: u32) {
    if json_flag {
        return;
    }
    let Some(range) = report::fee_calc::fee_range(fees) else {
        println!("No functions estimated; no fee range to report.");
        return;
    };

    println!();
    println!("Fee range across {} function(s):", range.count);
    println!(
        "  min: {} stroops ({})",
        range.min_stroops,
        report::fee_calc::stroops_to_xlm(range.min_stroops, precision)
    );
    println!(
        "  max: {} stroops ({})",
        range.max_stroops,
        report::fee_calc::stroops_to_xlm(range.max_stroops, precision)
    );
    println!(
        "  avg: {} stroops ({})",
        range.avg_stroops,
        report::fee_calc::stroops_to_xlm(range.avg_stroops, precision)
    );
}

/// Estimates one exported function against the network, returning a well-typed
/// [`EstimateAllResult`].
///
/// The returned record is always built and handed back to the caller, which
/// collects every record into a uniform array. When `quiet` is false (table
/// mode) progress lines are printed to stdout/stderr; machine formats keep the
/// stream clean.
#[allow(clippy::too_many_lines)]
async fn estimate_all_function(
    client: &rpc::client::RpcClient,
    wasm_info: &wasm::parser::WasmInfo,
    fn_info: &wasm::parser::FunctionInfo,
    contract_id: Option<&str>,
    wasm_hash: &str,
    network: &str,
    quiet: bool,
    fee_rates: Option<&report::fee_calc::FeeRates>,
    network_config: &report::cost_report::NetworkConfig,
    precision: u32,
) -> error::AppResult<EstimateAllResult> {
    use tracing::{Instrument, debug, info_span};

    let span =
        info_span!("estimate_all_function", fn = %fn_info.name, param_count = fn_info.param_count);
    async {
        if fn_info.param_count > 0 {
            let reason = format!("needs --fn/--arg ({} param(s))", fn_info.param_count);
            debug!(reason, "skipping function");
            if !quiet {
                println!("── Estimating '{}' ── Skipped: {reason}", fn_info.name);
            }
            return Ok(EstimateAllResult::skipped(&fn_info.name, reason));
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
                if !quiet {
                    eprintln!("── Estimating '{}' ── Skipped: {e}", fn_info.name);
                }
                return Ok(EstimateAllResult::skipped(&fn_info.name, e.to_string()));
            }
        };
        let tx_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &tx_xdr);
        debug!(tx_xdr_len = tx_xdr.len(), "built tx envelope");

        let sim_start = std::time::Instant::now();
        match rpc::simulate::simulate_transaction(client, &tx_b64).await {
            Ok(resp) => {
                let duration_ms = Some(sim_start.elapsed().as_millis() as u64);
                if missing_simulation_data(&resp) {
                    let msg = "simulation returned no cost data and no latest ledger — check --id and the RPC endpoint";
                    debug!(msg, "simulation missing data");
                    if !quiet {
                        eprintln!("── Estimating '{}' ── Error: {msg}", fn_info.name);
                    }
                    return Ok(EstimateAllResult::errored(&fn_info.name, msg));
                }

                let (cpu, mem, read_entries, write_entries, read_bytes, write_bytes) =
                    response_resources(&resp)?;
                let total_fee = rpc::simulate::parse_resource_fee(&resp.min_resource_fee)
                    .unwrap_or(None)
                    .or(rpc::simulate::parse_transaction_data_resource_fee(
                        &resp.transaction_data,
                    )?)
                    .unwrap_or(0);
                let xlm = report::fee_calc::stroops_to_xlm(total_fee, precision);
                let ledger: u32 = resp
                    .latest_ledger
                    .and_then(|l| u32::try_from(l).ok())
                    .unwrap_or(0);

                debug!(cpu, mem, total_fee, ledger, "simulation complete");

                let _ = cache::save_estimate(
                    wasm_hash,
                    &fn_info.name,
                    &[],
                    network,
                    ledger,
                    total_fee,
                    cpu,
                    mem,
                    duration_ms,
                    true,
                );

                // Itemize the fee breakdown only when we have the network's fee
                // rates (JSON mode). Otherwise emit a minimal breakdown with just
                // the authoritative total so the record shape stays consistent.
                let fee = match fee_rates {
                    Some(rates) => report::fee_calc::compute_fee_breakdown(
                        total_fee,
                        cpu,
                        read_entries,
                        write_entries,
                        read_bytes,
                        tx_xdr.len() as u32,
                        *rates,
                        precision,
                    ),
                    None => report::fee_calc::FeeBreakdown {
                        non_refundable_stroops: 0,
                        refundable_stroops: 0,
                        cpu_fee_stroops: 0,
                        storage_fee_stroops: 0,
                        bandwidth_fee_stroops: 0,
                        total_stroops: total_fee,
                        total_xlm: xlm.clone(),
                    },
                };

                if !quiet {
                    println!(
                        "CPU: {cpu} insns | Mem: {mem} bytes | Fee: {total_fee} stroops ({xlm} XLM) | Ledger: {ledger}"
                    );
                }

                // Warn when any resource approaches the network's protocol
                // limits (#322).
                let warnings = report::cost_report::check_resource_limits_raw(
                    cpu,
                    read_entries,
                    write_entries,
                    read_bytes,
                    write_bytes,
                    tx_xdr.len() as u32,
                    network_config,
                );

                Ok(EstimateAllResult {
                    function: fn_info.name.clone(),
                    status: EstimateAllStatus::Ok,
                    reason: None,
                    error: None,
                    wasm_hash: Some(wasm_hash.to_string()),
                    network: Some(network.to_string()),
                    ledger: Some(ledger),
                    cpu_instructions: Some(cpu),
                    memory_bytes: Some(mem),
                    read_entries: Some(read_entries),
                    write_entries: Some(write_entries),
                    read_bytes: Some(read_bytes),
                    write_bytes: Some(write_bytes),
                    tx_size: Some(tx_xdr.len() as u32),
                    fee: Some(fee),
                    warnings,
                })
            }
            Err(e) => {
                debug!(error = %e, "simulation failed");
                if !quiet {
                    eprintln!("Skipped — simulation failed: {e}");
                }
                Ok(EstimateAllResult::errored(&fn_info.name, e.to_string()))
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
    println!(
        "{}",
        wasm::parser::format_contract_meta(&wasm_info.contract_meta)
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
        "contract_meta": {
            "name": wasm_info.contract_meta.name,
            "version": wasm_info.contract_meta.version,
            "description": wasm_info.contract_meta.description,
            "entries": wasm_info.contract_meta.entries.iter().map(|(key, value)| {
                serde_json::json!({ "key": key, "value": value })
            }).collect::<Vec<_>>(),
        },
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
    rpc_fallback_url: Option<&str>,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<config_snapshot::model::ConfigSnapshot> {
    use tracing::Instrument;
    use tracing::{debug, info_span};

    let span = info_span!("fetch_config_snapshot", network);
    async {
        let endpoint = rpc::client::resolve_endpoint(network, None)?;
        let client = rpc::client::RpcClient::with_fallback_headers(
            &endpoint,
            rpc_fallback_url,
            rps,
            std::time::Duration::from_secs(timeout),
            max_retries,
            extra_headers,
        );
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
    rpc_fallback_url: Option<&str>,
    out_path: Option<&str>,
    json_flag: bool,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<()> {
    use tracing::Instrument;
    use tracing::info_span;

    let span = info_span!("cmd_config_snapshot", network);
    async {
        info!("taking config snapshot");
        let snapshot = fetch_config_snapshot(
            network,
            rpc_fallback_url,
            rps,
            timeout,
            max_retries,
            extra_headers,
        )
        .await?;

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

/// `config snapshot list` command: list all saved snapshots for a network.
fn cmd_config_snapshot_list(network: &str) -> error::AppResult<()> {
    let snapshots = config_snapshot::store::list_snapshots(network)?;

    if snapshots.is_empty() {
        println!("No snapshots found for network '{network}'.");
        return Ok(());
    }

    println!("Config snapshots for network '{network}':");
    for path in snapshots {
        let path_str = path.to_string_lossy();
        let snapshot = config_snapshot::store::load_snapshot_from_path(&path_str)?;
        println!(
            "  {}  ledger {}  {}",
            snapshot.timestamp, snapshot.ledger, path_str
        );
    }
    Ok(())
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
async fn cmd_config_diff(
    network: &str,
    rpc_fallback_url: Option<&str>,
    against_path: Option<&str>,
    summary: bool,
    json_flag: bool,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<()> {
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

        let new_snapshot = fetch_config_snapshot(
            network,
            rpc_fallback_url,
            rps,
            timeout,
            max_retries,
            extra_headers,
        )
        .await?;

        let diff = config_snapshot::diff::diff_snapshots(&old_snapshot, &new_snapshot);
        debug!(
            change_count = diff.changes.len(),
            has_pricing = diff.has_pricing_changes,
            "diff computed"
        );

        if json_flag {
            // Collect stale estimates for inclusion in JSON output.
            let stale: Vec<cache::CachedEstimate> = cache::list_cached_estimates(network)
                .map(|estimates| {
                    cache::find_stale_estimates(&estimates, new_snapshot.ledger)
                        .into_iter()
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let json_output = serde_json::json!({
                "diff": diff,
                "stale_estimates": stale,
            });
            println!("{}", serde_json::to_string_pretty(&json_output)?);
        } else if summary {
            println!("{}", config_snapshot::diff::format_diff_summary(&diff));
        } else {
            println!("{}", config_snapshot::diff::format_diff(&diff));
        }

        if upgrade_detected(&diff) {
            match config_snapshot::store::save_snapshot(&new_snapshot, None) {
                Ok(path) => {
                    info!(path = %path.display(), "auto-saved post-upgrade snapshot");
                    if !json_flag && !summary {
                        println!(
                            "  Protocol upgrade detected — new config auto-saved to {}",
                            path.display()
                        );
                    }
                }
                Err(e) => {
                    warn!(error = %e, "could not auto-save post-upgrade snapshot");
                    if !json_flag && !summary {
                        eprintln!("  Warning: could not auto-save post-upgrade snapshot: {e}");
                    }
                }
            }
        }

        if !json_flag && !summary {
            print_stale_estimates(network, new_snapshot.ledger);
        }

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
fn print_cached_estimate(
    fresh: &cache::CachedEstimate,
    ttl_secs: u64,
    json_flag: bool,
    precision: u32,
) {
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
            report::fee_calc::stroops_to_xlm(fresh.total_stroops, precision),
            fresh.cpu_instructions,
            fresh.memory_bytes,
            fresh.ledger,
        );
    }
}

/// `config validate` command: check all stored snapshots for integrity.
fn cmd_config_validate(network: &str) -> error::AppResult<()> {
    let statuses = config_snapshot::store::validate_all_snapshots(network)?;

    if statuses.is_empty() {
        println!("No snapshots found for network '{network}'.");
        return Ok(());
    }

    let total = statuses.len();
    let invalid: Vec<_> = statuses.iter().filter(|s| !s.valid).collect();

    println!("Validated {total} snapshot(s) for network '{network}'.");

    if invalid.is_empty() {
        println!("All snapshots are valid.");
    } else {
        println!("{}/{} snapshot(s) failed validation:", invalid.len(), total);
        for status in &invalid {
            println!(
                "  - {}: {}",
                status.filename,
                status.error.as_deref().unwrap_or("unknown error")
            );
        }
        std::process::exit(1);
    }

    Ok(())
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
async fn watch_poll_once(
    network: &str,
    rpc_fallback_url: Option<&str>,
    first: &mut bool,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<()> {
    use tracing::{debug, warn};

    let snapshot_result = fetch_config_snapshot(
        network,
        rpc_fallback_url,
        rps,
        timeout,
        max_retries,
        extra_headers,
    )
    .await;

    match snapshot_result {
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
async fn cmd_watch(
    network: &str,
    rpc_fallback_url: Option<&str>,
    interval: &str,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<()> {
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
                let _ = watch_poll_once(
                    network,
                    rpc_fallback_url,
                    &mut first,
                    rps,
                    timeout,
                    max_retries,
                    extra_headers,
                )
                .await;
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            } => {}
        }
    }
}

/// `cache verify` command: check every cache entry parses as valid JSON.
/// `cache stats` command: show cache health overview.
///
/// Prints total entries, disk usage, age (oldest/newest), and per-network
/// breakdown. Useful for checking whether the cache is being populated and
/// how much disk space it consumes.
///
/// # Network calls
/// None — pure SQLite I/O.
fn cmd_cache_stats() -> error::AppResult<()> {
    let stats = cache::cache_stats()?;

    if stats.total_entries == 0 {
        println!("Cache is empty — no cached estimates.");
        return Ok(());
    }

    println!("Cache Statistics");
    println!("================");
    println!("  Total entries:  {}", stats.total_entries);
    println!("  Disk usage:     {}", format_bytes(stats.disk_bytes));
    println!(
        "  Oldest entry:   {}",
        stats.oldest_entry.as_deref().unwrap_or("n/a")
    );
    println!(
        "  Newest entry:   {}",
        stats.newest_entry.as_deref().unwrap_or("n/a")
    );

    if !stats.per_network.is_empty() {
        println!("\nPer-network breakdown:");
        for (network, count) in &stats.per_network {
            println!(
                "  {network}: {count} entr{}",
                if *count == 1 { "y" } else { "ies" }
            );
        }
    }

    Ok(())
}

/// Format a byte count as a human-readable string (KB, MB, GB).
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

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

/// `cache clear` command: delete every cached estimate for a network.
///
/// Defaults to `testnet`; pass `--network` to target another network. Only
/// entries recorded for that network are removed — other networks' entries
/// are untouched.
///
/// # Network calls
/// None — pure SQLite I/O.
fn cmd_cache_clear(network: &str) -> error::AppResult<()> {
    let cleared = cache::clear_cache(network)?;
    println!("Cleared {cleared} cached estimate(s) for {network}.");
    Ok(())
}

/// `cache query` command: list cached estimates matching the given filters.
///
/// Prints a table (or JSON when `--json` is passed). An empty result prints a
/// friendly message instead of an empty table.
///
/// # Network calls
/// None — pure file I/O.
fn cmd_cache_query(
    network: &str,
    function: Option<&str>,
    wasm_hash: Option<&str>,
    min_stroops: Option<i64>,
    max_stroops: Option<i64>,
    from: Option<&str>,
    to: Option<&str>,
    json: bool,
) -> error::AppResult<()> {
    let filter = cache::QueryFilter {
        function: function.map(str::to_string),
        wasm_hash: wasm_hash.map(str::to_string),
        min_stroops,
        max_stroops,
        from: from.map(str::to_string),
        to: to.map(str::to_string),
    };

    let estimates = cache::query_estimates(network, &filter)?;

    if estimates.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No cached estimates match the query.");
        }
        return Ok(());
    }

    if json {
        let json = serde_json::json!(estimates);
        println!("{json}");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec![
        "Function",
        "Network",
        "WASM Hash",
        "Stroops",
        "Ledger",
        "Timestamp",
    ]);
    for e in &estimates {
        table.add_row(vec![
            Cell::new(e.function.as_str()),
            Cell::new(e.network.as_str()),
            Cell::new(e.wasm_hash.as_str()),
            Cell::new(e.total_stroops),
            Cell::new(e.ledger),
            Cell::new(e.timestamp.as_str()),
        ]);
    }
    println!("{table}");

    Ok(())
}

/// `cache export` command: print or save every cached estimate as a JSON array.
fn cmd_cache_export(out_path: Option<&str>) -> error::AppResult<()> {
    let estimates = cache::export_cached_estimates()?;
    let json = serde_json::to_string_pretty(&estimates)?;

    if let Some(out_path) = out_path {
        std::fs::write(out_path, json)?;
        println!(
            "Exported {} cache entr{} to {}.",
            estimates.len(),
            if estimates.len() == 1 { "y" } else { "ies" },
            out_path
        );
    } else {
        println!("{json}");
    }

    Ok(())
}

/// `cache warm` command: pre-populate cache by estimating every exported function.
async fn cmd_cache_warm(
    wasm_path: &str,
    network: &str,
    rpc_url: Option<&str>,
    rpc_fallback_url: Option<&str>,
    contract_id: Option<&str>,
    json_flag: bool,
    rps: Option<u64>,
    timeout: u64,
    max_retries: usize,
    extra_headers: &[String],
) -> error::AppResult<()> {
    let fmt = if json_flag { "json" } else { "table" };
    cmd_estimate_all(
        wasm_path,
        network,
        rpc_url,
        rpc_fallback_url,
        contract_id,
        fmt,
        rps,
        timeout,
        max_retries,
        7,
        extra_headers,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::EstimateAllResult;
    use super::EstimateAllStatus;
    use super::format_estimate_all_csv;
    use super::format_estimate_all_markdown;
    use super::parse_interval_secs;
    use super::upgrade_detected;
    use super::wasm_info_json;
    use soroban_cost_estimator::config_snapshot::diff;
    use soroban_cost_estimator::config_snapshot::model::{
        ConfigSnapshot, ContractComputeV0, ContractLedgerCostV0,
    };
    use soroban_cost_estimator::wasm::parser::{ContractMeta, FunctionInfo, ParamInfo, WasmInfo};

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
            contract_meta: ContractMeta::default(),
            functions: vec![FunctionInfo {
                name: "increment".to_string(),
                param_count: 1,
                result_count: 1,
                params: vec![ParamInfo {
                    name: "step".to_string(),
                    type_name: "I64".to_string(),
                    type_def: stellar_xdr::ScSpecTypeDef::I64,
                }],
            }],
            start_function: None,
            memories: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
        };
        let value = wasm_info_json("/tmp/contract.wasm", &info, "deadbeef");

        assert_eq!(value["path"], "/tmp/contract.wasm");
        assert_eq!(value["size"], 44);
        assert_eq!(value["sha256"], "deadbeef");
        assert_eq!(value["has_spec"], true);
        assert_eq!(value["contract_meta"]["name"], serde_json::Value::Null);
        assert_eq!(value["contract_meta"]["entries"], serde_json::json!([]));
        assert_eq!(value["functions"][0]["name"], "increment");
        assert_eq!(value["functions"][0]["params"][0]["name"], "step");
        assert_eq!(value["functions"][0]["params"][0]["type"], "I64");
    }

    #[test]
    fn test_estimate_all_result_json_shape() {
        let results = vec![
            EstimateAllResult {
                function: "inc".to_string(),
                status: EstimateAllStatus::Ok,
                reason: None,
                error: None,
                wasm_hash: Some("deadbeef".to_string()),
                network: Some("testnet".to_string()),
                ledger: Some(10),
                cpu_instructions: Some(100),
                memory_bytes: Some(0),
                read_entries: Some(1),
                write_entries: Some(1),
                read_bytes: Some(0),
                write_bytes: Some(10),
                tx_size: Some(50),
                fee: Some(soroban_cost_estimator::report::fee_calc::FeeBreakdown {
                    non_refundable_stroops: 1,
                    refundable_stroops: 2,
                    cpu_fee_stroops: 1,
                    storage_fee_stroops: 0,
                    bandwidth_fee_stroops: 0,
                    total_stroops: 3,
                    total_xlm: "0.0000003".to_string(),
                }),
                warnings: Vec::new(),
            },
            EstimateAllResult::skipped("needs_args", "needs --fn/--arg (1 param(s))"),
            EstimateAllResult::errored("bad", "boom"),
        ];
        let value: serde_json::Value = serde_json::to_value(&results).unwrap();

        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 3);

        // ok entry carries status + resources + fee, but no reason/error.
        let ok = &value[0];
        assert_eq!(ok["status"], "ok");
        assert_eq!(ok["function"], "inc");
        assert_eq!(ok["cpu_instructions"], 100);
        assert_eq!(ok["fee"]["total_stroops"], 3);
        assert!(ok.get("reason").is_none());
        assert!(ok.get("error").is_none());

        // skipped entry carries status + reason, omits resources/fee.
        let skipped = &value[1];
        assert_eq!(skipped["status"], "skipped");
        assert_eq!(skipped["reason"], "needs --fn/--arg (1 param(s))");
        assert!(skipped.get("cpu_instructions").is_none());
        assert!(skipped.get("fee").is_none());

        // error entry carries status + error.
        let errored = &value[2];
        assert_eq!(errored["status"], "error");
        assert_eq!(errored["error"], "boom");
    }

    // ── estimate-all CSV export (#324) ───────────────────────────────

    fn csv_fixture() -> Vec<EstimateAllResult> {
        vec![
            EstimateAllResult {
                function: "inc,with\"quotes\"".to_string(),
                status: EstimateAllStatus::Ok,
                reason: None,
                error: None,
                wasm_hash: Some("deadbeef".to_string()),
                network: Some("testnet".to_string()),
                ledger: Some(10),
                cpu_instructions: Some(100),
                memory_bytes: Some(5),
                read_entries: Some(1),
                write_entries: Some(2),
                read_bytes: Some(3),
                write_bytes: Some(4),
                tx_size: Some(50),
                fee: Some(soroban_cost_estimator::report::fee_calc::FeeBreakdown {
                    non_refundable_stroops: 1,
                    refundable_stroops: 2,
                    cpu_fee_stroops: 1,
                    storage_fee_stroops: 0,
                    bandwidth_fee_stroops: 0,
                    total_stroops: 3,
                    total_xlm: "0.0000003".to_string(),
                }),
                warnings: Vec::new(),
            },
            EstimateAllResult::skipped("needs_args", "needs --fn/--arg (1 param(s))"),
            EstimateAllResult::errored("bad", "boom"),
        ]
    }

    #[test]
    fn test_estimate_all_csv_header_and_rows() {
        let csv = format_estimate_all_csv(&csv_fixture());
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 4, "header + three function rows");
        assert_eq!(
            lines[0],
            "Function,Status,CPU_Instructions,RAM_Bytes,Read_Entries,Write_Entries,Read_Bytes,Write_Bytes,Total_Fee_Stroops,Total_Fee_XLM"
        );
    }

    #[test]
    fn test_estimate_all_csv_escapes_commas_and_quotes() {
        let csv = format_estimate_all_csv(&csv_fixture());
        let data = csv.lines().nth(1).expect("first data row");
        // Function name with a comma and embedded quotes is fully escaped.
        assert!(
            data.starts_with("\"inc,with\"\"quotes\"\"\""),
            "unescaped function name: {data}"
        );
        assert!(data.contains(",ok,100,5,1,2,3,4,3,0.0000003"));
    }

    #[test]
    fn test_estimate_all_csv_status_for_skipped_and_error() {
        let csv = format_estimate_all_csv(&csv_fixture());
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[2].starts_with("needs_args,skipped,"));
        assert!(lines[3].starts_with("bad,error,"));
    }

    #[test]
    fn test_estimate_all_csv_every_row_has_ten_fields() {
        // A minimal RFC 4180 parser: split on commas outside quotes.
        fn parse_row(line: &str) -> Vec<String> {
            let mut fields = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' if in_quotes => {
                        if chars.peek() == Some(&'"') {
                            current.push('"');
                            chars.next();
                        } else {
                            in_quotes = false;
                        }
                    }
                    '"' => in_quotes = true,
                    ',' if !in_quotes => {
                        fields.push(std::mem::take(&mut current));
                    }
                    other => current.push(other),
                }
            }
            fields.push(current);
            fields
        }

        let csv = format_estimate_all_csv(&csv_fixture());
        for line in csv.lines() {
            assert_eq!(
                parse_row(line).len(),
                10,
                "row should have 10 fields: {line}"
            );
        }
    }

    // ── estimate-all Markdown export (#325) ──────────────────────────

    #[test]
    fn test_estimate_all_markdown_summary_table() {
        let md = format_estimate_all_markdown(&csv_fixture());
        assert!(md.contains("| Function | CPU | Fee (XLM) |"));
        assert!(md.contains("| --- | --- | --- |"));
        assert!(md.contains("## Cost Estimate Summary"));
    }

    #[test]
    fn test_estimate_all_markdown_has_summary_stats_and_details() {
        let md = format_estimate_all_markdown(&csv_fixture());
        assert!(md.contains("**Functions:** 3 (1 ok, 1 skipped, 1 error)"));
        assert!(md.contains("<details>"));
        assert!(md.contains("</details>"));
        // One collapsible block per function.
        assert_eq!(md.matches("<details>").count(), 3);
        assert!(md.contains("Skipped: needs --fn/--arg"));
        assert!(md.contains("Error: boom"));
    }

    #[test]
    fn test_estimate_all_markdown_escapes_pipes_in_function_names() {
        let mut result = csv_fixture();
        result[0].function = "a|b".to_string();
        let md = format_estimate_all_markdown(&result);
        assert!(md.contains("a\\|b"), "pipe should be escaped: {md}");
        assert!(
            !md.contains("| `a|b` |"),
            "raw pipe must not split the cell"
        );
    }
}
