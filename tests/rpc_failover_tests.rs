//! Integration tests for RPC endpoint failover (`--rpc-fallback-url`, #306)
//! and endpoint provenance metadata (`rpc_endpoint`, `rpc_role`, #501).
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
use soroban_cost_estimator::report::cost_report::{CostReport, FeeBreakdown};
use soroban_cost_estimator::report::formatter::{JsonFormatter, ReportFormatter, TableFormatter};
use soroban_cost_estimator::wasm::parser::ContractMeta;

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

// ── Endpoint provenance tests (#501) ────────────────────────────────────────

/// When the primary endpoint serves the request successfully, `served_role`
/// must return `"primary"` and `sanitized_host` must return its host:port.
#[tokio::test]
async fn primary_serves_request_role_is_primary() {
    let (primary_url, _) = spawn_rpc_stub();
    let client = RpcClient::with_fallback(
        &primary_url,
        None,
        None,
        Duration::from_secs(10),
        0,
        false,
    );

    let result: AppResult<Value> = client.call("getHealth", serde_json::json!({})).await;
    assert!(result.is_ok(), "primary must serve the request: {result:?}");

    assert_eq!(
        client.served_role(),
        "primary",
        "role must be 'primary' when primary endpoint serves the request"
    );
    let host = client.sanitized_host();
    assert!(
        !host.is_empty(),
        "sanitized host must not be empty for a primary request"
    );
    // The sanitized host must not contain the scheme or any credentials.
    assert!(
        !host.starts_with("http"),
        "sanitized host must not contain the URL scheme; got: {host}"
    );
    assert!(
        !host.contains('@'),
        "sanitized host must not contain user-info (credentials); got: {host}"
    );
    assert!(
        !host.contains('?'),
        "sanitized host must not contain query parameters; got: {host}"
    );
}

/// When the primary is unreachable and the fallback serves the request,
/// `served_role` must return `"fallback"` and `sanitized_host` must
/// return the fallback's host:port, not the primary's.
#[tokio::test]
async fn fallback_serves_request_role_is_fallback() {
    let (fallback_url, _) = spawn_rpc_stub();
    let client = RpcClient::with_fallback(
        INVALID_PRIMARY,
        Some(&fallback_url),
        None,
        Duration::from_secs(10),
        0,
        false,
    );

    let result: AppResult<Value> = client.call("getHealth", serde_json::json!({})).await;
    assert!(
        result.is_ok(),
        "fallback must serve the request: {result:?}"
    );

    assert_eq!(
        client.served_role(),
        "fallback",
        "role must be 'fallback' when the fallback endpoint serves the request"
    );

    // The sanitized host must identify the fallback (loopback), not the
    // invalid primary.
    let host = client.sanitized_host();
    assert!(
        host.starts_with("127.0.0.1"),
        "sanitized host must be the fallback's loopback address; got: {host}"
    );
    assert!(
        !host.contains("primary.invalid"),
        "sanitized host must not expose the primary hostname; got: {host}"
    );
}

/// `sanitized_host` must strip URL credentials (user:password@) so they are
/// never rendered in human or structured reports. This is a security-critical
/// requirement from the issue acceptance criteria.
#[test]
fn sanitized_host_strips_url_credentials() {
    // Build a client whose URL embeds credentials in the userinfo component.
    // We do not actually connect here — we are only testing the sanitization
    // function, not the network path.
    let client = RpcClient::with_fallback(
        "http://user:secret@192.0.2.1:8080/rpc?apikey=tok",
        None,
        None,
        Duration::from_secs(5),
        0,
        false,
    );

    let host = client.sanitized_host();

    // Must not expose credentials or query parameters.
    assert!(
        !host.contains("secret"),
        "sanitized host must not contain the password; got: {host}"
    );
    assert!(
        !host.contains("user"),
        "sanitized host must not contain the username; got: {host}"
    );
    assert!(
        !host.contains("apikey"),
        "sanitized host must not contain query parameters; got: {host}"
    );
    assert!(
        !host.contains("tok"),
        "sanitized host must not contain query parameter values; got: {host}"
    );
    // Must keep the useful host:port identifier.
    assert!(
        host.contains("192.0.2.1"),
        "sanitized host must preserve the IP address; got: {host}"
    );
}

/// Authorization headers are never exposed through `sanitized_host` because
/// it only returns the URL's host component — header contents are a separate
/// concern handled by `redact_sensitive`. This test documents that the two
/// mechanisms are complementary: URL credentials are stripped by
/// `sanitized_host`, and header credentials are stripped by `redact_sensitive`.
#[test]
fn sanitized_host_does_not_expose_authorization_headers() {
    // A client with a bearer token in the Authorization header.
    let client = RpcClient::with_headers(
        "http://rpc.example.com:8080",
        &["Authorization=Bearer super-secret-token".to_string()],
        false,
    )
    .expect("valid headers");

    let host = client.sanitized_host();

    // The host must only contain the domain, not anything from headers.
    assert!(
        !host.contains("super-secret-token"),
        "sanitized host must not expose Authorization header values; got: {host}"
    );
    assert!(
        !host.contains("Bearer"),
        "sanitized host must not expose Authorization header values; got: {host}"
    );
    assert_eq!(
        host, "rpc.example.com:8080",
        "sanitized host must be exactly the host:port; got: {host}"
    );
}

