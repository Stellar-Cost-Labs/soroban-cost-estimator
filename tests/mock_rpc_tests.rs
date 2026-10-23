mod common;

use std::fs;
use std::path::Path;

use serde_json::json;

use soroban_cost_estimator::error::AppError;
use soroban_cost_estimator::rpc::client::{HealthResponse, RpcClient};
use soroban_cost_estimator::rpc::config::{GetLatestLedgerResponse, GetLedgerEntriesResponse};
use soroban_cost_estimator::rpc::simulate::{
    SimulateTransactionResponse, parse_resource_fee, parse_transaction_data_resource_fee,
    parse_transaction_data_resources,
};

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
async fn mock_rpc_fixture_serves_get_latest_ledger() {
    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(
        &server,
        "getLatestLedger",
        json!({
            "id": "3a1c",
            "protocolVersion": 23,
            "sequence": 1234
        }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str());
    let ledger: serde_json::Value = client
        .call("getLatestLedger", json!({}))
        .await
        .expect("latest ledger should succeed against the mock server");

    assert_eq!(ledger["sequence"], 1234);
    assert_eq!(ledger["protocolVersion"], 23);
}

#[tokio::test]
async fn mock_rpc_fixture_surfaces_transient_http_errors() {
    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_http_error(&server, "simulateTransaction", 503).await;

    // Retries are disabled so the assertion does not wait out the backoff.
    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        0,
        false,
    );
    let err = client
        .call::<serde_json::Value>("simulateTransaction", json!({ "transaction": "bad" }))
        .await
        .expect_err("a transient gateway status must surface as an error");

    assert!(
        matches!(err, AppError::HttpStatus { status: 503, .. }),
        "expected HttpStatus(503), got: {err:?}"
    );
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

// ─────────────────────────────────────────────────────────────────────────────
// Protocol Fixture Compatibility Tests
// ─────────────────────────────────────────────────────────────────────────────

fn read_fixture(filename: &str) -> String {
    let path = Path::new("tests/fixtures/rpc").join(filename);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture at {}: {e}", path.display()))
}

/// Compatibility case: Modern Soroban RPC (Protocol 21+) `simulateTransaction` response.
///
/// Features:
/// - Numeric `latestLedger`
/// - Plain decimal string `minResourceFee`
/// - Authoritative resources (CPU instructions, footprint sizes, read/write bytes)
///   and resource fee encoded in `transactionData` XDR
/// - Legacy `cost` object absent
#[tokio::test]
async fn test_fixture_simulate_transaction_current_compatibility() {
    let raw_json = read_fixture("simulate_transaction_current.json");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&raw_json).expect("valid fixture JSON");

    // 1. Direct typed deserialization
    let resp: SimulateTransactionResponse =
        serde_json::from_str(&raw_json).expect("must deserialize into SimulateTransactionResponse");

    assert_eq!(resp.latest_ledger, Some(3_894_195));
    assert_eq!(resp.ledger_sequence(), 3_894_195);
    assert_eq!(resp.min_resource_fee.as_deref(), Some("15427"));
    assert!(
        resp.cost.is_none(),
        "modern response does not include legacy cost object"
    );
    assert!(resp.error.is_none());

    // 2. Resource fee extraction (both decimal string and XDR)
    let fee_from_string = parse_resource_fee(&resp.min_resource_fee)
        .expect("parse resource fee")
        .expect("fee present");
    assert_eq!(fee_from_string, 15_427);

    let fee_from_xdr = parse_transaction_data_resource_fee(&resp.transaction_data)
        .expect("parse transaction data fee")
        .expect("fee present in XDR");
    assert_eq!(fee_from_xdr, 15_427);

    // 3. Simulation resource metrics extraction from transactionData XDR
    let resources = parse_transaction_data_resources(&resp.transaction_data)
        .expect("parse resources")
        .expect("resources present in XDR");
    assert_eq!(resources.cpu_insns, 532_502);
    assert_eq!(resources.read_entries, 1);
    assert_eq!(resources.write_entries, 1);
    assert_eq!(resources.read_bytes, 0);
    assert_eq!(resources.write_bytes, 136);

    // 4. End-to-end via mock RPC server
    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&server, "simulateTransaction", fixture_value).await;
    let client = RpcClient::new(server.uri().as_str());

    let server_resp: SimulateTransactionResponse = client
        .call("simulateTransaction", json!({ "transaction": "test" }))
        .await
        .expect("call should succeed");
    assert_eq!(server_resp.ledger_sequence(), 3_894_195);
}

