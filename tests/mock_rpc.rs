//! A mock Soroban RPC server for deterministic, fully offline testing.
//!
//! `MockRpcServer` binds an ephemeral loopback port and answers JSON-RPC 2.0
//! POSTs exactly like a Soroban RPC node: `simulateTransaction` returns a
//! response whose `transactionData` XDR carries known resource usage, and
//! `getLedgerEntries` returns all six `ConfigSetting` ledger entries encoded
//! from known values.
//!
//! The XDR fixtures are built at runtime with `stellar_xdr` (the same types
//! the crate decodes), so the fixtures stay valid if the schema evolves — no
//! brittle base64 blobs checked into the tree. Because every value in a
//! fixture is distinct and nonzero, each test can assert exact round-trip
//! numbers end to end: HTTP request → JSON-RPC envelope → camelCase
//! deserialization → XDR decode → report field.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use serde_json::{Value, json};

use soroban_cost_estimator::error::AppError;
use soroban_cost_estimator::rpc::client::RpcClient;
use soroban_cost_estimator::rpc::config::{
    ConfigSettingId, fetch_all_config_settings, fetch_config_setting,
};
use soroban_cost_estimator::rpc::simulate::{
    parse_resource_fee, parse_transaction_data_resource_fee, parse_transaction_data_resources,
    simulate_transaction,
};
use stellar_xdr::WriteXdr;

// ─────────────────────────────────────────────────────────────────────────
// Fixture constants — every assertion in this file traces back to one of
// these values.
// ─────────────────────────────────────────────────────────────────────────

/// Ledger sequence reported by both RPC methods.
const FIXTURE_LATEST_LEDGER: u64 = 3_894_195;

/// `lastModifiedLedgerSeq` on every config entry returned by the mock.
const FIXTURE_ENTRY_LEDGER: u32 = 500_000;

/// CPU instructions inside the fixture `transactionData`.
const FIXTURE_CPU_INSNS: u32 = 100_000;

/// Disk read bytes inside the fixture `transactionData`.
const FIXTURE_DISK_READ_BYTES: u32 = 512;

/// Write bytes inside the fixture `transactionData`.
const FIXTURE_WRITE_BYTES: u32 = 2_048;

/// Total resource fee (`resourceFee` XDR + `minResourceFee` decimal string).
const FIXTURE_RESOURCE_FEE: i64 = 15_427;

/// `fee_rate_per_instructions_increment` in the fixture compute config.
const FIXTURE_FEE_RATE_PER_INSN: i64 = 7;

/// `fee_tx_size1_kb` in the fixture bandwidth config.
const FIXTURE_FEE_TX_SIZE_1KB: i64 = 52;

// ─────────────────────────────────────────────────────────────────────────
// Mock server
// ─────────────────────────────────────────────────────────────────────────

/// Responder signature: given the JSON-RPC method name and params, produce
/// the complete JSON-RPC response body (success or error).
type Responder = Arc<dyn Fn(&str, &Value) -> Value + Send + Sync>;

/// A tiny HTTP/1.1 JSON-RPC server bound to an ephemeral loopback port.
///
/// Connections are answered until the process exits; tests never need to
/// shut it down explicitly.
struct MockRpcServer {
    /// Base URL of the mock endpoint (e.g. `http://127.0.0.1:41234`).
    url: String,

    // Keeping the listener alive keeps the accept loop's source valid for as
    // long as the test holds the handle.
    _listener: TcpListener,
}

impl MockRpcServer {
    /// Starts a mock server that delegates every request to `responder`.
    fn start(responder: impl Fn(&str, &Value) -> Value + Send + Sync + 'static) -> Self {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("mock RPC server should bind a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("local addr"));

        let responder: Responder = Arc::new(responder);
        // The accept loop owns one handle; the struct keeps a clone so the
        // socket stays bound for as long as the test holds this value.
        let keepalive = listener.try_clone().expect("listener should be clonable");
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let responder = Arc::clone(&responder);
                std::thread::spawn(move || {
                    let _ = serve_one(&stream, &responder);
                });
            }
        });

        Self {
            url,
            _listener: keepalive,
        }
    }

    /// Starts a mock server with the standard Soroban method dispatch
    /// backed by the fixtures in this file.
    fn with_fixtures() -> Self {
        Self::start(respond_with_fixtures)
    }
}

