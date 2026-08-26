//! End-to-end integration tests for every CLI command.
//!
//! These tests drive the real binary. They are deliberately **offline**: no
//! test contacts a live RPC endpoint. Network-touching code paths are
//! exercised by pointing `--rpc-url` at a closed local port, or by failing
//! earlier on argument/file/network-name validation, so the suite is
//! deterministic in CI and on a laptop with no connectivity.
//!
//! Commands that read or write `~/.soroban-cost-estimator` run with `HOME`
//! (and `USERPROFILE` on Windows) redirected into a per-test temporary
//! directory, so they never see — or clobber — the developer's real
//! snapshots and cache.

use std::path::{Path, PathBuf};
use std::process::Command;

/// An RPC URL that is guaranteed not to answer: port 1 on loopback.
/// Used to drive the network error path without leaving the machine.
const DEAD_RPC: &str = "http://127.0.0.1:1";

/// Helper to run the CLI binary and capture stdout/stderr/exit code.
fn run_cli(args: &[&str]) -> (String, String, i32) {
    run_cli_in_home(args, None)
}

/// Runs the CLI with `HOME` pointed at `home`, isolating the snapshot and
/// cache directories from the developer's real ones.
fn run_cli_in_home(args: &[&str], home: Option<&Path>) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd.args(args);
    if let Some(home) = home {
        cmd.env("HOME", home);
        // `dirs::home_dir()` reads USERPROFILE on Windows.
        cmd.env("USERPROFILE", home);
    }

    let output = cmd.output().expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

/// Creates a unique temporary directory for one test, removing any leftover
/// from a previous run. Kept dependency-free on purpose — the crate has no
/// dev-dependencies and this is all the isolation the suite needs.
fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sce-it-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("failed to create temp home");
    dir
}

/// A minimal but structurally valid config snapshot, matching `ConfigSnapshot`.
fn snapshot_json(network: &str, ledger: u32) -> String {
    format!(
        r#"{{
  "network": "{network}",
  "ledger": {ledger},
  "timestamp": "2026-01-01T00:00:00+00:00",
  "contract_compute": null,
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}}"#
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Help / discovery
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_help_output() {
    let (stdout, stderr, code) = run_cli(&["--help"]);
    assert_eq!(code, 0, "help should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("estimate"),
        "help should list estimate command"
    );
    assert!(
        stdout.contains("estimate-all"),
        "help should list estimate-all"
    );
    assert!(stdout.contains("config"), "help should list config command");
    assert!(stdout.contains("watch"), "help should list watch command");
}

#[test]
fn test_no_args_prints_usage_and_errors() {
    let (_, stderr, code) = run_cli(&[]);
    assert_ne!(code, 0, "running with no subcommand should exit non-zero");
    assert!(
        stderr.contains("Usage"),
        "no-args invocation should print usage; stderr: {stderr}"
    );
}

#[test]
fn test_short_help_flag() {
    let (stdout, stderr, code) = run_cli(&["-h"]);
    assert_eq!(code, 0, "-h should exit 0; stderr: {stderr}");
    assert!(stdout.contains("Usage"), "-h should print usage");
}

