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
async fn rejects_response_whose_content_length_exceeds_configured_cap() {
    let server = common::start_mock_rpc_server().await;
    let secret_marker = "SENSITIVE_BODY_CONTENT_SHOULD_NOT_LEAK";
    let large_payload = format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"blob":"{}","padding":"{}"}}}}"#,
        secret_marker,
        "x".repeat(512)
    );

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .set_body_string(large_payload.clone()),
        )
        .mount(&server)
        .await;

    let client = RpcClient::new(server.uri().as_str()).with_max_response_bytes(128);
    let err = client
        .call::<serde_json::Value>("getHealth", json!({}))
        .await
        .expect_err("oversized Content-Length response must be rejected");

    let rendered = err.to_string();
    assert!(
        !rendered.contains(secret_marker),
        "error message must never include response body contents: {rendered}"
    );
    match err {
        AppError::ResponseTooLarge {
            endpoint,
            max_bytes,
            observed_bytes,
        } => {
            assert_eq!(endpoint, server.uri());
            assert_eq!(max_bytes, 128);
            assert_eq!(observed_bytes, large_payload.len() as u64);
        }
        other => panic!("expected AppError::ResponseTooLarge, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_streamed_response_exceeding_cap_without_trusting_content_length_header() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback server");
    let addr = listener.local_addr().expect("local addr");
    let endpoint_url = format!("http://{addr}");

    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut req_buf = [0u8; 1024];
            let _ = socket.read(&mut req_buf).await;
            // Send a chunked response (no Content-Length) that streams more
            // bytes than the client's 100-byte cap.
            let chunk = format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"secret":"DO_NOT_LOG_STREAM","pad":"{}"}}}}"#,
                "y".repeat(200)
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                chunk.len(),
                chunk
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    let client = RpcClient::new(&endpoint_url).with_max_response_bytes(100);
    let err = client
        .call::<serde_json::Value>("getHealth", json!({}))
        .await
        .expect_err("oversized chunked stream must be rejected while reading");

    let rendered = err.to_string();
    assert!(
        !rendered.contains("DO_NOT_LOG_STREAM"),
        "error message must never include streamed body contents: {rendered}"
    );
    match err {
        AppError::ResponseTooLarge {
            endpoint,
            max_bytes,
            observed_bytes,
        } => {
            assert_eq!(endpoint, endpoint_url);
            assert_eq!(max_bytes, 100);
            assert!(
                observed_bytes > 100,
                "observed_bytes ({observed_bytes}) must exceed max_bytes (100)"
            );
        }
        other => panic!("expected AppError::ResponseTooLarge, got {other:?}"),
    }
}
