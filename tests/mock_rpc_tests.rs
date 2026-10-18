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

#[test]
fn verbose_diagnostics_redact_auth_and_api_key_headers_across_cases() {
    let auth_secret = "Bearer tok-alpha-998877";
    let x_api_secret = "xkey-beta-665544";
    let api_secret = "apikey-gamma-332211";

    for (auth_name, x_api_name, api_name) in [
        ("Authorization", "x-api-key", "api-key"),
        ("authorization", "X-API-Key", "API-KEY"),
        ("AUTHORIZATION", "X-Api-Key", "Api-Key"),
    ] {
        let headers = vec![
            format!("{auth_name}={auth_secret}"),
            format!("{x_api_name}: {x_api_secret}"),
            format!("{api_name}={api_secret}"),
            "X-Trace-Id=trace-visible-777".to_string(),
        ];

        let client = RpcClient::with_headers("http://127.0.0.1:8080", &headers, true)
            .expect("headers should parse");
        let diagnostic = client.format_redacted_headers();

        assert!(
            !diagnostic.contains(auth_secret),
            "Authorization secret leaked in verbose diagnostic output"
        );
        assert!(
            !diagnostic.contains(x_api_secret),
            "x-api-key secret leaked in verbose diagnostic output"
        );
        assert!(
            !diagnostic.contains(api_secret),
            "api-key secret leaked in verbose diagnostic output"
        );
        assert!(
            diagnostic.contains("authorization: <redacted>"),
            "expected redacted authorization header in diagnostic output"
        );
        assert!(
            diagnostic.contains("x-api-key: <redacted>"),
            "expected redacted x-api-key header in diagnostic output"
        );
        assert!(
            diagnostic.contains("api-key: <redacted>"),
            "expected redacted api-key header in diagnostic output"
        );
        assert!(
            diagnostic.contains("x-trace-id: trace-visible-777"),
            "non-sensitive header must remain visible for debugging"
        );
    }
}

fn assert_stderr_redacts_secrets(
    stderr: &str,
    auth_secret: &str,
    x_api_secret: &str,
    api_secret: &str,
    expected_trace: &str,
) {
    assert!(
        !stderr.contains(auth_secret),
        "Authorization secret must not appear in stderr"
    );
    assert!(
        !stderr.contains(x_api_secret),
        "x-api-key secret must not appear in stderr"
    );
    assert!(
        !stderr.contains(api_secret),
        "api-key secret must not appear in stderr"
    );
    assert!(
        stderr.contains(expected_trace),
        "non-sensitive header must appear in verbose stderr"
    );
    assert!(
        stderr.contains("authorization: <redacted>")
            && stderr.contains("x-api-key: <redacted>")
            && stderr.contains("api-key: <redacted>"),
        "redacted placeholders must appear in verbose stderr"
    );
}

#[tokio::test]
async fn verbose_stderr_redacts_secrets_on_success_response() {
    let auth_secret = "Bearer secret-jwt-token-xyz987";
    let x_api_secret = "secret-x-api-key-val-654";
    let api_secret = "secret-api-key-val-321";

    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({ "status": "healthy", "latestLedger": 100 }),
    )
    .await;
    common::stub_jsonrpc_success(
        &server,
        "simulateTransaction",
        json!({
            "latestLedger": 100,
            "minResourceFee": "15427",
            "cost": { "cpuInsns": "10000", "memBytes": "2048" }
        }),
    )
    .await;

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "--verbose",
            "--max-retries",
            "0",
            "--header",
            &format!("Authorization: {auth_secret}"),
            "--header",
            &format!("X-Api-Key={x_api_secret}"),
            "--header",
            &format!("Api-Key: {api_secret}"),
            "--header",
            "X-Debug-Trace=trace-id-ok-101",
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--rpc-url",
            server.uri().as_str(),
        ])
        .output()
        .expect("CLI execution should succeed");

    assert!(out.status.success(), "expected CLI estimate to succeed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_redacts_secrets(
        &stderr,
        auth_secret,
        x_api_secret,
        api_secret,
        "x-debug-trace: trace-id-ok-101",
    );
}

#[tokio::test]
async fn verbose_stderr_redacts_secrets_on_failure_response() {
    let auth_secret = "Bearer secret-jwt-token-xyz987";
    let x_api_secret = "secret-x-api-key-val-654";
    let api_secret = "secret-api-key-val-321";

    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_error(&server, "getHealth", -32000, "unauthorized node").await;

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "--verbose",
            "--max-retries",
            "0",
            "--header",
            &format!("AUTHORIZATION={auth_secret}"),
            "--header",
            &format!("x-api-key: {x_api_secret}"),
            "--header",
            &format!("API-KEY={api_secret}"),
            "--header",
            "X-Debug-Trace=trace-id-fail-202",
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--rpc-url",
            server.uri().as_str(),
        ])
        .output()
        .expect("CLI execution should complete");

    assert!(!out.status.success(), "expected CLI estimate to fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_stderr_redacts_secrets(
        &stderr,
        auth_secret,
        x_api_secret,
        api_secret,
        "x-debug-trace: trace-id-fail-202",
    );
}
