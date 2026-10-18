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

fn spawn_429_stub(
    retry_after_header: Option<String>,
    fail_times: usize,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind stub server");
    let addr = listener.local_addr().expect("no local address");
    let counter = Arc::new(AtomicUsize::new(0));
    let server_counter = Arc::clone(&counter);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let call_idx = server_counter.fetch_add(1, Ordering::SeqCst);
            let header = retry_after_header.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let response = if call_idx < fail_times {
                    let retry_line = header
                        .map(|h| format!("Retry-After: {h}\r\n"))
                        .unwrap_or_default();
                    let body = r#"{"error":"rate limited"}"#;
                    format!(
                        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\n{retry_line}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            });
        }
    });

    (format!("http://{addr}"), counter)
}

#[tokio::test]
async fn honors_retry_after_delta_seconds_and_http_date_and_max_retries_zero() {
    use soroban_cost_estimator::error::AppError;
    use soroban_cost_estimator::rpc::retry::MAX_RETRY_AFTER_DELAY;

    // 1. Valid delta-seconds (`Retry-After: 0`) retries immediately and succeeds.
    let (url_delta, calls_delta) = spawn_429_stub(Some("0".to_string()), 1);
    let client_delta = RpcClient::with_options(&url_delta, None, Duration::from_secs(5), 2, false);
    let res: Value = client_delta
        .call("getHealth", serde_json::json!({}))
        .await
        .expect("429 with Retry-After: 0 must retry and succeed");
    assert_eq!(res["status"], "healthy");
    assert_eq!(calls_delta.load(Ordering::SeqCst), 2);

    // 2. Valid HTTP-date in the past clamps to 0s and retries immediately.
    let (url_date, calls_date) =
        spawn_429_stub(Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()), 1);
    let client_date = RpcClient::with_options(&url_date, None, Duration::from_secs(5), 2, false);
    let res_date: Value = client_date
        .call("getHealth", serde_json::json!({}))
        .await
        .expect("429 with past HTTP-date Retry-After must retry and succeed");
    assert_eq!(res_date["status"], "healthy");
    assert_eq!(calls_date.load(Ordering::SeqCst), 2);

    // 3. Malformed and absent Retry-After fall back to exponential backoff and succeed.
    let (url_malformed, calls_malformed) =
        spawn_429_stub(Some("invalid-header-value".to_string()), 1);
    let client_malformed =
        RpcClient::with_options(&url_malformed, None, Duration::from_secs(5), 1, false);
    let res_malformed: Value = client_malformed
        .call("getHealth", serde_json::json!({}))
        .await
        .expect("429 with malformed Retry-After must fall back to backoff and succeed");
    assert_eq!(res_malformed["status"], "healthy");
    assert_eq!(calls_malformed.load(Ordering::SeqCst), 2);

    // 4. Excessive Retry-After with max_retries=0 does not sleep and surfaces capped retry_after.
    let (url_excessive, calls_excessive) = spawn_429_stub(Some("999999".to_string()), 2);
    let client_no_retry =
        RpcClient::with_options(&url_excessive, None, Duration::from_secs(5), 0, false);
    let err = client_no_retry
        .call::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect_err("max_retries=0 must fail immediately on 429");
    assert_eq!(calls_excessive.load(Ordering::SeqCst), 1);
    match err {
        AppError::HttpStatus {
            status,
            retry_after,
            ..
        } => {
            assert_eq!(status, 429);
            assert_eq!(retry_after, Some(MAX_RETRY_AFTER_DELAY));
        }
        other => panic!("expected AppError::HttpStatus, got {other:?}"),
    }
}
