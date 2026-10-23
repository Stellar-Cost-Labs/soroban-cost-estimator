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

/// Spawns a stub server that delays for `delay` before returning `status` and
/// `body`.
fn spawn_delayed_status_stub(
    delay: Duration,
    status_line: &'static str,
    body: &'static str,
) -> (String, Arc<AtomicUsize>) {
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
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                let response = format!(
                    "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
async fn overall_deadline_cancels_delayed_primary_before_per_attempt_timeout() {
    use soroban_cost_estimator::error::AppError;

    let (primary_url, hits) = spawn_delayed_status_stub(
        Duration::from_millis(800),
        "HTTP/1.1 200 OK",
        r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}"#,
    );

    let client = RpcClient::with_options(&primary_url, None, Duration::from_secs(10), 0, false)
        .with_total_timeout(Some(Duration::from_millis(150)));

    let start = tokio::time::Instant::now();
    let err = client
        .call_uncached::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect_err("delayed primary must exceed overall deadline");
    let elapsed = start.elapsed();

    assert!(
        matches!(err, AppError::TotalTimeout { timeout_ms: 150 }),
        "expected TotalTimeout(150), got: {err:?}"
    );
    assert!(
        elapsed < Duration::from_millis(550),
        "operation should abort near 150ms deadline, took {elapsed:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn overall_deadline_stops_retry_backoff_and_returns_total_timeout() {
    use soroban_cost_estimator::error::AppError;

    // Immediate 503 triggers retry backoff (500ms..1000ms), which exceeds the
    // 180ms overall deadline before the second attempt can run.
    let (primary_url, hits) = spawn_delayed_status_stub(
        Duration::ZERO,
        "HTTP/1.1 503 Service Unavailable",
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"busy"}}"#,
    );

    let client = RpcClient::with_options(&primary_url, None, Duration::from_secs(5), 3, false)
        .with_total_timeout(Some(Duration::from_millis(180)));

    let start = tokio::time::Instant::now();
    let err = client
        .call_uncached::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect_err("retries must stop when overall deadline expires");
    let elapsed = start.elapsed();

    assert!(
        matches!(err, AppError::TotalTimeout { timeout_ms: 180 }),
        "expected TotalTimeout(180), got: {err:?}"
    );
    assert!(
        elapsed < Duration::from_millis(450),
        "backoff sleep must be bounded by overall deadline, took {elapsed:?}"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "second retry attempt must not fire after overall deadline expires"
    );
}

#[tokio::test]
async fn overall_deadline_spans_primary_and_fallback_attempts() {
    use soroban_cost_estimator::error::AppError;

    // Primary takes 100ms then returns 503 (triggering failover); fallback takes
    // 500ms, so the combined operation exceeds the 250ms total deadline.
    let (primary_url, primary_hits) = spawn_delayed_status_stub(
        Duration::from_millis(100),
        "HTTP/1.1 503 Service Unavailable",
        "gateway down",
    );
    let (fallback_url, fallback_hits) = spawn_delayed_status_stub(
        Duration::from_millis(500),
        "HTTP/1.1 200 OK",
        r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy"}}"#,
    );

    let client = RpcClient::with_fallback(
        &primary_url,
        Some(&fallback_url),
        None,
        Duration::from_secs(5),
        0,
        false,
    )
    .with_total_timeout(Some(Duration::from_millis(250)));

    let start = tokio::time::Instant::now();
    let err = client
        .call_uncached::<Value>("getHealth", serde_json::json!({}))
        .await
        .expect_err("combined primary + fallback delay must hit overall deadline");
    let elapsed = start.elapsed();

    assert!(
        matches!(err, AppError::TotalTimeout { timeout_ms: 250 }),
        "expected TotalTimeout(250), got: {err:?}"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "combined primary + fallback must stop at overall deadline, took {elapsed:?}"
    );
    assert_eq!(primary_hits.load(Ordering::SeqCst), 1);
    assert_eq!(fallback_hits.load(Ordering::SeqCst), 1);
}