#[test]
fn test_estimate_help() {
    let (stdout, stderr, code) = run_cli(&["estimate", "--help"]);
    assert_eq!(code, 0, "estimate --help should exit 0; stderr: {stderr}");
    for flag in [
        "--wasm",
        "--network",
        "--rpc-url",
        "--fn",
        "--id",
        "--arg",
        "--json",
    ] {
        assert!(
            stdout.contains(flag),
            "estimate help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_estimate_all_help() {
    let (stdout, stderr, code) = run_cli(&["estimate-all", "--help"]);
    assert_eq!(
        code, 0,
        "estimate-all --help should exit 0; stderr: {stderr}"
    );
    for flag in ["--wasm", "--network", "--id", "--json"] {
        assert!(
            stdout.contains(flag),
            "estimate-all help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_config_help() {
    let (stdout, stderr, code) = run_cli(&["config", "--help"]);
    assert_eq!(code, 0, "config --help should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("snapshot"),
        "config help should list snapshot"
    );
    assert!(stdout.contains("diff"), "config help should list diff");
}

#[test]
fn test_config_snapshot_help() {
    let (stdout, stderr, code) = run_cli(&["config", "snapshot", "--help"]);
    assert_eq!(
        code, 0,
        "config snapshot --help should exit 0; stderr: {stderr}"
    );
    for flag in ["--network", "--out", "--json"] {
        assert!(
            stdout.contains(flag),
            "snapshot help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_config_diff_help() {
    let (stdout, stderr, code) = run_cli(&["config", "diff", "--help"]);
    assert_eq!(
        code, 0,
        "config diff --help should exit 0; stderr: {stderr}"
    );
    for flag in ["--network", "--against"] {
        assert!(
            stdout.contains(flag),
            "diff help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_watch_help() {
    let (stdout, stderr, code) = run_cli(&["watch", "--help"]);
    assert_eq!(code, 0, "watch --help should exit 0; stderr: {stderr}");
    for flag in ["--network", "--interval"] {
        assert!(
            stdout.contains(flag),
            "watch help should mention {flag}; got: {stdout}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Argument parsing errors
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_help_output_lists_cache() {
    let (stdout, stderr, code) = run_cli(&["--help"]);
    assert_eq!(code, 0, "help should exit 0; stderr: {stderr}");
    assert!(stdout.contains("cache"), "help should list cache command");
}

#[test]
fn test_cache_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "--help"]);
    assert_eq!(code, 0, "cache --help should exit 0; stderr: {stderr}");
    assert!(stdout.contains("verify"), "cache help should list verify");
}

#[test]
fn test_cache_verify_empty_cache_succeeds() {
    // Run against a temp HOME so we don't touch the real user's cache.
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "soroban_cli_verify_test_{}_{}",
        std::process::id(),
        suffix
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp home");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["cache", "verify"])
        .env("HOME", &tmp)
        .output()
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let code = output.status.code().unwrap_or(-1);

    let _ = std::fs::remove_dir_all(&tmp);

    assert_eq!(
        code, 0,
        "verify on empty cache should exit 0; stdout: {stdout}"
    );
    assert!(
        stdout.contains("empty") || stdout.contains("nothing to verify"),
        "should report an empty cache: {stdout}"
    );
}

#[test]
fn test_estimate_missing_wasm_errors() {
    let (_, stderr, code) = run_cli(&["estimate"]);
    assert_ne!(code, 0, "estimate without --wasm should error");
    assert!(
        stderr.contains("error") || stderr.contains("required"),
        "stderr should indicate error: {stderr}"
    );
}

#[test]
fn test_unknown_command_errors() {
    let (_, stderr, code) = run_cli(&["nonexistent"]);
    assert_ne!(code, 0, "unknown command should exit non-zero");
    assert!(
        stderr.to_lowercase().contains("error") || stderr.to_lowercase().contains("unrecognized"),
        "stderr should indicate error: {stderr}"
    );
}

#[test]
fn test_unknown_config_subcommand_errors() {
    let (_, stderr, code) = run_cli(&["config", "nonexistent"]);
    assert_ne!(code, 0, "unknown config subcommand should exit non-zero");
    assert!(
        stderr.to_lowercase().contains("error"),
        "stderr should indicate error: {stderr}"
    );
}

#[test]
fn test_unknown_flag_errors() {
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "x.wasm", "--not-a-flag"]);
    assert_ne!(code, 0, "unknown flag should exit non-zero");
    assert!(
        stderr.contains("unexpected argument") || stderr.to_lowercase().contains("error"),
        "stderr should name the bad flag: {stderr}"
    );
}

#[test]
fn test_estimate_all_missing_wasm_errors() {
    let (_, _stderr, code) = run_cli(&["estimate-all"]);
    assert_ne!(code, 0, "estimate-all without --wasm should error");
}

#[test]
fn test_json_flag_accepted() {
    // Verify --json is accepted as a valid argument for estimate
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--json"]);
    // Should fail because file doesn't exist, NOT because --json is unknown
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unrecognized"),
        "--json should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_short_wasm_flag_accepted() {
    // `-w` is the short form of `--wasm` on both estimate and estimate-all.
    let (_, stderr, code) = run_cli(&["estimate", "-w", "does-not-exist.wasm"]);
    assert_ne!(code, 0, "missing file should still error");
    assert!(
        !stderr.contains("unexpected argument"),
        "-w should be recognized; stderr: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `estimate` — runtime error paths (all offline)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_nonexistent_wasm_file() {
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "no/such/file.wasm"]);
    assert_eq!(
        code, 1,
        "a missing WASM file should exit 1; stderr: {stderr}"
    );
    assert!(
        stderr.starts_with("Error:"),
        "runtime failures are reported on stderr as `Error: …`; got: {stderr}"
    );
    assert!(
        stderr.contains("File not found"),
        "a missing file should surface as a file not found error; got: {stderr}"
    );
}

#[test]
fn test_estimate_invalid_wasm_file() {
    let home = temp_home("invalid-wasm");
    let bogus = home.join("not-really.wasm");
    std::fs::write(&bogus, b"this is not a wasm module").expect("write fixture");

    let (_, stderr, code) = run_cli(&["estimate", "--wasm", bogus.to_str().unwrap()]);
    assert_eq!(code, 1, "invalid WASM should exit 1");
    assert!(
        stderr.contains("WASM validation error"),
        "invalid bytes should fail validation; got: {stderr}"
    );
}

#[test]
fn test_estimate_unknown_network() {
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/minimal.wasm",
        "--network",
        "not-a-network",
    ]);
    assert_eq!(code, 1, "an unknown network should exit 1");
    assert!(
        stderr.contains("RPC endpoint not configured for network: not-a-network"),
        "the error should name the unknown network; got: {stderr}"
    );
}

#[test]
fn test_estimate_fn_without_id_errors() {
    // `simulateTransaction` loads the contract instance from the ledger, so
    // invoking a function requires --id. This must fail before any RPC call.
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--rpc-url",
        DEAD_RPC,
        "--fn",
        "increment",
    ]);
    assert_eq!(code, 1, "--fn without --id should exit 1");
    assert!(
        stderr.contains("contract id required"),
        "the error should tell the user to pass --id; got: {stderr}"
    );
}

#[test]
fn test_estimate_invalid_contract_id_errors() {
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--rpc-url",
        DEAD_RPC,
        "--fn",
        "increment",
        "--id",
        "not-a-contract-id",
    ]);
    assert_eq!(code, 1, "a malformed --id should exit 1");
    assert!(
        stderr.contains("invalid contract id"),
        "the error should name the bad id format; got: {stderr}"
    );
}