/// Compatibility case: Legacy Soroban RPC (Protocol 20) `simulateTransaction` response.
///
/// Features:
/// - String-formatted `latestLedger` (`"3894195"`)
/// - Base64-encoded XDR int64 `minResourceFee` (`"AAAAAAA81a8="`)
/// - Legacy `cost` object with string numbers (`"cpuInsns"`, `"memBytes"`)
/// - `transactionData` is null
#[tokio::test]
async fn test_fixture_simulate_transaction_legacy_compatibility() {
    let raw_json = read_fixture("simulate_transaction_legacy.json");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&raw_json).expect("valid fixture JSON");

    // 1. Direct typed deserialization
    let resp: SimulateTransactionResponse =
        serde_json::from_str(&raw_json).expect("must deserialize into SimulateTransactionResponse");

    // String latestLedger must be flexibly parsed
    assert_eq!(resp.latest_ledger, Some(3_894_195));
    assert_eq!(resp.ledger_sequence(), 3_894_195);

    // Legacy cost object with string numbers must be parsed
    let cost = resp.cost.expect("legacy cost object must be present");
    assert_eq!(cost.cpu_insns, 100_000);
    assert_eq!(cost.mem_bytes, 2_000);

    // Base64-encoded XDR int64 fee must be parsed
    let fee = parse_resource_fee(&resp.min_resource_fee)
        .expect("parse resource fee")
        .expect("fee present");
    assert_eq!(fee, 3_986_863);

    // 2. End-to-end via mock RPC server
    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&server, "simulateTransaction", fixture_value).await;
    let client = RpcClient::new(server.uri().as_str());

    let server_resp: SimulateTransactionResponse = client
        .call("simulateTransaction", json!({ "transaction": "test" }))
        .await
        .expect("call should succeed");
    assert_eq!(server_resp.ledger_sequence(), 3_894_195);
}

/// Compatibility case: Contract simulation execution error.
///
/// Features:
/// - `error` field carries failure string
/// - `latestLedger` is reported
#[test]
fn test_fixture_simulate_transaction_error_compatibility() {
    let raw_json = read_fixture("simulate_transaction_error.json");
    let resp: SimulateTransactionResponse =
        serde_json::from_str(&raw_json).expect("must deserialize error response");

    assert_eq!(
        resp.error.as_deref(),
        Some("HostError: Error(Contract, #1)")
    );
    assert_eq!(resp.ledger_sequence(), 3_894_195);
}

/// Compatibility case: Modern `getHealth` response with node metadata.
///
/// Features:
/// - `status`: "healthy"
/// - `networkPassphrase` and `coreVersion` present
/// - Numeric `latestLedger`
#[tokio::test]
async fn test_fixture_get_health_current_compatibility() {
    let raw_json = read_fixture("get_health_current.json");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&raw_json).expect("valid fixture JSON");

    let resp: HealthResponse =
        serde_json::from_str(&raw_json).expect("must deserialize into HealthResponse");
    assert_eq!(resp.status, "healthy");
    assert_eq!(
        resp.network_passphrase.as_deref(),
        Some("Test SDF Network ; September 2015")
    );
    assert_eq!(resp.core_version.as_deref(), Some("stellar-core 21.0.0"));
    assert_eq!(resp.latest_ledger, Some(123_456));

    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&server, "getHealth", fixture_value).await;
    let client = RpcClient::new(server.uri().as_str());

    let metadata = client.health_check().await.expect("health check must pass");
    assert_eq!(
        metadata.network_passphrase.as_deref(),
        Some("Test SDF Network ; September 2015")
    );
    assert_eq!(
        metadata.core_version.as_deref(),
        Some("stellar-core 21.0.0")
    );
}