/// Reads one HTTP request, dispatches to `responder`, writes the response.
fn serve_one(stream: &TcpStream, responder: &Responder) -> std::io::Result<()> {
    let mut stream = stream.try_clone().expect("TCP stream should be clonable");
    let body = read_request_body(&mut stream)?;
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    write_response(&mut stream, &responder(method, &params))
}

/// Drains one HTTP request head and returns its body bytes.
fn read_request_body(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);

    // Request line ("POST / HTTP/1.1") — irrelevant, but must be consumed.
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let mut content_length = 0usize;
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break; // blank line ends the header block
        }
        // HTTP header names are case-insensitive; reqwest emits them
        // lowercase, so match without regard to case.
        let lower = trimmed.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    Ok(body)
}

/// Writes a `200 OK` JSON response and closes the connection.
fn write_response(stream: &mut TcpStream, body: &Value) -> std::io::Result<()> {
    let payload = serde_json::to_vec(body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

/// Wraps `result` in a successful JSON-RPC 2.0 envelope.
fn rpc_ok(result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "result": result})
}

/// Builds a JSON-RPC error envelope.
fn rpc_error(code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "error": {"code": code, "message": message},
    })
}

/// Standard dispatch used by [`MockRpcServer::with_fixtures`].
fn respond_with_fixtures(method: &str, params: &Value) -> Value {
    match method {
        "simulateTransaction" => rpc_ok(&simulate_result()),
        "getLedgerEntries" => rpc_ok(&get_ledger_entries_result(params)),
        other => rpc_error(-32601, &format!("method not found: {other}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Fixtures: simulateTransaction
// ─────────────────────────────────────────────────────────────────────────

/// Encodes bytes as standard base64, matching how Soroban RPC ships XDR.
fn b64(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

/// Builds a base64 `SorobanTransactionData` carrying the fixture resource
/// values: 100k CPU insns, 512 disk-read bytes, 2048 write bytes, a
/// footprint of 1 read-only + 2 read-write entries, and fee 15_427 stroops.
fn fixture_transaction_data_b64() -> String {
    use stellar_xdr::ConfigSettingId as XdrConfigSettingId;

    let config_key = |id: XdrConfigSettingId| {
        stellar_xdr::LedgerKey::ConfigSetting(stellar_xdr::LedgerKeyConfigSetting {
            config_setting_id: id,
        })
    };

    let read_only: Vec<stellar_xdr::LedgerKey> =
        vec![config_key(XdrConfigSettingId::ContractComputeV0)];
    let read_write: Vec<stellar_xdr::LedgerKey> = vec![
        config_key(XdrConfigSettingId::ContractLedgerCostV0),
        config_key(XdrConfigSettingId::StateArchival),
    ];

    let data = stellar_xdr::SorobanTransactionData {
        ext: stellar_xdr::SorobanTransactionDataExt::V0,
        resources: stellar_xdr::SorobanResources {
            footprint: stellar_xdr::LedgerFootprint {
                read_only: read_only.try_into().expect("read_only footprint fits"),
                read_write: read_write.try_into().expect("read_write footprint fits"),
            },
            instructions: FIXTURE_CPU_INSNS,
            disk_read_bytes: FIXTURE_DISK_READ_BYTES,
            write_bytes: FIXTURE_WRITE_BYTES,
        },
        resource_fee: FIXTURE_RESOURCE_FEE,
    };

    b64(&data
        .to_xdr(stellar_xdr::Limits::none())
        .expect("fixture transaction data should encode"))
}

/// The `simulateTransaction` result body served by the mock: modern shape
/// (no legacy `cost` object), string-encoded numbers as the live testnet RPC
/// returns them.
fn simulate_result() -> Value {
    json!({
        "transactionData": fixture_transaction_data_b64(),
        "minResourceFee": FIXTURE_RESOURCE_FEE.to_string(),
        "latestLedger": FIXTURE_LATEST_LEDGER.to_string(),
        "cost": null,
        "events": [],
        "restoreFee": null,
        "stateChanges": [],
    })
}

/// A `simulateTransaction` result that reports a host-side simulation error.
fn simulate_error_result(message: &str) -> Value {
    json!({
        "error": message,
        "latestLedger": FIXTURE_LATEST_LEDGER.to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Fixtures: getLedgerEntries
// ─────────────────────────────────────────────────────────────────────────

/// One mocked ledger entry: the base64 `LedgerKey` and the base64
/// `LedgerEntryData` the RPC would return for it.
struct FixtureEntry {
    key_b64: String,
    xdr_b64: String,
}

/// Builds all six `ConfigSetting` ledger entries with known, distinct values.
fn fixture_config_entries() -> Vec<FixtureEntry> {
    use stellar_xdr::{
        ConfigSettingContractBandwidthV0, ConfigSettingContractComputeV0,
        ConfigSettingContractEventsV0, ConfigSettingContractHistoricalDataV0,
        ConfigSettingContractLedgerCostV0, StateArchivalSettings,
    };

    let key = |id: stellar_xdr::ConfigSettingId| {
        let ledger_key =
            stellar_xdr::LedgerKey::ConfigSetting(stellar_xdr::LedgerKeyConfigSetting {
                config_setting_id: id,
            });
        b64(&ledger_key
            .to_xdr(stellar_xdr::Limits::none())
            .expect("fixture ledger key should encode"))
    };
    let value = |entry: stellar_xdr::ConfigSettingEntry| {
        let data = stellar_xdr::LedgerEntryData::ConfigSetting(entry);
        b64(&data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("fixture ledger entry should encode"))
    };

    vec![
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::ContractComputeV0),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::ContractComputeV0(
                ConfigSettingContractComputeV0 {
                    ledger_max_instructions: 580_000_000,
                    tx_max_instructions: 400_000_000,
                    fee_rate_per_instructions_increment: FIXTURE_FEE_RATE_PER_INSN,
                    tx_memory_limit: 41_943_040,
                },
            )),
        },
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::ContractLedgerCostV0),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::ContractLedgerCostV0(
                ConfigSettingContractLedgerCostV0 {
                    ledger_max_disk_read_entries: 10,
                    ledger_max_disk_read_bytes: 11,
                    ledger_max_write_ledger_entries: 12,
                    ledger_max_write_bytes: 13,
                    tx_max_disk_read_entries: 14,
                    tx_max_disk_read_bytes: 15,
                    tx_max_write_ledger_entries: 16,
                    tx_max_write_bytes: 17,
                    fee_disk_read_ledger_entry: 18,
                    fee_write_ledger_entry: 19,
                    fee_disk_read1_kb: 20,
                    soroban_state_target_size_bytes: 21,
                    rent_fee1_kb_soroban_state_size_low: 22,
                    rent_fee1_kb_soroban_state_size_high: 23,
                    soroban_state_rent_fee_growth_factor: 24,
                },
            )),
        },
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::ContractHistoricalDataV0),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::ContractHistoricalDataV0(
                ConfigSettingContractHistoricalDataV0 {
                    fee_historical1_kb: 30,
                },
            )),
        },
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::ContractEventsV0),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::ContractEventsV0(
                ConfigSettingContractEventsV0 {
                    tx_max_contract_events_size_bytes: 40,
                    fee_contract_events1_kb: 41,
                },
            )),
        },
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::ContractBandwidthV0),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(
                ConfigSettingContractBandwidthV0 {
                    ledger_max_txs_size_bytes: 50,
                    tx_max_size_bytes: 51,
                    fee_tx_size1_kb: FIXTURE_FEE_TX_SIZE_1KB,
                },
            )),
        },
        FixtureEntry {
            key_b64: key(stellar_xdr::ConfigSettingId::StateArchival),
            xdr_b64: value(stellar_xdr::ConfigSettingEntry::StateArchival(
                StateArchivalSettings {
                    max_entry_ttl: 60,
                    min_temporary_ttl: 61,
                    min_persistent_ttl: 62,
                    persistent_rent_rate_denominator: 63,
                    temp_rent_rate_denominator: 64,
                    max_entries_to_archive: 65,
                    live_soroban_state_size_window_sample_size: 66,
                    live_soroban_state_size_window_sample_period: 67,
                    eviction_scan_size: 68,
                    starting_eviction_scan_level: 69,
                },
            )),
        },
    ]
}

