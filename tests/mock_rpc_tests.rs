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

#[tokio::test]
async fn mock_rpc_retries_valid_delta_seconds_retry_after() {
    let server = common::start_mock_rpc_server().await;

    // First request gets 429 with delta-seconds "0" (immediate retry)
    common::stub_rate_limit(&server, "getHealth", Some("0"), 1).await;
    // Second request succeeds
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy"
        }),
    )
    .await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        2,
        false,
    );

    let health: serde_json::Value = client.call("getHealth", json!({})).await.expect(
        "rate-limited request with valid delta-seconds Retry-After should succeed on retry",
    );
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_retries_valid_http_date_retry_after() {
    let server = common::start_mock_rpc_server().await;

    // HTTP-date 1 second in the future
    let future_time = std::time::SystemTime::now() + std::time::Duration::from_secs(1);
    let date_str = httpdate::fmt_http_date(future_time);

    common::stub_rate_limit(&server, "getHealth", Some(&date_str), 1).await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy"
        }),
    )
    .await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        2,
        false,
    );

    let health: serde_json::Value = client
        .call("getHealth", json!({}))
        .await
        .expect("rate-limited request with valid HTTP-date Retry-After should succeed on retry");
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_retries_past_http_date_with_zero_delay() {
    let server = common::start_mock_rpc_server().await;

    // HTTP-date in the past
    let past_date = "Sun, 06 Nov 1994 08:49:37 GMT";

    common::stub_rate_limit(&server, "getHealth", Some(past_date), 1).await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy"
        }),
    )
    .await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        2,
        false,
    );

    let health: serde_json::Value = client
        .call("getHealth", json!({}))
        .await
        .expect("rate-limited request with past HTTP-date should retry immediately and succeed");
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_handles_malformed_retry_after_falling_back_to_backoff() {
    let server = common::start_mock_rpc_server().await;

    common::stub_rate_limit(&server, "getHealth", Some("malformed-not-a-date"), 1).await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy"
        }),
    )
    .await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        2,
        false,
    );

    let health: serde_json::Value = client.call("getHealth", json!({})).await.expect(
        "rate-limited request with malformed Retry-After should fall back to backoff and succeed",
    );
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_handles_absent_retry_after_falling_back_to_backoff() {
    let server = common::start_mock_rpc_server().await;

    // No Retry-After header
    common::stub_rate_limit(&server, "getHealth", None, 1).await;
    common::stub_jsonrpc_success(
        &server,
        "getHealth",
        json!({
            "status": "healthy"
        }),
    )
    .await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        2,
        false,
    );

    let health: serde_json::Value = client.call("getHealth", json!({})).await.expect(
        "rate-limited request without Retry-After header should fall back to backoff and succeed",
    );
    assert_eq!(health["status"], "healthy");
}

#[tokio::test]
async fn mock_rpc_excessive_retry_after_honors_zero_max_retries() {
    let server = common::start_mock_rpc_server().await;

    // Server returns excessive 429 delay: 86400s (1 day)
    common::stub_rate_limit(&server, "getHealth", Some("86400"), 1).await;

    // max_retries = 0: must not retry
    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        0,
        false,
    );

    let err = client
        .call::<serde_json::Value>("getHealth", json!({}))
        .await
        .expect_err("rate-limited request with max_retries=0 must surface error without retrying");

    match err {
        AppError::HttpStatus {
            status,
            retry_after,
            ..
        } => {
            assert_eq!(status, 429);
            // Must be capped by MAX_RETRY_DELAY (60s)
            assert_eq!(
                retry_after,
                Some(soroban_cost_estimator::rpc::retry::MAX_RETRY_DELAY)
            );
        }
        other => panic!("expected AppError::HttpStatus, got: {other:?}"),
    }
}

#[tokio::test]
async fn mock_rpc_excessive_http_date_is_capped_at_max_delay() {
    let server = common::start_mock_rpc_server().await;

    // Excessive HTTP-date far in the future
    let far_future = "Thu, 31 Dec 2099 23:59:59 GMT";
    common::stub_rate_limit(&server, "getHealth", Some(far_future), 1).await;

    let client = RpcClient::with_options(
        server.uri().as_str(),
        None,
        std::time::Duration::from_secs(5),
        0,
        false,
    );

    let err = client
        .call::<serde_json::Value>("getHealth", json!({}))
        .await
        .expect_err("excessive HTTP-date must fail fast when max_retries=0");

    match err {
        AppError::HttpStatus {
            status,
            retry_after,
            ..
        } => {
            assert_eq!(status, 429);
            assert_eq!(
                retry_after,
                Some(soroban_cost_estimator::rpc::retry::MAX_RETRY_DELAY)
            );
        }
        other => panic!("expected AppError::HttpStatus, got: {other:?}"),
    }
}
