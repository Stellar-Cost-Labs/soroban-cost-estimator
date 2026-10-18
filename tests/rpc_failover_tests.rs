//! Integration tests for RPC endpoint failover (`--rpc-fallback-url`, #306).
//!
//! These drive the crate's public [`RpcClient`] and the built CLI binary
//! against a primary endpoint that cannot be reached, proving the client
//! automatically fails over to the configured fallback and announces the
//! failover on stderr. Everything runs against loopback stubs, so the tests
//! are deterministic and never contact a live network.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::Value;
use soroban_cost_estimator::error::AppResult;
use soroban_cost_estimator::rpc::client::RpcClient;

/// A hostname that is guaranteed never to resolve (RFC 6761 reserves the
/// `.invalid` TLD), standing in for a misconfigured/unreachable primary
/// endpoint without depending on a live network.
const INVALID_PRIMARY: &str = "http://primary.invalid:8080";

/// The exact stderr notice promised by the CLI contract for issue #306.
const FAILOVER_NOTICE: &str = "Primary RPC failed, failing over to fallback endpoint:";

/// Starts a minimal blocking JSON-RPC stub on an ephemeral loopback port.
///
/// Every request is answered with `{"result":{"status":"healthy"}}`, which is
/// sufficient for both `getHealth` and the generic `Value` used by the client
/// test. Returns the stub URL and a counter of the requests it served.
fn spawn_rpc_stub() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind stub server");
    let addr = listener.local_addr().expect("no local address");
    let counter = Arc::new(AtomicUsize::new(0));
    let server_counter = Arc::clone(&counter);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let counter = Arc::clone(&server_counter);
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                counter.fetch_add(1, Ordering::SeqCst);
                let body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            });
        }
    });

    (format!("http://{addr}"), counter)
}

/// When the primary endpoint points at an invalid host (DNS never resolves)
/// and a fallback URL is configured, a request must transparently fail over
/// and succeed against the fallback.
#[tokio::test]
async fn fails_over_to_fallback_when_primary_host_is_invalid() {
    let (fallback_url, served) = spawn_rpc_stub();
    let client = RpcClient::with_fallback(
        INVALID_PRIMARY,
        Some(&fallback_url),
        None,
        Duration::from_secs(10),
        // No retries: the primary is unreachable, so retrying it would only
        // lengthen the test without exercising a different code path.
        0,
        false,
    );

    let result: AppResult<Value> = client.call("getHealth", serde_json::json!({})).await;

    assert!(
        result.is_ok(),
        "request must succeed via the fallback endpoint, got: {result:?}"
    );
    assert_eq!(
        served.load(Ordering::SeqCst),
        1,
        "fallback stub should have served exactly one request"
    );
}

/// End-to-end: the real CLI, given an invalid primary and a healthy fallback,
/// announces the failover on stderr and actually reaches the fallback.
#[test]
fn cli_announces_failover_on_stderr() {
    let (fallback_url, served) = spawn_rpc_stub();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "--rpc-fallback-url",
            &fallback_url,
            "--max-retries",
            "0",
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--rpc-url",
            INVALID_PRIMARY,
        ])
        .output()
        .expect("failed to run CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(FAILOVER_NOTICE),
        "stderr must announce the failover; got: {stderr}"
    );
    assert!(
        stderr.contains(&fallback_url),
        "stderr notice must name the fallback endpoint; got: {stderr}"
    );
    assert!(
        served.load(Ordering::SeqCst) >= 1,
        "the fallback endpoint must have been contacted"
    );
}

