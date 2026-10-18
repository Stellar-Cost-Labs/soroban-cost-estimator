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

fn spawn_custom_jsonrpc_stub(responses: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind stub server");
    let addr = listener.local_addr().expect("no local address");
    let counter = Arc::new(AtomicUsize::new(0));
    let server_counter = Arc::clone(&counter);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let idx = server_counter.fetch_add(1, Ordering::SeqCst);
            let body = responses
                .get(idx)
                .copied()
                .or_else(|| responses.last().copied())
                .unwrap_or(r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}"#);
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
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

#[tokio::test]
async fn transient_json_rpc_error_retries_and_fails_over_to_fallback() {
    use soroban_cost_estimator::rpc::retry::{JsonRpcErrorClass, classify_json_rpc_error};

    assert_eq!(
        classify_json_rpc_error(-32004, None),
        JsonRpcErrorClass::TransientServer
    );
    assert_eq!(
        classify_json_rpc_error(-32602, None),
        JsonRpcErrorClass::InvalidRequest
    );
    assert_eq!(
        classify_json_rpc_error(-32099, None),
        JsonRpcErrorClass::Application
    );

    // Primary returns transient server error (-32004) on every attempt;
    // client retries once on primary (2 attempts total) and then fails over to fallback.
    let (primary_url, primary_calls) = spawn_custom_jsonrpc_stub(vec![
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32004,"message":"server overloaded","data":{"retryable":true,"queueDepth":99}}}"#,
    ]);
    let (fallback_url, fallback_calls) = spawn_rpc_stub();

    let client = RpcClient::with_fallback(
        &primary_url,
        Some(&fallback_url),
        None,
        Duration::from_secs(5),
        1,
        false,
    );

    let result: Value = client
        .call("getHealth", serde_json::json!({}))
        .await
        .expect("transient JSON-RPC error must fail over to fallback after retries");
    assert_eq!(result["status"], "healthy");
    assert_eq!(primary_calls.load(Ordering::SeqCst), 2);
    assert_eq!(fallback_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn non_retryable_json_rpc_error_makes_one_request_and_preserves_structured_data() {
    use soroban_cost_estimator::error::AppError;

    // Invalid params (-32602) with structured `data`: must not retry and must not hit fallback.
    let (primary_url, primary_calls) = spawn_custom_jsonrpc_stub(vec![
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"invalid transaction XDR","data":{"field":"transaction","reason":"corrupt base64"}}}"#,
    ]);
    let (fallback_url, fallback_calls) = spawn_rpc_stub();

    let client = RpcClient::with_fallback(
        &primary_url,
        Some(&fallback_url),
        None,
        Duration::from_secs(5),
        3,
        false,
    );

    let err = client
        .call::<Value>("simulateTransaction", serde_json::json!({}))
        .await
        .expect_err("invalid params must fail immediately");

    assert_eq!(
        primary_calls.load(Ordering::SeqCst),
        1,
        "non-retryable JSON-RPC error must make only one request"
    );
    assert_eq!(
        fallback_calls.load(Ordering::SeqCst),
        0,
        "non-retryable JSON-RPC error must not fail over to fallback"
    );

    let rendered = err.to_string();
    assert!(
        rendered.contains("-32602") && rendered.contains("invalid transaction XDR"),
        "user-facing message must retain code and message: {rendered}"
    );
    match err {
        AppError::JsonRpcError {
            code,
            message,
            data,
            retryable,
        } => {
            assert_eq!(code, -32602);
            assert_eq!(message, "invalid transaction XDR");
            assert!(!retryable);
            let data = data.expect("structured error data must be preserved");
            assert_eq!(data["field"], "transaction");
            assert_eq!(data["reason"], "corrupt base64");
        }
        other => panic!("expected AppError::JsonRpcError, got {other:?}"),
    }
}