/// Integration test verifying that fallback success records the correct
/// `rpc_endpoint` and `rpc_role` in the CostReport, and that any credentials,
/// query tokens, and Authorization headers in the underlying URLs are
/// properly sanitized and never rendered in human or structured reports (#501).
#[tokio::test]
async fn fallback_success_records_provenance_and_scrubs_credentials_in_cost_report() {
    let (fallback_url, served) = spawn_rpc_stub();
    // Embed sensitive userinfo and query string tokens in both endpoints,
    // and provide an Authorization header with a secret token.
    let primary_with_creds = "http://primary_user:primary_secret@primary.invalid:8080/rpc?token=prim_secret_tok";
    let fallback_with_creds = format!(
        "{}/rpc?key=fallback_secret_token",
        fallback_url.replace("http://", "http://fallback_user:fallback_pass@")
    );
    let secret_auth = "Authorization=Bearer ultra-secret-jwt-token";

    let client = RpcClient::with_fallback_headers(
        primary_with_creds,
        Some(&fallback_with_creds),
        None,
        Duration::from_secs(10),
        0,
        &[secret_auth.to_string()],
        false,
    )
    .expect("valid client config");

    let result: AppResult<Value> = client.call("getHealth", serde_json::json!({})).await;
    assert!(result.is_ok(), "fallback should successfully serve the request: {result:?}");
    assert_eq!(served.load(Ordering::SeqCst), 1, "fallback server must have served 1 request");

    // Endpoint provenance asserts
    assert_eq!(client.served_role(), "fallback");
    let sanitized_host = client.sanitized_host();

    assert!(
        sanitized_host.starts_with("127.0.0.1:"),
        "sanitized host should be fallback loopback host:port; got: {sanitized_host}"
    );
    // Ensure all secrets are absent from sanitized host
    assert!(!sanitized_host.contains("primary_secret"));
    assert!(!sanitized_host.contains("primary_user"));
    assert!(!sanitized_host.contains("fallback_secret"));
    assert!(!sanitized_host.contains("fallback_user"));
    assert!(!sanitized_host.contains("fallback_pass"));
    assert!(!sanitized_host.contains("fallback_secret_token"));
    assert!(!sanitized_host.contains("prim_secret_tok"));
    assert!(!sanitized_host.contains("ultra-secret-jwt-token"));
    assert!(!sanitized_host.contains('@'));
    assert!(!sanitized_host.contains('?'));

    // Construct CostReport using client's provenance
    let report = CostReport {
        function: "transfer".to_string(),
        wasm_hash: "0123456789abcdef".to_string(),
        wasm_size: 2048,
        wasm_sections: Vec::new(),
        cpu_instructions: 250_000,
        memory_bytes: 4096,
        tx_size: 512,
        read_entries: 2,
        write_entries: 2,
        read_bytes: 128,
        write_bytes: 128,
        fee: FeeBreakdown {
            non_refundable_stroops: 5000,
            refundable_stroops: 10000,
            cpu_fee_stroops: 500,
            storage_fee_stroops: 1000,
            bandwidth_fee_stroops: 100,
            base_fee_stroops: 100,
            total_stroops: 16700,
            total_xlm: "0.0016700".to_string(),
            fee_percentages: std::collections::BTreeMap::new(),
        },
        ledger: 123456,
        network: "testnet".to_string(),
        simulation_duration_ms: 45,
        rpc_endpoint: sanitized_host.clone(),
        rpc_role: client.served_role().to_string(),
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    };

    assert_eq!(report.rpc_role, "fallback");
    assert_eq!(report.rpc_endpoint, sanitized_host);

    // Verify human-readable table report
    let table_output = TableFormatter.format(&report);
    assert!(
        table_output.contains(&format!("RPC endpoint: {sanitized_host} (fallback)")),
        "table must display sanitized endpoint and role; got:\n{table_output}"
    );
    assert!(!table_output.contains("primary_secret"));
    assert!(!table_output.contains("primary_user"));
    assert!(!table_output.contains("fallback_secret"));
    assert!(!table_output.contains("fallback_user"));
    assert!(!table_output.contains("fallback_pass"));
    assert!(!table_output.contains("fallback_secret_token"));
    assert!(!table_output.contains("prim_secret_tok"));
    assert!(!table_output.contains("ultra-secret-jwt-token"));
    assert!(!table_output.contains("Bearer"));

    // Verify structured JSON report
    let json_output = JsonFormatter.format(&report);
    assert!(
        json_output.contains(&format!(r#""rpc_endpoint": "{sanitized_host}""#)),
        "JSON output must include sanitized rpc_endpoint; got:\n{json_output}"
    );
    assert!(
        json_output.contains(r#""rpc_role": "fallback""#),
        "JSON output must include rpc_role: fallback; got:\n{json_output}"
    );
    assert!(!json_output.contains("primary_secret"));
    assert!(!json_output.contains("fallback_pass"));
    assert!(!json_output.contains("fallback_secret_token"));
    assert!(!json_output.contains("ultra-secret-jwt-token"));
}
