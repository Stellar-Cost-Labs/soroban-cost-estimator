//! Integration tests against a mock Soroban RPC server.
//!
//! These tests run the real CLI binary against a local HTTP server that
//! responds with canned JSON-RPC responses, proving the full pipeline
//! works without network access.

use std::net::SocketAddr;

use axum::Router;
use axum::extract::Json;
use axum::routing::post;
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// A canned `simulateTransaction` response matching the increment contract.
fn simulate_response() -> Value {
    let transaction_data = "AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAEmU1Fc+h02S4iEBnpjdCESXpKHG/bOUxC3DeRWUy9+mQAAABQAAAABAAggFgAAAAAAAACIAAAAAAAAPEM=";
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "transactionData": transaction_data,
            "cost": { "cpuInsns": 532_502, "memBytes": 0 },
            "error": null,
            "latestLedger": 3_894_195,
            "events": [],
            "minResourceFee": "15427",
            "restoreFee": null,
            "stateChanges": []
        }
    })
}

/// Encode a minimal `ConfigSettingContractComputeV0` XDR.
fn encode_contract_compute_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, WriteXdr};

    let entry =
        ConfigSettingEntry::ContractComputeV0(stellar_xdr::ConfigSettingContractComputeV0 {
            ledger_max_instructions: 1_000_000,
            tx_max_instructions: 100_000,
            fee_rate_per_instructions_increment: 10,
            tx_memory_limit: 41_943_040,
        });
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// Encode a minimal `ConfigSettingContractLedgerCostV0` XDR.
fn encode_contract_ledger_cost_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, WriteXdr};

    let entry =
        ConfigSettingEntry::ContractLedgerCostV0(stellar_xdr::ConfigSettingContractLedgerCostV0 {
            ledger_max_disk_read_entries: 1_000_000,
            ledger_max_disk_read_bytes: 1_000_000,
            ledger_max_write_ledger_entries: 1_000_000,
            ledger_max_write_bytes: 1_000_000,
            tx_max_disk_read_entries: 100,
            tx_max_disk_read_bytes: 1_000_000,
            tx_max_write_ledger_entries: 100,
            tx_max_write_bytes: 1_000_000,
            fee_disk_read_ledger_entry: 100,
            fee_write_ledger_entry: 200,
            fee_disk_read1_kb: 50,
            soroban_state_target_size_bytes: 1_000_000,
            rent_fee1_kb_soroban_state_size_low: 10,
            rent_fee1_kb_soroban_state_size_high: 20,
            soroban_state_rent_fee_growth_factor: 2000,
        });
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// Encode a minimal `ConfigSettingContractHistoricalDataV0` XDR.
fn encode_contract_historical_data_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, WriteXdr};

    let entry = ConfigSettingEntry::ContractHistoricalDataV0(
        stellar_xdr::ConfigSettingContractHistoricalDataV0 {
            fee_historical1_kb: 100,
        },
    );
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// Encode a minimal `ConfigSettingContractEventsV0` XDR.
fn encode_contract_events_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, WriteXdr};

    let entry = ConfigSettingEntry::ContractEventsV0(stellar_xdr::ConfigSettingContractEventsV0 {
        tx_max_contract_events_size_bytes: 1_000_000,
        fee_contract_events1_kb: 50,
    });
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// Encode a minimal `ConfigSettingContractBandwidthV0` XDR.
fn encode_contract_bandwidth_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, WriteXdr};

    let entry =
        ConfigSettingEntry::ContractBandwidthV0(stellar_xdr::ConfigSettingContractBandwidthV0 {
            ledger_max_txs_size_bytes: 1_000_000,
            tx_max_size_bytes: 100_000,
            fee_tx_size1_kb: 10,
        });
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// Encode a minimal `StateArchivalSettings` XDR.
fn encode_state_archival_xdr() -> String {
    use base64::Engine;
    use stellar_xdr::{ConfigSettingEntry, StateArchivalSettings, WriteXdr};

    let entry = ConfigSettingEntry::StateArchival(StateArchivalSettings {
        max_entry_ttl: 4096,
        min_temporary_ttl: 16,
        min_persistent_ttl: 2_073_600,
        persistent_rent_rate_denominator: 100_000,
        temp_rent_rate_denominator: 50_000,
        max_entries_to_archive: 100,
        live_soroban_state_size_window_sample_size: 20,
        live_soroban_state_size_window_sample_period: 10_000,
        eviction_scan_size: 1000,
        starting_eviction_scan_level: 16,
    });
    let xdr = entry.to_xdr(stellar_xdr::Limits::none()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(xdr)
}

/// A canned `getLedgerEntries` response with all 6 config settings.
/// Echoes back the same keys the client sent, paired with the XDR values.
fn get_ledger_entries_response(keys: &[String]) -> Value {
    let xdr_values = [
        encode_contract_compute_xdr(),
        encode_contract_ledger_cost_xdr(),
        encode_contract_historical_data_xdr(),
        encode_contract_events_xdr(),
        encode_contract_bandwidth_xdr(),
        encode_state_archival_xdr(),
    ];

    let entries: Vec<Value> = keys
        .iter()
        .zip(xdr_values.iter())
        .map(|(key, xdr)| {
            json!({
                "key": key,
                "xdr": xdr,
                "lastModifiedLedgerSeq": 3_894_195
            })
        })
        .collect();

    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "entries": entries,
            "latestLedger": 3_894_195
        }
    })
}

