//! Integration tests for the JSON-RPC payload diagnostics (#298).
//!
//! When a simulation fails it is usually impossible to tell *what* the node was
//! asked or *what* it answered from the CLI output alone. `--verbose` (or
//! `RUST_LOG=debug`) now dumps every JSON-RPC request and the raw response.
//!
//! These tests pin the contract of that feature:
//!
//! - the payloads are written to **stderr**, so the data written to stdout
//!   (tables, `--json` documents) is unaffected,
//! - the request names the method and includes the exact body,
//! - the response includes the HTTP status and the raw body verbatim,
//! - credential headers are redacted,
//! - and nothing is printed unless the feature is switched on.
//!
//! They drive the real binary against a loopback mock endpoint, so they stay
//! offline and deterministic.

use std::path::PathBuf;
use std::process::Command;

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A JSON-RPC error body. `estimate` fails fast on `getHealth`, so a run
/// against an endpoint that always answers this reaches the RPC layer and
/// stops there — no valid XDR fixtures needed.
const RPC_ERROR_BODY: &str =
    r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"invalid transaction: boom"}}"#;

/// Starts a mock endpoint that answers every POST with [`RPC_ERROR_BODY`].
async fn start_failing_rpc() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(RPC_ERROR_BODY))
        .mount(&server)
        .await;
    server
}

/// A per-test temporary `HOME`, so the run never reads or writes the
/// developer's real cache and snapshots.
fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sce-rpc-debug-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create temp home");
    dir
}

/// Runs `estimate` against `rpc_url` and captures `(stdout, stderr, code)`.
///
/// `extra_env` is applied last, so a test can set `RUST_LOG` explicitly and not
/// inherit whatever the developer's shell had exported.
fn run_estimate(
    label: &str,
    rpc_url: &str,
    extra_args: &[&str],
    extra_env: &[(&str, &str)],
) -> (String, String, i32) {
    let home = temp_home(label);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd.args([
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--rpc-url",
        rpc_url,
        "--no-cache",
    ])
    .args(extra_args)
    .env("HOME", &home)
    .env("USERPROFILE", &home)
    .env_remove("RUST_LOG");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("failed to run CLI");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[tokio::test]
async fn verbose_prints_request_and_response_payloads_to_stderr() {
    let server = start_failing_rpc().await;
    let (stdout, stderr, code) = run_estimate(
        "verbose",
        &server.uri(),
        &["--verbose", "--header", "Authorization=Bearer super-secret"],
        &[],
    );

    assert_eq!(code, 1, "a failed RPC call must exit 1; stderr: {stderr}");
    // The request: endpoint, method, and the exact body that was sent.
    assert!(
        stderr.contains("[RPC REQ] POST"),
        "the request payload is missing; got: {stderr}"
    );
    assert!(
        stderr.contains("method=getHealth"),
        "the request must name the JSON-RPC method; got: {stderr}"
    );
    assert!(
        stderr.contains("body: {"),
        "the exact request body must be included; got: {stderr}"
    );
    // The response: status and the raw body the node returned.
    assert!(
        stderr.contains("[RPC RES] HTTP 200 OK"),
        "the response payload is missing; got: {stderr}"
    );
    assert!(
        stderr.contains("invalid transaction: boom"),
        "the raw response body must be included verbatim; got: {stderr}"
    );
    // Credentials stay masked, exactly as the `--header` debug log promises.
    assert!(
        !stderr.contains("super-secret"),
        "the bearer token leaked into the log; got: {stderr}"
    );
    assert!(
        stderr.contains("authorization: <redacted>"),
        "the credential header must be masked, not dropped; got: {stderr}"
    );
    // Payload dumps are diagnostics: they must never pollute stdout.
    assert!(
        !stdout.contains("[RPC "),
        "diagnostics must not be written to stdout; got: {stdout}"
    );
}

#[tokio::test]
async fn rust_log_debug_enables_payload_logging_without_verbose() {
    let server = start_failing_rpc().await;
    let (stdout, stderr, code) =
        run_estimate("rustlog", &server.uri(), &[], &[("RUST_LOG", "debug")]);

    assert_eq!(code, 1, "a failed RPC call must exit 1; stderr: {stderr}");
    assert!(
        stderr.contains("[RPC REQ] POST"),
        "RUST_LOG=debug must enable the request payload; got: {stderr}"
    );
    assert!(
        stderr.contains("[RPC RES] HTTP 200 OK"),
        "RUST_LOG=debug must enable the response payload; got: {stderr}"
    );
    assert!(
        stderr.contains("invalid transaction: boom"),
        "the raw response body must be included; got: {stderr}"
    );
    assert!(
        !stdout.contains("[RPC "),
        "diagnostics must not be written to stdout; got: {stdout}"
    );
}

#[tokio::test]
async fn payload_logging_is_opt_in() {
    let server = start_failing_rpc().await;
    let (_, stderr, code) = run_estimate("default", &server.uri(), &[], &[("RUST_LOG", "error")]);

    assert_eq!(code, 1, "a failed RPC call must exit 1; stderr: {stderr}");
    assert!(
        !stderr.contains("[RPC REQ]"),
        "request payloads must stay opt-in; got: {stderr}"
    );
    assert!(
        !stderr.contains("[RPC RES]"),
        "response payloads must stay opt-in; got: {stderr}"
    );
    // The normal, user-facing error is still reported: `health_check` wraps the
    // JSON-RPC failure, so the endpoint and the server's own message both reach
    // the user even though no payload dump is written.
    assert!(
        stderr.contains("unable to reach RPC endpoint"),
        "the regular error message must survive; got: {stderr}"
    );
    assert!(
        stderr.contains("invalid transaction: boom"),
        "the RPC error detail must still reach the user; got: {stderr}"
    );
}
