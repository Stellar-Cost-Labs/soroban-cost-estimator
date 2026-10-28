mod common;

use serde_json::json;

use soroban_cost_estimator::error::AppError;
use soroban_cost_estimator::rpc::client::RpcClient;

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

#[tokio::test]
async fn test_cache_warm_end_to_end_with_mock_rpc() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy",
            "latestLedger": 100
        }),
    )
    .await;

    let live_tx_data = "AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAEmU1Fc+h02S4iEBnpjdCESXpKHG/bOUxC3DeRWUy9+mQAAABQAAAABAAggFgAAAAAAAACIAAAAAAAAPEM=";
    common::stub_jsonrpc_success(
        &server,
        "simulateTransaction",
        json!({
            "latestLedger": 100,
            "minResourceFee": "15427",
            "transactionData": live_tx_data
        }),
    )
    .await;

    let temp_home = std::env::temp_dir().join(format!("sce-warm-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_home);
    let _ = std::fs::create_dir_all(&temp_home);

    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd.env("HOME", &temp_home);
    cmd.env("USERPROFILE", &temp_home);
    cmd.args([
        "config",
        "cache",
        "warm",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--rpc-url",
        &server.uri(),
    ]);

    let output = cmd.output().expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().unwrap_or(-1);

    assert_eq!(code, 0, "cache warm should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Warmed 1 functions into cache for"),
        "summary format: {stdout}"
    );
    assert!(
        stderr.contains("Warming cache"),
        "stderr should display progress: {stderr}"
    );

    // JSON mode test (use --quiet so tracing log is suppressed from stdout)
    let mut cmd_json = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd_json.env("HOME", &temp_home);
    cmd_json.env("USERPROFILE", &temp_home);
    cmd_json.args([
        "config",
        "cache",
        "warm",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--rpc-url",
        &server.uri(),
        "--json",
        "--quiet",
    ]);

    let output_json = cmd_json.output().expect("failed to run CLI");
    let stdout_json = String::from_utf8_lossy(&output_json.stdout);
    assert_eq!(output_json.status.code().unwrap_or(-1), 0);
    let v: serde_json::Value =
        serde_json::from_str(stdout_json.trim()).expect("valid JSON summary");
    assert_eq!(v["warmed_functions"], 1);
    assert!(v.get("wasm_hash").is_some(), "wasm_hash in JSON: {v}");

    // Verify cache entry is present and queryable
    let mut cmd_query = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd_query.env("HOME", &temp_home);
    cmd_query.env("USERPROFILE", &temp_home);
    cmd_query.args([
        "config",
        "cache",
        "query",
        "--fn",
        "increment",
        "--json",
        "--quiet",
    ]);
    let output_query = cmd_query.output().expect("failed to run CLI query");
    let stdout_query = String::from_utf8_lossy(&output_query.stdout);
    assert_eq!(output_query.status.code().unwrap_or(-1), 0);
    let entries: serde_json::Value =
        serde_json::from_str(stdout_query.trim()).expect("valid JSON query");
    assert_eq!(entries.as_array().map(Vec::len), Some(1));
    assert_eq!(entries[0]["function"], "increment");
}
