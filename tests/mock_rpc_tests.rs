mod common;

use serde_json::json;

use soroban_cost_estimator::error::AppError;
use soroban_cost_estimator::rpc::client::RpcClient;
use soroban_cost_estimator::rpc::config::{self, ConfigSettingId};

/// Base64 `LedgerEntryData::ConfigSetting` XDR for a compute-settings entry
/// with a known CPU rate — the payload a real `getLedgerEntries` response
/// carries for `ConfigSettingContractComputeV0`.
fn compute_config_entry_xdr() -> String {
    use stellar_xdr::{
        ConfigSettingContractComputeV0, ConfigSettingEntry, LedgerEntryData, Limits, WriteXdr,
    };

    let entry = ConfigSettingEntry::ContractComputeV0(ConfigSettingContractComputeV0 {
        ledger_max_instructions: 580_000_000,
        tx_max_instructions: 400_000_000,
        fee_rate_per_instructions_increment: 7,
        tx_memory_limit: 41_943_040,
    });
    let bytes = LedgerEntryData::ConfigSetting(entry)
        .to_xdr(Limits::none())
        .expect("encode config entry");
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

/// Issue #527, normal case: a served config entry carries the ledger the RPC
/// answered at (`latestLedger`) and the entry's last-modified ledger — the
/// values the fee-rate provenance is built from.
#[tokio::test]
async fn fetch_config_setting_records_serving_ledger_and_last_modified() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "getLedgerEntries",
        json!({
            "latestLedger": 12_345,
            "entries": [
                {
                    "key": "ignored-by-single-fetch",
                    "xdr": compute_config_entry_xdr(),
                    "lastModifiedLedgerSeq": 12_000
                }
            ]
        }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str());
    let raw = config::fetch_config_setting(&client, ConfigSettingId::ContractComputeV0)
        .await
        .expect("config setting should be fetched");

    assert_eq!(raw.latest_ledger, Some(12_345));
    assert_eq!(raw.last_modified_ledger, 12_000);
    assert_eq!(raw.id, ConfigSettingId::ContractComputeV0);
}

/// Issue #527, partial-fetch building block: when the RPC answers but serves
/// no entry, the fetch fails with "not found" — that is how one missing
/// setting becomes `available: false` in the fee-rate provenance.
#[tokio::test]
async fn fetch_config_setting_missing_entry_is_not_found() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "getLedgerEntries",
        json!({ "latestLedger": 7, "entries": [] }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str());
    let err = config::fetch_config_setting(&client, ConfigSettingId::ContractBandwidthV0)
        .await
        .expect_err("an empty entries list must surface as not-found");

    assert!(
        matches!(err, AppError::ConfigSettingNotFound(_)),
        "got: {err}"
    );
}

#[tokio::test]
async fn mock_rpc_fixture_handles_simulate_transaction_and_health() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "simulateTransaction",
        json!({
            "latestLedger": 42,
            "minResourceFee": "100",
            "transactionData": "abc123"
        }),
    )
    .await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy",
            "latestLedger": 42
        }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str());

    let tx: serde_json::Value = client
        .call("simulateTransaction", json!({ "transaction": "abc" }))
        .await
        .expect("simulateTransaction should succeed against the mock server");
    assert_eq!(tx["latestLedger"], 42);
    assert_eq!(tx["minResourceFee"], "100");

    let health: serde_json::Value = client
        .call("getHealth", json!({}))
        .await
        .expect("health check should succeed against the mock server");
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_fixture_handles_ledger_entries_and_network_payloads() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "getLedgerEntries",
        json!({
            "latestLedger": 7,
            "entries": [
                {
                    "key": "abc",
                    "xdr": "e30=",
                    "lastModifiedLedgerSeq": 7
                }
            ]
        }),
    )
    .await;
    common::stub_jsonrpc_success(
        &server,
        "getNetwork",
        json!({
            "name": "testnet",
            "passphrase": "Test SDF Network ; September 2015",
            "protocolVersion": 23
        }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str());

    let entries: serde_json::Value = client
        .call("getLedgerEntries", json!({ "keys": ["abc"] }))
        .await
        .expect("ledger entries should succeed against the mock server");
    assert_eq!(entries["latestLedger"], 7);
    assert_eq!(entries["entries"][0]["key"], "abc");

    let network: serde_json::Value = client
        .call("getNetwork", json!({}))
        .await
        .expect("network metadata should succeed against the mock server");
    assert_eq!(network["name"], "testnet");
}

#[tokio::test]
async fn mock_rpc_fixture_surfaces_rpc_errors_from_the_server() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_error(&server, "simulateTransaction", -32602, "invalid params").await;

    let client = RpcClient::new(server.uri().as_str());
    let err = client
        .call::<serde_json::Value>("simulateTransaction", json!({ "transaction": "bad" }))
        .await
        .expect_err("mocked JSON-RPC errors should propagate as AppError::Rpc");

    assert!(matches!(err, AppError::Rpc { status: -32602, .. }));
}