/// The `getLedgerEntries` result body: returns exactly the requested keys
/// (like a real node, which omits unknown keys), each stamped with the
/// fixture ledger sequence.
fn get_ledger_entries_result(params: &Value) -> Value {
    let requested = params
        .get("keys")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let entries: Vec<Value> = fixture_config_entries()
        .into_iter()
        .filter(|entry| {
            requested
                .iter()
                .any(|key| key.as_str() == Some(entry.key_b64.as_str()))
        })
        .map(|entry| {
            json!({
                "key": entry.key_b64,
                "xdr": entry.xdr_b64,
                "lastModifiedLedgerSeq": FIXTURE_ENTRY_LEDGER,
            })
        })
        .collect();

    json!({
        "entries": entries,
        "latestLedger": FIXTURE_LATEST_LEDGER,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Library-level tests: simulateTransaction over the mock
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_simulate_roundtrip_against_mock_server() {
    let server = MockRpcServer::with_fixtures();
    let client = RpcClient::new(&server.url.clone());

    // A realistic request: build a genuine upload envelope from the crate's
    // own constructor so the mock sees well-formed params.
    let tx_xdr = soroban_cost_estimator::xdr_helper::build_simulation_tx_envelope(
        b"\0asm-fixture",
        None,
        None,
        &[],
    )
    .expect("envelope should construct");
    let tx_b64 = b64(&tx_xdr);

    let response = simulate_transaction(&client, &tx_b64)
        .await
        .expect("simulation against the mock should succeed");

    assert_eq!(
        response.latest_ledger,
        Some(FIXTURE_LATEST_LEDGER),
        "latestLedger arrives as a JSON string and must deserialize flexibly"
    );
    assert!(response.error.is_none());

    // The full resource set must round-trip through transactionData XDR.
    let resources = parse_transaction_data_resources(&response.transaction_data)
        .expect("resources should parse")
        .expect("resources should be present");
    assert_eq!(resources.cpu_insns, u64::from(FIXTURE_CPU_INSNS));
    assert_eq!(resources.read_entries, 1);
    assert_eq!(resources.write_entries, 2);
    assert_eq!(resources.read_bytes, u64::from(FIXTURE_DISK_READ_BYTES));
    assert_eq!(resources.write_bytes, u64::from(FIXTURE_WRITE_BYTES));

    // Both fee sources carry the same fixture total.
    let min_fee =
        parse_resource_fee(&response.min_resource_fee).expect("minResourceFee should parse");
    assert_eq!(min_fee, Some(FIXTURE_RESOURCE_FEE));
    let xdr_fee = parse_transaction_data_resource_fee(&response.transaction_data)
        .expect("XDR fee should parse");
    assert_eq!(xdr_fee, Some(FIXTURE_RESOURCE_FEE));
}

#[tokio::test]
async fn test_simulate_host_error_maps_to_simulation_failed() {
    let server = MockRpcServer::start(|method, _| match method {
        "simulateTransaction" => rpc_ok(&simulate_error_result("Host function error")),
        other => rpc_error(-32601, &format!("method not found: {other}")),
    });
    let client = RpcClient::new(&server.url.clone());

    let err = simulate_transaction(&client, "AAAA")
        .await
        .expect_err("a simulation error must surface");

    match err {
        AppError::SimulationFailed(message) => {
            assert_eq!(message, "Host function error");
        }
        other => panic!("expected SimulationFailed, got: {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Library-level tests: getLedgerEntries over the mock
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_fetch_all_config_settings_returns_all_six() {
    let server = MockRpcServer::with_fixtures();
    let client = RpcClient::new(&server.url.clone());

    let entries = fetch_all_config_settings(&client)
        .await
        .expect("all six config settings should fetch");

    assert_eq!(entries.len(), 6, "every requested setting must come back");
    assert_eq!(
        entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [
            ConfigSettingId::ContractComputeV0,
            ConfigSettingId::ContractLedgerCostV0,
            ConfigSettingId::ContractHistoricalDataV0,
            ConfigSettingId::ContractEventsV0,
            ConfigSettingId::ContractBandwidthV0,
            ConfigSettingId::StateArchival,
        ],
        "entries must be matched back to their keys in request order"
    );

    // Spot-check the decoded XDR against the fixture values.
    for raw in &entries {
        assert_eq!(
            raw.last_modified_ledger, FIXTURE_ENTRY_LEDGER,
            "lastModifiedLedgerSeq should flow through"
        );
    }

    let compute =
        soroban_cost_estimator::xdr_helper::decode_config_entry_xdr(&entries[0].config_xdr)
            .expect("compute entry should decode");
    match compute {
        stellar_xdr::ConfigSettingEntry::ContractComputeV0(settings) => {
            assert_eq!(
                settings.fee_rate_per_instructions_increment,
                FIXTURE_FEE_RATE_PER_INSN
            );
        }
        other => panic!("expected ContractComputeV0, got {other:?}"),
    }

    let bandwidth =
        soroban_cost_estimator::xdr_helper::decode_config_entry_xdr(&entries[4].config_xdr)
            .expect("bandwidth entry should decode");
    match bandwidth {
        stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(settings) => {
            assert_eq!(settings.fee_tx_size1_kb, FIXTURE_FEE_TX_SIZE_1KB);
        }
        other => panic!("expected ContractBandwidthV0, got {other:?}"),
    }
}

#[tokio::test]
async fn test_fetch_single_config_setting_against_mock() {
    let server = MockRpcServer::with_fixtures();
    let client = RpcClient::new(&server.url.clone());

    // The single-key call must receive back exactly one entry even though
    // the mock knows six — the key filter has to work.
    let raw = fetch_config_setting(&client, ConfigSettingId::StateArchival)
        .await
        .expect("state archival setting should fetch");
    assert_eq!(raw.id, ConfigSettingId::StateArchival);

    let decoded = soroban_cost_estimator::xdr_helper::decode_config_entry_xdr(&raw.config_xdr)
        .expect("state archival entry should decode");
    match decoded {
        stellar_xdr::ConfigSettingEntry::StateArchival(settings) => {
            assert_eq!(settings.max_entry_ttl, 60);
        }
        other => panic!("expected StateArchival, got {other:?}"),
    }
}

#[tokio::test]
async fn test_missing_config_setting_errors_when_node_omits_it() {
    // An empty-ledger node returns no entries for any key.
    let server = MockRpcServer::start(|method, _| match method {
        "getLedgerEntries" => rpc_ok(&json!({"entries": [], "latestLedger": 1})),
        other => rpc_error(-32601, &format!("method not found: {other}")),
    });
    let client = RpcClient::new(&server.url.clone());

    let err = fetch_config_setting(&client, ConfigSettingId::ContractEventsV0)
        .await
        .expect_err("a missing entry must surface");

    match err {
        AppError::ConfigSettingNotFound(name) => {
            assert_eq!(
                name, "CONFIG_SETTING_CONTRACT_EVENTS_V0",
                "the error should name the missing setting"
            );
        }
        other => panic!("expected ConfigSettingNotFound, got: {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Library-level tests: JSON-RPC transport edge cases
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_jsonrpc_error_envelope_maps_to_app_error() {
    let server = MockRpcServer::start(|_, _| rpc_error(-32000, "server overloaded"));
    let client = RpcClient::new(&server.url.clone());

    let err: AppError = client
        .call::<Value>("simulateTransaction", json!({}))
        .await
        .expect_err("an error envelope must surface");

    match err {
        AppError::Rpc { status, message } => {
            assert_eq!(status, -32000);
            assert_eq!(message, "server overloaded");
        }
        other => panic!("expected AppError::Rpc, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_response_without_result_or_error_is_rejected() {
    let server = MockRpcServer::start(|_, _| json!({"jsonrpc": "2.0", "id": 1}));
    let client = RpcClient::new(&server.url.clone());

    let err: AppError = client
        .call::<Value>("simulateTransaction", json!({}))
        .await
        .expect_err("a response with neither result nor error must be rejected");

    match err {
        AppError::Rpc { message, .. } => {
            assert!(
                message.contains("missing 'result'"),
                "the rejection should explain itself; got: {message}"
            );
        }
        other => panic!("expected AppError::Rpc, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_unknown_method_returns_method_not_found() {
    let server = MockRpcServer::with_fixtures();
    let client = RpcClient::new(&server.url.clone());

    let err: AppError = client
        .call::<Value>("notAMethod", json!({}))
        .await
        .expect_err("unknown methods must be rejected by the mock too");

    match err {
        AppError::Rpc { status, message } => {
            assert_eq!(status, -32601);
            assert!(message.contains("notAMethod"));
        }
        other => panic!("expected AppError::Rpc, got: {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// CLI-level tests: the real binary against the mock server
// ─────────────────────────────────────────────────────────────────────────

use std::path::PathBuf;
use std::process::Command;

/// Creates a unique temporary directory for one test, removing any leftover
/// from a previous run. Same pattern as `cli_tests.rs`: dependency-free and
/// enough isolation for `~/.soroban-cost-estimator`.
fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sce-mock-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create temp home");
    dir
}

/// Runs the CLI binary with `HOME` redirected into `home`, so cache and
/// snapshot writes never touch the developer's real directory.
fn run_cli_in_home(args: &[&str], home: &PathBuf) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .output()
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

/// End-to-end: `estimate --json` against the mock produces the exact fixture
/// numbers — HTTP → JSON-RPC → camelCase parse → XDR decode → report.
#[test]
fn test_cli_estimate_against_mock_server() {
    let server = MockRpcServer::with_fixtures();
    let home = temp_home("estimate");

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--network",
            "mocknet",
            "--rpc-url",
            &server.url,
            "--json",
        ],
        &home,
    );

    assert_eq!(
        code, 0,
        "estimate should succeed against the mock; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Warning"),
        "all fee-rate sources are served by the mock, so nothing may degrade; stderr: {stderr}"
    );

    let report: Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be a JSON cost report");
    assert_eq!(report["cpu_instructions"], FIXTURE_CPU_INSNS);
    assert_eq!(report["read_entries"], 1);
    assert_eq!(report["write_entries"], 2);
    assert_eq!(report["read_bytes"], FIXTURE_DISK_READ_BYTES);
    assert_eq!(report["write_bytes"], FIXTURE_WRITE_BYTES);
    assert_eq!(report["ledger"], FIXTURE_LATEST_LEDGER);
    assert_eq!(report["fee"]["total_stroops"], FIXTURE_RESOURCE_FEE);
    // The upload path reports "(wasm upload)" as the function.
    assert_eq!(report["function"], "(wasm upload)");
}

/// The fee-rate degradation path, deterministically: the mock serves
/// simulations but no config entries, so all three rate sources must warn by
/// name while the estimate still completes with the XDR-derived total fee.
#[test]
fn test_cli_estimate_reports_degraded_fee_rate_sources() {
    let server = MockRpcServer::start(|method, _| match method {
        "simulateTransaction" => rpc_ok(&simulate_result()),
        _ => rpc_ok(&json!({"entries": [], "latestLedger": 1})),
    });
    let home = temp_home("degraded");

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--rpc-url",
            &server.url,
            "--json",
        ],
        &home,
    );

    assert_eq!(
        code, 0,
        "degraded fee rates must not fail the command; stderr: {stderr}"
    );
    for source in [
        "ContractComputeV0",
        "ContractLedgerCostV0",
        "ContractBandwidthV0",
    ] {
        assert!(
            stderr.contains(source),
            "the warning should name degraded source {source}; stderr: {stderr}"
        );
    }

    // The total fee still comes from minResourceFee even with zero rates.
    let report: Value = serde_json::from_str(stdout.trim()).expect("JSON cost report");
    assert_eq!(report["fee"]["total_stroops"], FIXTURE_RESOURCE_FEE);
}