#[tokio::test]
async fn call_with_outcome_records_primary_or_fallback_and_sanitizes_host() {
    use soroban_cost_estimator::rpc::client::{EndpointRole, sanitize_endpoint_host};

    let (stub_url, _served) = spawn_rpc_stub();
    let expected_host = sanitize_endpoint_host(&stub_url);

    // Primary succeeds with credentials and query parameters in URL.
    let primary_with_secrets = format!("{stub_url}/rpc?api_key=super-secret-token#frag").replacen(
        "http://",
        "http://operator:topsecret@",
        1,
    );
    let primary_client = RpcClient::with_fallback(
        &primary_with_secrets,
        None,
        None,
        Duration::from_secs(5),
        0,
        false,
    );
    let primary_outcome = primary_client
        .call_with_outcome::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect("primary call must succeed");
    assert_eq!(primary_outcome.endpoint.role, EndpointRole::Primary);
    assert!(!primary_outcome.endpoint.failed_over);
    assert_eq!(primary_outcome.endpoint.host, expected_host);
    assert!(!primary_outcome.endpoint.host.contains("topsecret"));
    assert!(!primary_outcome.endpoint.host.contains("super-secret-token"));

    // Primary fails -> fallback with credentials and query parameters serves response.
    let fallback_with_secrets = format!("{stub_url}/v1?token=fallback-secret-key").replacen(
        "http://",
        "http://backup_user:backup_pass@",
        1,
    );
    let failover_client = RpcClient::with_fallback(
        INVALID_PRIMARY,
        Some(&fallback_with_secrets),
        None,
        Duration::from_secs(5),
        0,
        false,
    );
    let fallback_outcome = failover_client
        .call_with_outcome::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect("fallback call must succeed");
    assert_eq!(fallback_outcome.endpoint.role, EndpointRole::Fallback);
    assert!(fallback_outcome.endpoint.failed_over);
    assert_eq!(fallback_outcome.endpoint.host, expected_host);
    assert!(!fallback_outcome.endpoint.host.contains("backup_pass"));
    assert!(
        !fallback_outcome
            .endpoint
            .host
            .contains("fallback-secret-key")
    );
}

#[test]
fn report_formatters_render_sanitized_endpoint_without_credentials() {
    use soroban_cost_estimator::report::cost_report::CostReport;
    use soroban_cost_estimator::report::fee_calc::FeeBreakdown;
    use soroban_cost_estimator::report::formatter::{
        JsonFormatter, MarkdownFormatter, ReportFormatter, TableFormatter,
    };
    use soroban_cost_estimator::rpc::client::{
        EndpointRole, RpcEndpointMetadata, sanitize_endpoint_host,
    };
    use soroban_cost_estimator::wasm::parser::ContractMeta;

    let raw_url = "https://user:secret_password@rpc-fallback.example.org:8443/soroban?api_key=secret_query_token";
    let endpoint = RpcEndpointMetadata {
        role: EndpointRole::Fallback,
        host: sanitize_endpoint_host(raw_url),
        failed_over: true,
    };
    assert_eq!(endpoint.host, "rpc-fallback.example.org:8443");

    let report = CostReport {
        function: "increment".to_string(),
        wasm_hash: "abc123".to_string(),
        wasm_size: 512,
        wasm_sections: Vec::new(),
        cpu_instructions: 1_000,
        memory_bytes: 512,
        tx_size: 120,
        read_entries: 1,
        write_entries: 1,
        read_bytes: 64,
        write_bytes: 64,
        fee: FeeBreakdown {
            non_refundable_stroops: 500,
            refundable_stroops: 500,
            cpu_fee_stroops: 100,
            storage_fee_stroops: 200,
            bandwidth_fee_stroops: 100,
            base_fee_stroops: 100,
            total_stroops: 1_000,
            total_xlm: "0.0001000".to_string(),
            fee_percentages: std::collections::BTreeMap::new(),
        },
        ledger: 42,
        network: "testnet".to_string(),
        rpc_latency_ms: 15,
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
        rpc_endpoint: Some(endpoint),
    };

    let table = TableFormatter.format(&report);
    let json = JsonFormatter.format(&report);
    let markdown = MarkdownFormatter.format(&report);

    assert!(
        table.contains("RPC endpoint: rpc-fallback.example.org:8443 (fallback, failover)"),
        "table output should show sanitized endpoint and failover role: {table}"
    );
    assert!(
        markdown
            .contains("- **RPC endpoint:** `rpc-fallback.example.org:8443` (fallback, failover)"),
        "markdown output should show sanitized endpoint and failover role: {markdown}"
    );
    let parsed: Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed["rpc_endpoint"]["role"], "fallback");
    assert_eq!(
        parsed["rpc_endpoint"]["host"],
        "rpc-fallback.example.org:8443"
    );
    assert_eq!(parsed["rpc_endpoint"]["failed_over"], true);

    for rendered in [&table, &json, &markdown] {
        assert!(!rendered.contains("secret_password"));
        assert!(!rendered.contains("secret_query_token"));
    }
}
