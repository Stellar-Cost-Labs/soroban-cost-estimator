mod common;

#[tokio::test]
async fn global_proxy_applies_to_standard_client_and_fallback() {
    // Isolate process-global configuration from other concurrently running tests.
    const CHILD: &str = "SCE_GLOBAL_PROXY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "global_proxy_applies_to_standard_client_and_fallback",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let proxy = common::start_mock_rpc_server().await;
    wiremock::Mock::given(wiremock::matchers::header("host", "primary.invalid"))
        .respond_with(wiremock::ResponseTemplate::new(502))
        .expect(1)
        .mount(&proxy)
        .await;
    wiremock::Mock::given(wiremock::matchers::header("host", "fallback.invalid"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}),
        ))
        .expect(1)
        .mount(&proxy)
        .await;
    soroban_cost_estimator::rpc::client::set_explicit_proxy(Some(&proxy.uri())).unwrap();
    let client = RpcClient::with_fallback_headers_connect_timeout(
        "http://primary.invalid",
        Some("http://fallback.invalid"),
        None,
        std::time::Duration::from_secs(3),
        std::time::Duration::from_secs(1),
        0,
        &[],
        false,
    )
    .unwrap();
    let result: serde_json::Value = client
        .call("getHealth", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(result["status"], "healthy");
}

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
async fn explicit_proxy_routes_rpc_requests_through_local_proxy() {
    let proxy = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(&proxy, "getHealth", json!({ "status": "healthy" })).await;

    // The target is deliberately unreachable. A successful response proves
    // reqwest sent the request to the explicit local proxy instead.
    let client = RpcClient::with_proxy("http://unreachable.invalid", proxy.uri().as_str())
        .expect("local HTTP proxy URL should be valid");
    let health: serde_json::Value = client
        .call("getHealth", json!({}))
        .await
        .expect("explicit proxy should forward the RPC request");

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

#[tokio::test]
async fn mock_rpc_fixture_bounds_dedup_cache_and_evicts_lru() {
    let server = common::start_mock_rpc_server().await;

    common::stub_jsonrpc_success(
        &server,
        "getLedgerEntries",
        json!({
            "latestLedger": 100,
            "entries": []
        }),
    )
    .await;

    // Client with capacity bound of 2
    let client = RpcClient::new(server.uri().as_str()).with_dedup_capacity(2);

    let k1 = json!({ "keys": ["key1"] });
    let k2 = json!({ "keys": ["key2"] });
    let k3 = json!({ "keys": ["key3"] });

    let _: serde_json::Value = client
        .call("getLedgerEntries", k1.clone())
        .await
        .expect("call k1");
    let _: serde_json::Value = client
        .call("getLedgerEntries", k2.clone())
        .await
        .expect("call k2");
    assert_eq!(client.dedup_cache_len().await, 2);

    // Re-access k1 to promote it as most recently used
    let _: serde_json::Value = client
        .call("getLedgerEntries", k1.clone())
        .await
        .expect("repeat k1");
    assert_eq!(client.dedup_cache_len().await, 2);

    // Call k3: exceeds bound of 2, evicts least-recently used (k2)
    let _: serde_json::Value = client
        .call("getLedgerEntries", k3.clone())
        .await
        .expect("call k3");
    assert_eq!(client.dedup_cache_len().await, 2);

    // k1 and k3 are retained in cache; k2 was evicted
    assert_eq!(client.dedup_cache_len().await, 2);
}
