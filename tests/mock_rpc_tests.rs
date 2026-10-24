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

#[test]
fn protocol_fixture_simulate_transaction_modern_p22_parses_typed_response() {
    use soroban_cost_estimator::rpc::simulate::{
        SimulateTransactionResponse, parse_resource_fee, parse_transaction_data_resource_fee,
        parse_transaction_data_resources,
    };

    let raw = include_str!("fixtures/rpc/simulate_transaction_modern_p22.json");
    let resp: SimulateTransactionResponse =
        serde_json::from_str(raw).expect("modern p22 fixture should deserialize");

    assert_eq!(resp.ledger_sequence(), 3_894_195);
    assert!(
        resp.cost.is_none(),
        "modern p22 responses omit top-level cost"
    );
    assert_eq!(
        parse_resource_fee(&resp.min_resource_fee).expect("decimal fee should parse"),
        Some(15_427)
    );
    assert_eq!(
        parse_transaction_data_resource_fee(&resp.transaction_data)
            .expect("transactionData resource_fee should parse"),
        Some(15_427)
    );

    let resources = parse_transaction_data_resources(&resp.transaction_data)
        .expect("transactionData resources should parse")
        .expect("resources should be present");
    assert_eq!(resources.cpu_insns, 532_502);
    assert_eq!(resources.read_entries, 1);
    assert_eq!(resources.write_entries, 1);
    assert_eq!(resources.read_bytes, 0);
    assert_eq!(resources.write_bytes, 136);
}

#[test]
fn protocol_fixture_simulate_transaction_legacy_p20_parses_strings_and_xdr_fee() {
    use soroban_cost_estimator::rpc::simulate::{SimulateTransactionResponse, parse_resource_fee};

    let raw = include_str!("fixtures/rpc/simulate_transaction_legacy_p20.json");
    let resp: SimulateTransactionResponse =
        serde_json::from_str(raw).expect("legacy p20 fixture should deserialize");

    assert_eq!(resp.ledger_sequence(), 1_842_910);
    let cost = resp.cost.expect("legacy p20 fixture carries cost object");
    assert_eq!(cost.cpu_insns, 100_000);
    assert_eq!(cost.mem_bytes, 2_000);
    assert_eq!(
        parse_resource_fee(&resp.min_resource_fee).expect("base64 XDR int64 fee should parse"),
        Some(42_000)
    );
}

#[test]
fn protocol_fixture_simulate_transaction_numeric_cost_parses_typed_response() {
    use soroban_cost_estimator::rpc::simulate::{SimulateTransactionResponse, parse_resource_fee};

    let raw = include_str!("fixtures/rpc/simulate_transaction_numeric_cost.json");
    let resp: SimulateTransactionResponse =
        serde_json::from_str(raw).expect("numeric cost fixture should deserialize");

    assert_eq!(resp.ledger_sequence(), 2_539_605);
    let cost = resp.cost.expect("numeric cost object should be present");
    assert_eq!(cost.cpu_insns, 250_000);
    assert_eq!(cost.mem_bytes, 4_096);
    assert_eq!(
        parse_resource_fee(&resp.min_resource_fee).expect("decimal fee should parse"),
        Some(28_500)
    );
}