/// Compatibility case: Legacy minimal `getHealth` response.
///
/// Features:
/// - `status`: "healthy"
/// - Node metadata omitted
/// - Numeric `latestLedger`
#[tokio::test]
async fn test_fixture_get_health_legacy_compatibility() {
    let raw_json = read_fixture("get_health_legacy.json");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&raw_json).expect("valid fixture JSON");

    let resp: HealthResponse =
        serde_json::from_str(&raw_json).expect("must deserialize into HealthResponse");
    assert_eq!(resp.status, "healthy");
    assert!(resp.network_passphrase.is_none());
    assert!(resp.core_version.is_none());
    assert_eq!(resp.latest_ledger, Some(42));

    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&server, "getHealth", fixture_value).await;
    let client = RpcClient::new(server.uri().as_str());

    let metadata = client.health_check().await.expect("health check must pass");
    assert!(metadata.network_passphrase.is_none());
    assert!(metadata.core_version.is_none());
}

/// Compatibility case: Unhealthy node response.
#[tokio::test]
async fn test_fixture_get_health_unhealthy_compatibility() {
    let raw_json = read_fixture("get_health_unhealthy.json");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&raw_json).expect("valid fixture JSON");

    let resp: HealthResponse =
        serde_json::from_str(&raw_json).expect("must deserialize into HealthResponse");
    assert_eq!(resp.status, "unhealthy");

    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&server, "getHealth", fixture_value).await;
    let client = RpcClient::new(server.uri().as_str());

    let err = client
        .health_check()
        .await
        .expect_err("unhealthy status must fail");
    assert!(err.to_string().contains("unhealthy"));
}

/// Compatibility case: `getLatestLedger` for modern and legacy protocol versions.
///
/// Tests both Protocol 22 (current) and Protocol 20 (legacy).
#[test]
fn test_fixture_get_latest_ledger_current_and_legacy_compatibility() {
    let current_raw = read_fixture("get_latest_ledger_current.json");
    let current: GetLatestLedgerResponse =
        serde_json::from_str(&current_raw).expect("deserialize current latest ledger");
    assert_eq!(current.protocol_version, 22);
    assert_eq!(current.sequence, 2_539_605);

    let legacy_raw = read_fixture("get_latest_ledger_legacy.json");
    let legacy: GetLatestLedgerResponse =
        serde_json::from_str(&legacy_raw).expect("deserialize legacy latest ledger");
    assert_eq!(legacy.protocol_version, 20);
    assert_eq!(legacy.sequence, 1_000_000);
}

/// Compatibility case: `getLedgerEntries` for modern and legacy responses.
///
/// Verifies:
/// - Modern response: numeric latestLedger, entry with liveUntilLedgerSeq
/// - Legacy response: string latestLedger ("1000005"), entry without liveUntilLedgerSeq
#[tokio::test]
async fn test_fixture_get_ledger_entries_current_and_legacy_compatibility() {
    // 1. Current shape
    let current_raw = read_fixture("get_ledger_entries_current.json");
    let current: GetLedgerEntriesResponse =
        serde_json::from_str(&current_raw).expect("deserialize current ledger entries");
    assert_eq!(current.latest_ledger, Some(2_539_605));
    assert_eq!(current.entries.len(), 1);
    assert_eq!(current.entries[0].last_modified_ledger_seq, Some(2_539_600));
    assert_eq!(current.entries[0].live_until_ledger_seq, Some(2_600_000));

    // 2. Legacy shape (string latestLedger, no liveUntilLedgerSeq)
    let legacy_raw = read_fixture("get_ledger_entries_legacy.json");
    let legacy: GetLedgerEntriesResponse =
        serde_json::from_str(&legacy_raw).expect("deserialize legacy ledger entries");
    assert_eq!(legacy.latest_ledger, Some(1_000_005));
    assert_eq!(legacy.entries.len(), 1);
    assert_eq!(legacy.entries[0].last_modified_ledger_seq, Some(1_000_000));
    assert_eq!(legacy.entries[0].live_until_ledger_seq, None);

    // 3. Via mock server
    let server = common::start_mock_rpc_server().await;
    let legacy_val: serde_json::Value = serde_json::from_str(&legacy_raw).expect("valid json");
    common::stub_jsonrpc_success(&server, "getLedgerEntries", legacy_val).await;
    let client = RpcClient::new(server.uri().as_str());

    let server_resp: GetLedgerEntriesResponse = client
        .call("getLedgerEntries", json!({ "keys": ["AAAAFA=="] }))
        .await
        .expect("call should succeed");
    assert_eq!(server_resp.latest_ledger, Some(1_000_005));
}