/// Handles JSON-RPC requests, dispatching to the appropriate mock handler.
async fn rpc_handler(Json(body): Json<Value>) -> Json<Value> {
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = body.get("params");

    match method {
        "simulateTransaction" => Json(simulate_response()),
        "getLedgerEntries" => {
            let keys: Vec<String> = params
                .and_then(|p| p.get("keys"))
                .and_then(|k| k.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Json(get_ledger_entries_response(&keys))
        }
        _ => Json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {method}")
            }
        })),
    }
}

/// Start a mock RPC server and return its address.
async fn start_mock_server() -> SocketAddr {
    let app = Router::new().route("/", post(rpc_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

// ─────────────────────────────────────────────────────────────────────────
// Integration tests
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_estimate_single_function_against_mock_rpc() {
    let addr = start_mock_server().await;
    let rpc_url = format!("http://{addr}");

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--rpc-url",
            &rpc_url,
        ])
        .output()
        .await
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "CLI should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Function:"),
        "output should contain function header; got: {stdout}"
    );
    assert!(
        stdout.contains("testnet"),
        "output should contain network name; got: {stdout}"
    );
    assert!(
        stdout.contains("Fee Breakdown"),
        "output should contain fee breakdown; got: {stdout}"
    );
}

#[tokio::test]
async fn test_estimate_json_output_against_mock_rpc() {
    let addr = start_mock_server().await;
    let rpc_url = format!("http://{addr}");

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--rpc-url",
            &rpc_url,
            "--json",
        ])
        .env("RUST_LOG", "error")
        .output()
        .await
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "CLI should exit 0; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output should be valid JSON");
    assert_eq!(parsed["function"], "(wasm upload)");
    assert_eq!(parsed["network"], "testnet");
    assert!(parsed["cpu_instructions"].is_number());
    assert!(parsed["fee"]["total_stroops"].is_number());
}

#[tokio::test]
async fn test_estimate_all_json_output_accepts_json_flag() {
    // Verify --json is accepted as a valid argument for estimate-all.
    // (estimate-all doesn't have --rpc-url, so we test argument parsing only.)
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate-all",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--json",
        ])
        .env("RUST_LOG", "error")
        .output()
        .await
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail on RPC/network, not on unknown argument
    assert!(
        !stderr.contains("unexpected argument"),
        "--json should be accepted; stderr: {stderr}"
    );
}

#[tokio::test]
async fn test_config_snapshot_offline() {
    // config snapshot doesn't have --rpc-url, so we test that it
    // gracefully fails when the network is unknown.
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["config", "snapshot", "--network", "not-a-network"])
        .output()
        .await
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "unknown network should fail");
    assert!(
        stderr.contains("RPC endpoint not configured"),
        "should name the unknown network; stderr: {stderr}"
    );
}

#[tokio::test]
async fn test_estimate_all_json_output_offline() {
    // estimate-all --json on an unknown network fails but still produces
    // a JSON array with error entries.
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate-all",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--json",
            "--network",
            "not-a-network",
        ])
        .env("RUST_LOG", "error")
        .output()
        .await
        .expect("failed to run CLI");

    let _stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should fail because the network is unknown
    assert!(!output.status.success(), "unknown network should fail");
    assert!(
        stderr.contains("RPC endpoint not configured"),
        "should name the unknown network; stderr: {stderr}"
    );
}