#[test]
fn protocol_fixtures_health_and_ledger_parse_across_versions() {
    use soroban_cost_estimator::rpc::client::HealthResponse;
    use soroban_cost_estimator::rpc::config::{GetLatestLedgerResponse, GetLedgerEntriesResponse};

    let health_modern: HealthResponse =
        serde_json::from_str(include_str!("fixtures/rpc/get_health_healthy.json"))
            .expect("modern getHealth fixture should deserialize");
    assert_eq!(health_modern.status, "healthy");
    assert_eq!(health_modern.latest_ledger, Some(3_894_195));
    assert_eq!(health_modern.oldest_ledger, Some(3_876_915));
    assert_eq!(health_modern.ledger_retention_window, Some(17_280));

    let health_legacy: HealthResponse =
        serde_json::from_str(include_str!("fixtures/rpc/get_health_legacy_strings.json"))
            .expect("legacy string getHealth fixture should deserialize");
    assert_eq!(health_legacy.status, "healthy");
    assert_eq!(health_legacy.latest_ledger, Some(1_842_910));
    assert_eq!(health_legacy.oldest_ledger, Some(1_825_630));
    assert_eq!(health_legacy.ledger_retention_window, Some(17_280));

    let latest_ledger: GetLatestLedgerResponse =
        serde_json::from_str(include_str!("fixtures/rpc/get_latest_ledger_p22.json"))
            .expect("getLatestLedger fixture should deserialize");
    assert_eq!(latest_ledger.protocol_version, 22);
    assert_eq!(latest_ledger.sequence, 2_539_605);

    let entries_modern: GetLedgerEntriesResponse =
        serde_json::from_str(include_str!("fixtures/rpc/get_ledger_entries_p22.json"))
            .expect("modern getLedgerEntries fixture should deserialize");
    assert_eq!(entries_modern.latest_ledger, Some(2_539_605));
    assert_eq!(entries_modern.entries.len(), 1);
    assert_eq!(
        entries_modern.entries[0].last_modified_ledger_seq,
        Some(2_539_000)
    );
    assert_eq!(
        entries_modern.entries[0].live_until_ledger_seq,
        Some(2_600_000)
    );

    let entries_legacy: GetLedgerEntriesResponse = serde_json::from_str(include_str!(
        "fixtures/rpc/get_ledger_entries_legacy_p20.json"
    ))
    .expect("legacy getLedgerEntries fixture should deserialize");
    assert_eq!(entries_legacy.latest_ledger, Some(1_842_910));
    assert_eq!(entries_legacy.entries.len(), 1);
    assert_eq!(entries_legacy.entries[0].live_until_ledger_seq, None);
}

#[tokio::test]
async fn mock_rpc_serves_checked_in_protocol_fixtures_end_to_end() {
    use soroban_cost_estimator::rpc::config::{
        ConfigSettingId, fetch_config_setting, fetch_protocol_version,
    };
    use soroban_cost_estimator::rpc::simulate::simulate_transaction;

    let server = common::start_mock_rpc_server().await;

    let sim_fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/rpc/simulate_transaction_modern_p22.json"
    ))
    .expect("valid JSON fixture");
    let health_fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/rpc/get_health_legacy_strings.json"))
            .expect("valid JSON fixture");
    let ledger_fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/rpc/get_latest_ledger_p22.json"))
            .expect("valid JSON fixture");
    let entries_fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/rpc/get_ledger_entries_legacy_p20.json"
    ))
    .expect("valid JSON fixture");

    common::stub_jsonrpc_success(&server, "simulateTransaction", sim_fixture).await;
    common::stub_jsonrpc_success(&server, "getHealth", health_fixture).await;
    common::stub_jsonrpc_success(&server, "getLatestLedger", ledger_fixture).await;
    common::stub_jsonrpc_success(&server, "getLedgerEntries", entries_fixture).await;

    let client = RpcClient::new(server.uri().as_str());

    client
        .health_check()
        .await
        .expect("health_check should accept legacy string ledger fields");

    let protocol = fetch_protocol_version(&client)
        .await
        .expect("fetch_protocol_version should parse fixture");
    assert_eq!(protocol, 22);

    let setting = fetch_config_setting(&client, ConfigSettingId::ContractComputeV0)
        .await
        .expect("fetch_config_setting should parse legacy getLedgerEntries fixture");
    assert_eq!(setting.last_modified_ledger, 1_842_000);

    let sim = simulate_transaction(&client, "AAAA")
        .await
        .expect("simulate_transaction should parse modern p22 fixture");
    assert_eq!(sim.ledger_sequence(), 3_894_195);
}
