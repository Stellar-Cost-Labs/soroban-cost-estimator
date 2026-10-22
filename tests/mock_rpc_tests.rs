mod common;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;

use soroban_cost_estimator::error::AppError;
use soroban_cost_estimator::rpc::client::{is_sensitive_header, redact_sensitive, RpcClient};

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
async fn rpc_rejects_oversized_response_with_content_length() {
    let server = common::start_mock_rpc_server().await;

    // Response exceeds the 10 MB limit
    common::stub_oversized_response_with_content_length(&server, "simulateTransaction").await;

    let client = RpcClient::new(server.uri().as_str());
    let err = client
        .call::<serde_json::Value>("simulateTransaction", json!({ "transaction": "abc" }))
        .await
        .expect_err("oversized response should be rejected");

    match err {
        AppError::ResponseTooLarge {
            endpoint,
            observed_size,
            max_size,
        } => {
            assert_eq!(endpoint, server.uri().as_str());
            assert!(observed_size > 10 * 1024 * 1024);
            assert_eq!(max_size, 10 * 1024 * 1024);
        }
        _ => panic!("expected ResponseTooLarge error, got: {:?}", err),
    }
}

#[tokio::test]
async fn rpc_rejects_oversized_response_without_content_length() {
    let server = common::start_mock_rpc_server().await;

    // Response exceeds the 10 MB limit, without Content-Length header
    common::stub_oversized_response_without_content_length(&server, "getLedgerEntries").await;

    let client = RpcClient::new(server.uri().as_str());
    let err = client
        .call::<serde_json::Value>("getLedgerEntries", json!({ "keys": ["abc"] }))
        .await
        .expect_err("oversized response without Content-Length should be rejected");

    match err {
        AppError::ResponseTooLarge {
            endpoint,
            observed_size,
            max_size,
        } => {
            assert_eq!(endpoint, server.uri().as_str());
            assert!(observed_size > 10 * 1024 * 1024);
            assert_eq!(max_size, 10 * 1024 * 1024);
        }
        _ => panic!("expected ResponseTooLarge error, got: {:?}", err),
    }
}

#[tokio::test]
async fn rpc_accepts_response_within_size_limit() {
    let server = common::start_mock_rpc_server().await;

    // 5 MB is within the 10 MB limit
    let large_but_valid_response = vec![b'x'; 5 * 1024 * 1024];
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

    let client = RpcClient::new(server.uri().as_str());
    let result: serde_json::Value = client
        .call("simulateTransaction", json!({ "transaction": "abc" }))
        .await
        .expect("normal-sized response should succeed");

    assert_eq!(result["latestLedger"], 42);
}

#[test]
fn redaction_identifies_sensitive_headers_case_insensitive() {
    let test_cases = vec![
        "authorization",
        "Authorization",
        "AUTHORIZATION",
        "x-api-key",
        "X-API-KEY",
        "X-Api-Key",
        "api-key",
        "API-KEY",
        "Api-Key",
        "proxy-authorization",
        "Proxy-Authorization",
        "cookie",
        "Cookie",
    ];

    for header_name in test_cases {
        let name = HeaderName::from_bytes(header_name.as_bytes()).unwrap();
        assert!(
            is_sensitive_header(&name),
            "header '{}' should be identified as sensitive",
            header_name
        );
    }
}

#[test]
fn redaction_identifies_non_sensitive_headers() {
    let test_cases = vec![
        "content-type",
        "accept",
        "user-agent",
        "x-custom-header",
        "x-request-id",
    ];

    for header_name in test_cases {
        let name = HeaderName::from_bytes(header_name.as_bytes()).unwrap();
        assert!(
            !is_sensitive_header(&name),
            "header '{}' should not be identified as sensitive",
            header_name
        );
    }
}

#[test]
fn redaction_replaces_sensitive_values_with_placeholder() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer secret-token"));
    headers.insert("x-api-key", HeaderValue::from_static("my-api-key"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));

    let redacted = redact_sensitive(&headers);

    assert_eq!(
        redacted.get("authorization").unwrap(),
        "<redacted>",
        "authorization header should be redacted"
    );
    assert_eq!(
        redacted.get("x-api-key").unwrap(),
        "<redacted>",
        "x-api-key header should be redacted"
    );
    assert_eq!(
        redacted.get("content-type").unwrap(),
        "application/json",
        "non-sensitive header should remain unchanged"
    );
}

#[test]
fn redaction_preserves_header_names() {
    let mut headers = HeaderMap::new();
    headers.insert("Authorization", HeaderValue::from_static("Bearer secret"));
    headers.insert("X-API-KEY", HeaderValue::from_static("key123"));

    let redacted = redact_sensitive(&headers);

    assert!(
        redacted.contains_key("Authorization"),
        "header name should be preserved"
    );
    assert!(
        redacted.contains_key("X-API-KEY"),
        "header name should be preserved"
    );
}