#[test]
fn test_estimate_unreachable_rpc_errors() {
    // The last offline checkpoint: everything parses, the envelope builds,
    // and the failure comes from the RPC call itself.
    let home = temp_home("dead-rpc");
    let (_, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--rpc-url",
            DEAD_RPC,
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "an unreachable RPC should exit 1");
    assert!(
        stderr.contains("HTTP request failed") || stderr.contains("error sending request"),
        "an unreachable endpoint should surface as an HTTP failure; got: {stderr}"
    );
}

#[test]
fn test_estimate_rpc_url_overrides_unknown_network() {
    // `--rpc-url` bypasses network-name resolution entirely, so an otherwise
    // unknown network name must not be rejected.
    let home = temp_home("rpc-url-override");
    let (_, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--network",
            "not-a-network",
            "--rpc-url",
            DEAD_RPC,
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "the dead endpoint should still fail");
    assert!(
        !stderr.contains("RPC endpoint not configured"),
        "--rpc-url should override network resolution; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `estimate-all`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_all_nonexistent_wasm_file() {
    let (_, stderr, code) = run_cli(&["estimate-all", "--wasm", "no/such/file.wasm"]);
    assert_eq!(code, 1, "a missing WASM file should exit 1");
    assert!(
        stderr.contains("File not found"),
        "a missing file should surface as a file not found error; got: {stderr}"
    );
}

#[test]
fn test_estimate_all_invalid_wasm_file() {
    let home = temp_home("all-invalid-wasm");
    let bogus = home.join("bogus.wasm");
    std::fs::write(&bogus, b"\0asm-but-not-really").expect("write fixture");

    let (_, stderr, code) = run_cli(&["estimate-all", "--wasm", bogus.to_str().unwrap()]);
    assert_eq!(code, 1, "invalid WASM should exit 1");
    assert!(
        stderr.contains("WASM validation error"),
        "invalid bytes should fail validation; got: {stderr}"
    );
}

#[test]
fn test_estimate_all_unknown_network() {
    let (_, stderr, code) = run_cli(&[
        "estimate-all",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--network",
        "not-a-network",
    ]);
    assert_eq!(code, 1, "an unknown network should exit 1");
    assert!(
        stderr.contains("RPC endpoint not configured for network: not-a-network"),
        "the error should name the unknown network; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `config snapshot`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_snapshot_unknown_network() {
    let (_, stderr, code) = run_cli(&["config", "snapshot", "--network", "not-a-network"]);
    assert_eq!(code, 1, "an unknown network should exit 1");
    assert!(
        stderr.contains("RPC endpoint not configured for network: not-a-network"),
        "the error should name the unknown network; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `config diff`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_diff_without_snapshots_errors() {
    // A pristine home has no snapshots to diff against; the tool must say so
    // before it ever touches the network.
    let home = temp_home("diff-no-snapshots");
    let (_, stderr, code) =
        run_cli_in_home(&["config", "diff", "--network", "testnet"], Some(&home));
    assert_eq!(code, 1, "diffing with no snapshots should exit 1");
    assert!(
        stderr.contains("No snapshots available for network: testnet"),
        "the error should name the network with no snapshots; got: {stderr}"
    );
}

#[test]
fn test_config_diff_against_missing_file_errors() {
    let home = temp_home("diff-missing-against");
    let (_, stderr, code) = run_cli_in_home(
        &["config", "diff", "--against", "no/such/snapshot.json"],
        Some(&home),
    );
    assert_eq!(code, 1, "a missing --against file should exit 1");
    assert!(
        stderr.contains("I/O error"),
        "a missing snapshot file should surface as an I/O error; got: {stderr}"
    );
}

#[test]
fn test_config_diff_against_malformed_snapshot_errors() {
    let home = temp_home("diff-malformed-against");
    let path = home.join("malformed.json");
    std::fs::write(&path, b"{ not valid json").expect("write fixture");

    let (_, stderr, code) = run_cli_in_home(
        &["config", "diff", "--against", path.to_str().unwrap()],
        Some(&home),
    );
    assert_eq!(code, 1, "a malformed snapshot should exit 1");
    assert!(
        stderr.contains("Snapshot parse error"),
        "a malformed snapshot should surface as a parse error; got: {stderr}"
    );
}

#[test]
fn test_config_diff_loads_valid_snapshot_before_network() {
    // A well-formed snapshot must get past loading — the next failure has to
    // come from the network, not from the snapshot. This pins the ordering:
    // snapshot load first, RPC second.
    let home = temp_home("diff-valid-snapshot");
    let path = home.join("snapshot.json");
    std::fs::write(&path, snapshot_json("not-a-network", 1000)).expect("write fixture");

    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against",
            path.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "the unknown network should exit 1");
    assert!(
        stderr.contains("RPC endpoint not configured for network: not-a-network"),
        "the snapshot should load cleanly and the network should be the failure; got: {stderr}"
    );
    assert!(
        !stderr.contains("Snapshot parse error"),
        "a valid snapshot must not be reported as malformed; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `watch`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_watch_unknown_network_is_non_fatal() {
    // `watch` is a long-running loop: a failing poll warns and retries rather
    // than exiting. Verify it accepts the args, warns, and keeps running —
    // then kill it, since it would otherwise never return.
    let home = temp_home("watch-loop");
    let mut child = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["watch", "--network", "not-a-network", "--interval", "1h"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn watch");

    // Give the first poll a moment to run, then confirm it has not exited.
    std::thread::sleep(std::time::Duration::from_millis(750));
    let status = child.try_wait().expect("failed to poll watch process");
    assert!(
        status.is_none(),
        "watch should still be running after a failed poll, got: {status:?}"
    );

    let _ = child.kill();
    let output = child.wait_with_output().expect("failed to reap watch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Watching not-a-network for config changes every 3600s"),
        "watch should announce its network and resolved interval; got: {stdout}"
    );
}

#[test]
fn test_watch_interval_suffixes_are_parsed() {
    // `30m` must resolve to 1800s in the banner — the interval parser is unit
    // tested in-crate, this pins the wiring through the CLI.
    let home = temp_home("watch-interval");
    let mut child = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["watch", "--network", "not-a-network", "--interval", "30m"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn watch");

    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = child.kill();
    let output = child.wait_with_output().expect("failed to reap watch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("every 1800s"),
        "`30m` should resolve to 1800s; got: {stdout}"
    );
}
