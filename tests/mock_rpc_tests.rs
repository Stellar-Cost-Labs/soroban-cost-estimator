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
async fn completed_dedup_cache_evicts_least_recently_used_when_bound_exceeded() {
    let server = common::start_mock_rpc_server().await;
    common::stub_jsonrpc_success(
        &server,
        "simulateTransaction",
        json!({
            "latestLedger": 100,
            "minResourceFee": "100"
        }),
    )
    .await;

    let client = RpcClient::new(server.uri().as_str()).with_max_completed_dedup_entries(2);

    // Issue k1 and k2 (cache is now full: [k1, k2]).
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k1" }))
        .await
        .expect("k1 should succeed");
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k2" }))
        .await
        .expect("k2 should succeed");
    assert_eq!(client.completed_dedup_len().await, 2);
    assert_eq!(server.received_requests().await.expect("requests").len(), 2);

    // Touch k1 so order becomes [k2, k1] (k1 is most-recently-used, k2 is LRU).
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k1" }))
        .await
        .expect("k1 should hit dedup cache");
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        2,
        "cache hit on k1 must not send a network request"
    );

    // Insert k3 -> evicts k2 (LRU), retaining [k1, k3].
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k3" }))
        .await
        .expect("k3 should succeed");
    assert_eq!(client.completed_dedup_len().await, 2);
    assert_eq!(server.received_requests().await.expect("requests").len(), 3);

    // Identical recent calls (k1 and k3) still deduplicate without network traffic.
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k1" }))
        .await
        .expect("k1 still cached");
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k3" }))
        .await
        .expect("k3 still cached");
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        3,
        "recent entries k1 and k3 must remain deduplicated"
    );

    // Evicted entry k2 must perform a fresh network call.
    let _: serde_json::Value = client
        .call("simulateTransaction", json!({ "tx": "k2" }))
        .await
        .expect("evicted k2 re-fetches");
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        4,
        "evicted entry k2 must be re-requested over the network"
    );
    assert_eq!(client.completed_dedup_len().await, 2);
}

#[tokio::test]
async fn single_flight_in_progress_requests_still_coalesce_with_zero_completed_bound() {
    use std::time::Duration;
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{body_string_contains, method},
    };

    let server = common::start_mock_rpc_server().await;
    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"simulateTransaction\""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(120))
                .set_body_json(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "latestLedger": 77 }
                })),
        )
        .mount(&server)
        .await;

    let client = std::sync::Arc::new(
        RpcClient::new(server.uri().as_str()).with_max_completed_dedup_entries(0),
    );

    let c1 = std::sync::Arc::clone(&client);
    let c2 = std::sync::Arc::clone(&client);
    let (r1, r2) = tokio::join!(
        c1.call::<serde_json::Value>("simulateTransaction", json!({ "tx": "inflight" })),
        c2.call::<serde_json::Value>("simulateTransaction", json!({ "tx": "inflight" })),
    );

    assert_eq!(r1.expect("r1")["latestLedger"], 77);
    assert_eq!(r2.expect("r2")["latestLedger"], 77);
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        1,
        "concurrent identical requests must still share a single flight"
    );
    assert_eq!(client.completed_dedup_len().await, 0);
}
