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

use sha2::Digest;
use soroban_cost_estimator::cache;

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
        // The CLI prefers USERPROFILE on Windows when resolving its data dir.
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
    snapshot_json_at(network, ledger, "2026-01-01T00:00:00+00:00")
}

/// As [`snapshot_json`], but with an explicit RFC 3339 timestamp so a test can
/// lay out several snapshots in a known chronological order.
fn snapshot_json_at(network: &str, ledger: u32, timestamp: &str) -> String {
    format!(
        r#"{{
  "network": "{network}",
  "ledger": {ledger},
  "timestamp": "{timestamp}",
  "contract_compute": null,
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}}"#
    )
}

/// Writes one snapshot into the isolated home's snapshots directory.
///
/// The filename mirrors `save_snapshot`: `{network}-{timestamp}.json` with
/// `:` replaced by `-`. That naming is what orders snapshots on disk, so a
/// test has to reproduce it to exercise the real lookup path.
fn write_snapshot(home: &Path, network: &str, timestamp: &str, ledger: u32) -> PathBuf {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(format!("{network}-{}.json", timestamp.replace(':', "-")));
    std::fs::write(&path, snapshot_json_at(network, ledger, timestamp)).expect("write snapshot");
    path
}

/// An RFC 3339 timestamp `days` in the past, in the same shape
/// `begin_snapshot` records.
fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::TimeDelta::days(days)).to_rfc3339()
}

/// Snapshot filenames on disk for `network` under `home`, oldest first.
fn snapshot_files(home: &Path, network: &str) -> Vec<String> {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with(&format!("{network}-")) && name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

/// Runs the CLI with `HOME` isolated and tracing silenced.
///
/// `tracing`'s `info!` lines go to stdout in this binary, so `RUST_LOG=error`
/// is what makes stdout exactly the command's own output for JSON assertions.
fn run_cli_quiet(args: &[&str], home: Option<&Path>) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd.args(args).env("RUST_LOG", "error");
    if let Some(home) = home {
        cmd.env("HOME", home);
        cmd.env("USERPROFILE", home);
    }

    let output = cmd.output().expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
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
    assert!(stdout.contains("cache"), "help should list cache command");
    assert!(stdout.contains("watch"), "help should list watch command");
    assert!(
        stdout.contains("--rpc-fallback-url"),
        "help should list --rpc-fallback-url flag"
    );
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
        "--interactive",
        "--cache-ttl",
        "--compare",
        "--clear-cache",
        "--no-cache",
        "--json",
        "--repeat",
    ] {
        assert!(
            stdout.contains(flag),
            "estimate help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_estimate_repeat_flag_accepted() {
    // `--repeat` must be a recognized estimate flag (the run fails on the
    // missing WASM file, not on the argument).
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--repeat", "3"]);
    assert_ne!(code, 0, "missing WASM file should still error");
    assert!(
        !stderr.contains("unexpected argument"),
        "--repeat should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_repeat_out_of_range_rejected() {
    // The accepted range is 1..=100; clap must reject 0 and 101 up front.
    for bad in ["0", "101"] {
        let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--repeat", bad]);
        assert_ne!(code, 0, "--repeat {bad} should be rejected");
        assert!(
            stderr.contains("invalid value")
                || stderr.contains("not in")
                || stderr.contains("range"),
            "clap should reject --repeat {bad}; stderr: {stderr}"
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
    for flag in [
        "--wasm",
        "--network",
        "--id",
        "--no-cache",
        "--json",
        "--format",
    ] {
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
    for flag in ["--network", "--out", "--json", "--retain", "prune"] {
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
    for flag in [
        "--network",
        "--against",
        "--against-previous",
        "--summary",
        "--ignore-pricing-exit",
        "--fail-on-any-change",
    ] {
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
    assert!(stdout.contains("query"), "cache help should list query");
    assert!(stdout.contains("clear"), "cache help should list clear");
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
        .env("USERPROFILE", &tmp)
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
fn test_cache_export_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "export", "--help"]);
    assert_eq!(
        code, 0,
        "cache export --help should exit 0; stderr: {stderr}"
    );
    for flag in ["--out", "--network"] {
        assert!(
            stdout.contains(flag),
            "export help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_cache_export_to_file_with_network_filter() {
    let home = temp_home("cache-export-file");
    // Distinct functions: the cache key is (wasm_hash, function, args_hash),
    // so identical keys would upsert instead of producing two rows.
    seed_cache_entry_for(&home, "testnet", "f_testnet", 42, "2026-01-01T00:00:00Z");
    seed_cache_entry_for(&home, "mainnet", "f_mainnet", 43, "2026-01-02T00:00:00Z");
    let out = home.join("backup.json");

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "cache",
            "export",
            "--network",
            "testnet",
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "export should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Exported 1 cache entry to"),
        "confirmation should name the count and path; got: {stdout}"
    );
    assert!(
        stdout.contains(out.to_str().unwrap()),
        "confirmation should name the output file; got: {stdout}"
    );

    let raw = std::fs::read_to_string(&out).expect("read export file");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON export");
    assert_eq!(
        parsed["schema_version"], 1,
        "envelope should stamp the schema version"
    );
    assert!(
        parsed["exported_at"].is_string(),
        "envelope should carry an export timestamp; got: {parsed}"
    );
    assert_eq!(parsed["network"], "testnet");
    let estimates = parsed["estimates"].as_array().expect("estimates array");
    assert_eq!(estimates.len(), 1, "only the testnet entry should export");
    assert_eq!(estimates[0]["network"], "testnet");
}

#[test]
fn test_cache_export_all_networks_to_stdout() {
    let home = temp_home("cache-export-stdout");
    // Distinct functions: the cache key is (wasm_hash, function, args_hash),
    // so identical keys would upsert instead of producing two rows.
    seed_cache_entry_for(&home, "testnet", "f_testnet", 42, "2026-01-01T00:00:00Z");
    seed_cache_entry_for(&home, "mainnet", "f_mainnet", 43, "2026-01-02T00:00:00Z");

    // tracing's `info!` lines go to stdout in this binary, so silence them
    // with RUST_LOG=error (via run_cli_quiet) to get pure JSON on stdout.
    let (stdout, stderr, code) = run_cli_quiet(&["cache", "export"], Some(&home));
    assert_eq!(code, 0, "export should exit 0; stderr: {stderr}");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be the JSON envelope");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed.get("network").is_none() || parsed["network"].is_null());
    assert_eq!(
        parsed["estimates"]
            .as_array()
            .expect("estimates array")
            .len(),
        2,
        "unfiltered export should carry both networks"
    );
}

#[test]
fn test_cache_export_unwritable_destination_errors() {
    let home = temp_home("cache-export-unwritable");
    seed_cache_entry_for(
        &home,
        "testnet",
        "(wasm upload)",
        42,
        "2026-01-01T00:00:00Z",
    );
    // A directory is never a writable file destination.
    let dir = home.join("a-directory");
    std::fs::create_dir_all(&dir).expect("create dir");

    let (_, stderr, code) = run_cli_in_home(
        &["cache", "export", "--out", dir.to_str().unwrap()],
        Some(&home),
    );
    assert_eq!(code, 1, "an unwritable destination should exit 1");
    assert!(
        stderr.contains(dir.to_str().unwrap()),
        "the error should name the destination; got: {stderr}"
    );
}

#[test]
fn test_cache_stats_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "stats", "--help"]);
    assert_eq!(
        code, 0,
        "cache stats --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("cache health") || stdout.contains("breakdown"),
        "cache stats help should describe the command; got: {stdout}"
    );
}

#[test]
fn test_cache_stats_on_empty_cache_succeeds() {
    let home = temp_home("cache-stats-empty");
    let (stdout, stderr, code) = run_cli_in_home(&["cache", "stats"], Some(&home));
    assert_eq!(
        code, 0,
        "cache stats on an empty cache should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Cache is empty"),
        "should report an empty cache; got: {stdout}"
    );
}

#[test]
fn test_cache_stats_reports_seeded_entries() {
    let home = temp_home("cache-stats-seeded");
    let now = chrono::Utc::now().to_rfc3339();
    seed_cache_entry_for(&home, "testnet", "(wasm upload)", 42, &now);
    seed_cache_entry_for(&home, "mainnet", "mainnet_fn", 77, &now);

    let (stdout, stderr, code) = run_cli_in_home(&["cache", "stats"], Some(&home));
    assert_eq!(code, 0, "cache stats should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Total entries:  2"),
        "should count both seeded entries; got: {stdout}"
    );
    assert!(
        stdout.contains("testnet") && stdout.contains("mainnet"),
        "should break entries down by network; got: {stdout}"
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
fn test_rpc_fallback_url_flag_accepted() {
    // Verify --rpc-fallback-url is accepted as a global argument.
    let (_, stderr, code) = run_cli(&[
        "--rpc-fallback-url",
        "http://127.0.0.1:9999",
        "estimate",
        "--wasm",
        "test.wasm",
    ]);
    // Should fail because file doesn't exist, NOT because --rpc-fallback-url is unknown.
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unrecognized"),
        "--rpc-fallback-url should be recognized; stderr: {stderr}"
    );
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
fn test_format_flag_accepted() {
    // Verify --format is a recognized argument for estimate, including the
    // new markdown variant (#80).
    for fmt in ["table", "json", "csv", "markdown"] {
        let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--format", fmt]);
        // Should fail because the file doesn't exist, NOT because --format is
        // unknown or the value is invalid.
        assert_ne!(code, 0, "should error on missing file for {fmt}");
        assert!(
            !stderr.contains("unrecognized") && !stderr.contains("invalid value"),
            "--format {fmt} should be a recognized argument; stderr: {stderr}"
        );
    }
}

#[test]
fn test_format_invalid_value_rejected() {
    // clap's value_parser must reject unknown formats before the command runs.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--format", "xml"]);
    assert_ne!(code, 0, "invalid --format value should error");
    assert!(
        stderr.contains("invalid value") || stderr.contains("possible values"),
        "clap should reject unknown format; stderr: {stderr}"
    );
}

#[test]
fn test_format_markdown_with_json_flag_accepted() {
    // --format wins over the legacy --json flag; the combination must be
    // accepted as valid arguments (failure here is a missing file, not an
    // argument conflict).
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "test.wasm",
        "--json",
        "--format",
        "markdown",
    ]);
    assert_ne!(code, 0, "should error on missing file");
    assert!(
        !stderr.contains("unrecognized") && !stderr.contains("cannot be used"),
        "--json + --format markdown should be accepted; stderr: {stderr}"
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

#[test]
fn test_estimate_cache_ttl_flag_accepted() {
    // Verify --cache-ttl is a recognized argument for estimate.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--cache-ttl", "1h"]);
    // Should fail because the file doesn't exist, NOT because --cache-ttl is unknown.
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--cache-ttl should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_timeout_flag_accepted() {
    // Verify --timeout is a recognized global argument for estimate.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--timeout", "10"]);
    // Should fail because the file doesn't exist, NOT because --timeout is unknown.
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--timeout should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_timeout_flag_accepted_before_subcommand() {
    // Global flags must also be accepted before the subcommand.
    let (_, stderr, code) = run_cli(&["--timeout", "10", "estimate", "--wasm", "test.wasm"]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--timeout before the subcommand should be recognized; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_flag_accepted() {
    // Verify --max-retries is a recognized global argument for estimate.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--max-retries", "5"]);
    // Should fail because the file doesn't exist, NOT because --max-retries is unknown.
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--max-retries should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_flag_accepted_before_subcommand() {
    // Global flags must also be accepted before the subcommand.
    let (_, stderr, code) = run_cli(&["--max-retries", "5", "estimate", "--wasm", "test.wasm"]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--max-retries before the subcommand should be recognized; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_zero_accepted() {
    // 0 is the documented "disable retries" value and must be accepted, not
    // rejected as an out-of-range value.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--max-retries", "0"]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("invalid value"),
        "--max-retries 0 should be accepted; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_negative_value_rejected() {
    // The flag is unsigned, so a negative value must be rejected during
    // argument parsing rather than silently wrapping to a huge retry count.
    // clap rejects a bare `-1` as an unexpected argument, which is a
    // non-zero exit before the command runs — the same convention the
    // existing unknown-flag test asserts.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--max-retries", "-1"]);
    assert_ne!(code, 0, "a negative --max-retries must be rejected");
    assert!(
        stderr.contains("unexpected argument") || stderr.to_lowercase().contains("error"),
        "--max-retries -1 should be rejected at parse time; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_accepted_by_all_network_commands() {
    // The flag is global, so every command that builds an RPC client must
    // accept it — not just `estimate`. Each case below fails fast *before*
    // any network call: `estimate-all`/`cache warm` on a missing WASM file,
    // and the `config` subcommands on an unknown network name. That keeps the
    // suite offline while still exercising argument parsing for each command.
    for args in [
        vec!["estimate-all", "--wasm", "test.wasm"],
        vec!["cache", "warm", "--wasm", "test.wasm"],
        vec!["config", "snapshot", "--network", "nosuchnet"],
        vec!["config", "diff", "--network", "nosuchnet"],
    ] {
        let mut full = args.clone();
        full.extend_from_slice(&["--max-retries", "5"]);

        let (_, stderr, code) = run_cli(&full);
        assert_ne!(
            code,
            0,
            "`{}` should fail on its own validation, not on flag parsing",
            args.join(" ")
        );
        assert!(
            !stderr.contains("unexpected argument"),
            "--max-retries should be accepted by `{}`; stderr: {stderr}",
            args.join(" ")
        );
    }
}

#[test]
fn test_help_lists_global_flags() {
    // Global flags (--rps, --timeout, --precision, --quiet) must appear in
    // subcommand help.
    let (stdout, stderr, code) = run_cli(&["estimate", "--help"]);
    assert_eq!(code, 0, "estimate --help should exit 0; stderr: {stderr}");
    for flag in ["--timeout", "--rps", "--precision", "--quiet"] {
        assert!(
            stdout.contains(flag),
            "help should list {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_precision_flag_accepted() {
    // `--precision` is a global flag, so it must parse both before and after
    // the subcommand; failure here is a missing file, not a bad argument.
    for args in [
        vec!["estimate", "--wasm", "test.wasm", "--precision", "2"],
        vec!["--precision", "4", "estimate", "--wasm", "test.wasm"],
    ] {
        let (_, stderr, code) = run_cli(&args);
        assert_ne!(code, 0, "should error on missing file");
        assert!(
            !stderr.contains("unrecognized") && !stderr.contains("invalid value"),
            "--precision should be a recognized argument; stderr: {stderr}"
        );
    }
}

#[test]
fn test_precision_out_of_range_rejected() {
    // The flag is documented as 0..=7; clap must reject 8 with a clear error.
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--precision", "8"]);
    assert_ne!(code, 0, "out-of-range precision should error");
    assert!(
        stderr.to_lowercase().contains("invalid value")
            || stderr.contains("not in")
            || stderr.to_lowercase().contains("range"),
        "clap should reject precision 8; stderr: {stderr}"
    );
}

#[test]
fn test_max_retries_help_documents_default_and_zero_behavior() {
    // Acceptance criteria: the flag must be in --help with a clear description
    // of both the default and the "0 disables retries" behavior.
    let (stdout, stderr, code) = run_cli(&["--help"]);
    assert_eq!(code, 0, "--help should exit 0; stderr: {stderr}");

    // clap hard-wraps the doc comment to the terminal width, so a phrase can be
    // split across lines. Collapse all whitespace before searching for the
    // phrases the acceptance criteria require.
    let help = stdout
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        help.contains("--max-retries"),
        "help should list --max-retries; got: {stdout}"
    );
    assert!(
        help.contains("default 3"),
        "help should state the default of 3; got: {stdout}"
    );
    assert!(
        help.contains("0 disables retries"),
        "help should explain that 0 disables retries; got: {stdout}"
    );
}

#[test]
fn test_quiet_flag_accepted() {
    // `--quiet` / `-q` is a global flag used to suppress the fee bar chart.
    for args in [
        vec!["estimate", "--wasm", "test.wasm", "--quiet"],
        vec!["estimate", "--wasm", "test.wasm", "-q"],
    ] {
        let (_, stderr, code) = run_cli(&args);
        assert_ne!(code, 0, "should error on missing file");
        assert!(
            !stderr.contains("unrecognized") && !stderr.contains("unexpected argument"),
            "--quiet should be a recognized argument; stderr: {stderr}"
        );
    }
}

#[test]
fn test_estimate_all_format_flag_accepted() {
    // Verify --format is a recognized argument for estimate-all.
    let (_, stderr, code) = run_cli(&["estimate-all", "--wasm", "test.wasm", "--format", "csv"]);
    // Should fail because the file doesn't exist, NOT because --format is unknown.
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument"),
        "--format should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_all_format_wins_over_json() {
    // --format should take precedence over the legacy --json flag.
    // Both flags are accepted; the combination fails only because
    // test.wasm doesn't exist, NOT because of an argument conflict.
    let (_, stderr, code) = run_cli(&[
        "estimate-all",
        "--wasm",
        "test.wasm",
        "--format",
        "csv",
        "--json",
    ]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("cannot") && !stderr.contains("conflicts"),
        "--format and --json should NOT conflict; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_all_format_invalid_value_rejected() {
    // clap's value_parser must reject unknown formats before the command runs.
    let (_, stderr, code) = run_cli(&["estimate-all", "--wasm", "test.wasm", "--format", "xml"]);
    assert_ne!(code, 0, "invalid --format value should error");
    assert!(
        stderr.contains("invalid value") || stderr.contains("possible values"),
        "clap should reject unknown format; stderr: {stderr}"
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
        stderr.contains("failed to validate WASM"),
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
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
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
fn test_estimate_interactive_eof_cancels_cleanly() {
    // `run_cli` leaves stdin closed, so the first prompt reads EOF. The
    // command must abort with a clear error — no hang, no panic.
    let (stdout, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--interactive",
    ]);
    assert_eq!(code, 1, "EOF on stdin should exit 1");
    assert!(
        stdout.contains("Available functions:"),
        "the function list should print before the read; got: {stdout}"
    );
    assert!(
        stdout.contains("increment"),
        "the fixture's increment function should be listed; got: {stdout}"
    );
    assert!(
        stderr.contains("cancelled"),
        "the error should say the input was cancelled; got: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "cancelling the prompt must not panic; got: {stderr}"
    );
}

#[test]
fn test_estimate_interactive_short_flag_accepted() {
    // `-i` is the short form of `--interactive`; like above, stdin is
    // closed, so it must reach the prompt (then cancel) rather than fail
    // on argument parsing.
    let (stdout, stderr, code) =
        run_cli(&["estimate", "--wasm", "tests/fixtures/contract.wasm", "-i"]);
    assert_eq!(code, 1, "EOF on stdin should exit 1");
    assert!(
        stdout.contains("Available functions:"),
        "the -i flag should enable the prompt; got: {stdout}; stderr: {stderr}"
    );
    assert!(
        stderr.contains("cancelled"),
        "the error should say the input was cancelled; got: {stderr}"
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

/// Seed a cache entry for `tests/fixtures/minimal.wasm` (default `(wasm
/// upload)` function, no args) with the given timestamp, in `home`.
///
/// Mirrors the library's cache-key computation: `wasm_hash` is the SHA-256 of
/// the WASM bytes and `args_hash` is the SHA-256 of the concatenated args
/// (empty for no args). The entry is written directly into the SQLite cache
/// database so the `estimate` command's cache-hit path can find it.
fn seed_cache_entry(home: &Path, timestamp: &str) {
    seed_cache_entry_for(home, "testnet", "(wasm upload)", 42, timestamp);
}

/// Seed a cache entry on the given network/function, in `home`.
///
/// A thin generalization of [`seed_cache_entry`] so tests can populate more
/// than one network (or several functions) and exercise per-network
/// cache-clear isolation. The row targets `tests/fixtures/minimal.wasm` with
/// no args, exactly like [`seed_cache_entry`].
fn seed_cache_entry_for(home: &Path, network: &str, function: &str, ledger: i64, timestamp: &str) {
    let wasm_bytes = std::fs::read("tests/fixtures/minimal.wasm").expect("read fixture");
    let wasm_hash = hex::encode(sha2::Sha256::digest(&wasm_bytes));
    let args_hash = hex::encode(sha2::Sha256::digest(b""));

    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir).expect("create data dir");
    let db = dir.join("cache.db");
    let conn = rusqlite::Connection::open(&db).expect("open cache db");
    // Ensure the schema exists in this exact database before writing rows
    // directly (the CLI reads the same path, so they must match).
    cache::ensure_cache_schema(&conn).expect("ensure cache schema");

    conn.execute(
        "INSERT OR REPLACE INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            1i64,
            wasm_hash,
            function,
            args_hash,
            network,
            ledger,
            1_000i64,
            500i64,
            250i64,
            timestamp,
        ],
    )
    .expect("seed cache entry");
}

#[test]
fn test_estimate_cache_hit_skips_simulation() {
    // A fresh cached estimate plus --cache-ttl must short-circuit before any
    // RPC call: even a dead endpoint succeeds, because it is never contacted.
    let home = temp_home("cache-ttl-hit");
    seed_cache_entry(&home, &chrono::Utc::now().to_rfc3339());

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--cache-ttl",
            "1h",
            "--rpc-url",
            DEAD_RPC,
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "a fresh cache hit should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Cache hit"),
        "stdout should announce the cache hit; got: {stdout}"
    );
    assert!(
        stdout.contains("1,000 stroops") || stdout.contains("1000 stroops"),
        "stdout should include the cached fee; got: {stdout}"
    );
}

#[test]
fn test_estimate_cache_hit_json_output() {
    let home = temp_home("cache-ttl-json");
    seed_cache_entry(&home, &chrono::Utc::now().to_rfc3339());

    // tracing's `info!` lines go to stdout in this binary, so silence them
    // with RUST_LOG=error to get pure JSON on stdout.
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--cache-ttl",
            "1h",
            "--json",
            "--rpc-url",
            DEAD_RPC,
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a fresh cache hit should exit 0; stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    assert_eq!(parsed["cache"], "hit");
    assert_eq!(parsed["total_stroops"], 1_000);
    assert_eq!(parsed["ledger"], 42);
}

#[test]
fn test_estimate_cache_expired_resimulates() {
    // An expired entry must NOT short-circuit: the command proceeds to
    // simulate, so the dead endpoint is contacted and the run fails.
    let home = temp_home("cache-ttl-expired");
    seed_cache_entry(
        &home,
        &(chrono::Utc::now() - chrono::TimeDelta::hours(2)).to_rfc3339(),
    );

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--cache-ttl",
            "1h",
            "--rpc-url",
            DEAD_RPC,
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "an expired entry must fall through to simulation");
    assert!(
        !stdout.contains("Cache hit"),
        "an expired entry must not be reported as a hit; got: {stdout}"
    );
    assert!(
        stderr.contains("HTTP request failed") || stderr.contains("error sending request"),
        "re-simulation should hit the dead endpoint; got: {stderr}"
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
        stderr.contains("failed to validate WASM"),
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
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
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
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
        "the error should name the unknown network; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_retain_flag_accepted() {
    // `--retain` must be a recognized argument: the run fails on the unknown
    // network (before any RPC), not on the flag itself.
    let (_, stderr, code) = run_cli(&[
        "config",
        "snapshot",
        "--network",
        "not-a-network",
        "--retain",
        "5",
    ]);
    assert_eq!(code, 1, "an unknown network should exit 1");
    assert!(
        !stderr.contains("unexpected argument"),
        "--retain should be a recognized argument; stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
        "the failure should come from the network, not the flag; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `config snapshot` retention
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_snapshot_retain_flag_is_accepted() {
    // `--retain` must parse; the failure should come from the unresolvable
    // network, not from clap rejecting the flag.
    let home = temp_home("snapshot-retain-flag");
    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "--retain",
            "3",
            "--network",
            "not-a-network",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "the unknown network should exit 1");
    assert!(
        stderr.contains("failed to locate RPC endpoint"),
        "the failure should come from the network, not the flag; got: {stderr}"
    );
    assert!(
        !stderr.contains("unexpected argument"),
        "--retain must not be rejected by the parser; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_prune_help() {
    let (stdout, stderr, code) = run_cli(&["config", "snapshot", "prune", "--help"]);
    assert_eq!(code, 0, "prune --help should exit 0; stderr: {stderr}");
    for flag in ["--network", "--older-than", "--json"] {
        assert!(
            stdout.contains(flag),
            "prune help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_config_snapshot_prune_requires_older_than() {
    let (_, stderr, code) = run_cli(&["config", "snapshot", "prune"]);
    assert_ne!(code, 0, "prune without --older-than must be rejected");
    assert!(
        stderr.contains("--older-than"),
        "the error should name the missing flag; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_prune_rejects_fetch_flags() {
    // Pruning never fetches a snapshot, so combining it with the fetching
    // flags would silently ignore them. It must be an explicit error instead.
    let (_, stderr, code) = run_cli(&[
        "config",
        "snapshot",
        "--retain",
        "3",
        "prune",
        "--older-than",
        "1",
    ]);
    assert_ne!(code, 0, "--retain with prune should be rejected");
    assert!(
        stderr.contains("--retain") && stderr.contains("prune"),
        "the error should name both the flag and the subcommand; got: {stderr}"
    );

    let (_, stderr, code) = run_cli(&[
        "config",
        "snapshot",
        "--out",
        "/tmp/x.json",
        "prune",
        "--older-than",
        "1",
    ]);
    assert_ne!(code, 0, "--out with prune should be rejected");
    assert!(
        stderr.contains("--out") && stderr.contains("prune"),
        "the error should name both the flag and the subcommand; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_prune_deletes_only_stale_snapshots() {
    let home = temp_home("prune-stale");
    write_snapshot(&home, "testnet", &days_ago(40), 1);
    write_snapshot(&home, "testnet", &days_ago(10), 2);
    write_snapshot(&home, "testnet", &days_ago(1), 3);

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "prune",
            "--network",
            "testnet",
            "--older-than",
            "30",
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "pruning should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Pruned 1 snapshot(s)"),
        "the count of pruned snapshots must be logged; got: {stdout}"
    );
    assert_eq!(
        snapshot_files(&home, "testnet").len(),
        2,
        "only the 40-day-old snapshot should be gone; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_prune_keeps_latest_however_old_it_is() {
    // Every snapshot is older than the threshold, but the newest one is the
    // only thing left to diff against, so it has to survive.
    let home = temp_home("prune-latest");
    write_snapshot(&home, "testnet", &days_ago(300), 1);
    write_snapshot(&home, "testnet", &days_ago(200), 2);
    write_snapshot(&home, "testnet", &days_ago(100), 3);
    let newest = snapshot_files(&home, "testnet")
        .pop()
        .expect("three snapshots were written");

    let (stdout, _, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "prune",
            "--network",
            "testnet",
            "--older-than",
            "0",
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "pruning should exit 0");
    assert!(
        stdout.contains("Pruned 2 snapshot(s)"),
        "only the two older snapshots should go; got: {stdout}"
    );
    assert_eq!(
        snapshot_files(&home, "testnet"),
        vec![newest],
        "the newest snapshot must survive any age threshold"
    );
}

#[test]
fn test_config_snapshot_prune_without_snapshots_is_a_noop() {
    let home = temp_home("prune-empty");
    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "prune",
            "--network",
            "testnet",
            "--older-than",
            "1",
        ],
        Some(&home),
    );
    assert_eq!(
        code, 0,
        "an empty snapshots dir is not an error; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Pruned 0 snapshot(s)"),
        "a no-op run should still report its count; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_prune_json_reports_the_run() {
    let home = temp_home("prune-json");
    write_snapshot(&home, "testnet", &days_ago(90), 1);
    write_snapshot(&home, "testnet", &days_ago(30), 2);

    let (stdout, stderr, code) = run_cli_quiet(
        &[
            "config",
            "snapshot",
            "prune",
            "--network",
            "testnet",
            "--older-than",
            "60",
            "--json",
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "prune --json should exit 0; stderr: {stderr}");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("valid JSON; {e}: {stdout}"));
    assert_eq!(parsed["network"], "testnet");
    assert_eq!(parsed["retain_days"], 60);
    assert_eq!(parsed["pruned_count"], 1);
    assert_eq!(parsed["remaining"], 1);
    assert_eq!(parsed["pruned"].as_array().map(Vec::len), Some(1));
}

#[test]
fn test_config_snapshot_prune_leaves_other_networks_alone() {
    let home = temp_home("prune-network");
    write_snapshot(&home, "testnet", &days_ago(90), 1);
    write_snapshot(&home, "testnet", &days_ago(1), 2);
    write_snapshot(&home, "mainnet", &days_ago(90), 9);

    let (_, _, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "prune",
            "--network",
            "testnet",
            "--older-than",
            "30",
        ],
        Some(&home),
    );

    assert_eq!(code, 0);
    assert_eq!(
        snapshot_files(&home, "mainnet").len(),
        1,
        "another network's snapshots must be untouched"
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
        stderr.contains("none available for network testnet"),
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
        stderr.contains("Error: failed to perform I/O"),
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
        stderr.contains("Error: failed to parse snapshot"),
        "a malformed snapshot should surface as a parse error; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `cache warm`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_cache_warm_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "warm", "--help"]);
    assert_eq!(code, 0, "cache warm --help should exit 0; stderr: {stderr}");
    for flag in ["--wasm", "--network", "--id", "--json"] {
        assert!(
            stdout.contains(flag),
            "cache warm help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_cache_warm_missing_wasm_errors() {
    let (_, stderr, code) = run_cli(&["cache", "warm"]);
    assert_ne!(code, 0, "cache warm without --wasm should error");
    assert!(
        stderr.contains("error") || stderr.contains("required"),
        "stderr should indicate error: {stderr}"
    );
}

#[test]
fn test_cache_warm_nonexistent_wasm_file() {
    let (_, stderr, code) = run_cli(&["cache", "warm", "--wasm", "no/such/file.wasm"]);
    assert_eq!(
        code, 1,
        "a missing WASM file should exit 1; stderr: {stderr}"
    );
    assert!(
        stderr.contains("File not found") || stderr.contains("Error: failed to perform I/O"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_cache_warm_invalid_wasm_file() {
    let home = temp_home("warm-invalid-wasm");
    let bogus = home.join("bogus.wasm");
    std::fs::write(&bogus, b"not a real wasm").expect("write fixture");

    let (_, stderr, code) = run_cli(&["cache", "warm", "--wasm", bogus.to_str().unwrap()]);
    assert_eq!(code, 1, "invalid WASM should exit 1");
    assert!(
        stderr.contains("failed to validate WASM"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_cache_warm_unknown_network() {
    let (_, stderr, code) = run_cli(&[
        "cache",
        "warm",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--network",
        "not-a-network",
    ]);
    assert_eq!(code, 1, "an unknown network should exit 1");
    assert!(
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
        "stderr: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `config diff`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_diff_summary_flag_accepted() {
    // `--summary` on config diff must be a recognized flag (the run still
    // fails, but on the unknown network, not on the argument itself).
    let home = temp_home("diff-summary-flag");
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
            "--summary",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "the unknown network should exit 1");
    assert!(
        !stderr.contains("unexpected argument"),
        "--summary should be a recognized argument; stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
        "the failure should come from the network, not the flag; got: {stderr}"
    );
}

#[test]
fn test_config_diff_exit_code_flags_accepted() {
    // `--ignore-pricing-exit` and `--fail-on-any-change` must be recognized
    // flags (the run still fails, but on the unknown network, not on the
    // arguments themselves).
    for (label, flag) in [
        ("ignore", "--ignore-pricing-exit"),
        ("fail", "--fail-on-any-change"),
    ] {
        let home = temp_home(&format!("diff-exit-flag-{label}"));
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
                flag,
            ],
            Some(&home),
        );
        assert_eq!(code, 1, "the unknown network should exit 1");
        assert!(
            !stderr.contains("unexpected argument"),
            "{flag} should be a recognized argument; stderr: {stderr}"
        );
        assert!(
            stderr.contains(
                "Error: failed to locate RPC endpoint: not configured for network not-a-network"
            ),
            "the failure should come from the network, not the flag; got: {stderr}"
        );
    }
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
        stderr.contains(
            "Error: failed to locate RPC endpoint: not configured for network not-a-network"
        ),
        "the snapshot should load cleanly and the network should be the failure; got: {stderr}"
    );
    assert!(
        !stderr.contains("Error: failed to parse snapshot"),
        "a valid snapshot must not be reported as malformed; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `config diff --against-previous`
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_diff_against_previous_no_snapshots_errors() {
    // Scenario 0: nothing on disk to compare. The network name is
    // deliberately unresolvable, so if the command reached for the RPC
    // endpoint at all the failure would name the network instead.
    let home = temp_home("diff-prev-none");
    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against-previous",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "0 snapshots should exit 1");
    assert!(
        stderr.contains("need at least 2 for network not-a-network") && stderr.contains("found 0"),
        "the error should report the snapshot count; got: {stderr}"
    );
    assert!(
        !stderr.contains("failed to locate RPC endpoint"),
        "--against-previous must never contact the network; got: {stderr}"
    );
}

#[test]
fn test_config_diff_against_previous_one_snapshot_errors() {
    // Scenario 1: a single point in time has nothing to diff against.
    let home = temp_home("diff-prev-one");
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:01+00:00", 100);

    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against-previous",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "1 snapshot should exit 1");
    assert!(
        stderr.contains("need at least 2 for network not-a-network") && stderr.contains("found 1"),
        "the error should report the snapshot count; got: {stderr}"
    );
    assert!(
        !stderr.contains("failed to locate RPC endpoint"),
        "--against-previous must never contact the network; got: {stderr}"
    );
}

#[test]
fn test_config_diff_against_previous_diffs_two_newest_snapshots() {
    // Scenario 2+: three snapshots on disk, so the command must compare the
    // two newest (ledgers 200 → 300) and leave the oldest (100) alone. The
    // network is unresolvable on purpose: this command is purely local, so a
    // network failure here would mean it leaked past the snapshot store.
    let home = temp_home("diff-prev-two");
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:01+00:00", 100);
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:02+00:00", 200);
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:03+00:00", 300);

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against-previous",
        ],
        Some(&home),
    );

    assert_eq!(
        code, 0,
        "identical consecutive snapshots should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("(ledger 200) → 2026-01-01T00:00:03+00:00 (ledger 300)"),
        "the two newest snapshots must be the ones compared; got: {stdout}"
    );
    assert!(
        !stdout.contains("(ledger 100)"),
        "the oldest snapshot must not take part in the diff; got: {stdout}"
    );
    assert!(
        stdout.contains("No changes detected"),
        "identical snapshots should report no changes; got: {stdout}"
    );
}

#[test]
fn test_config_diff_against_previous_summary_output() {
    let home = temp_home("diff-prev-summary");
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:01+00:00", 100);
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:02+00:00", 200);

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against-previous",
            "--summary",
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "--summary should exit 0 here; stderr: {stderr}");
    assert!(
        stdout.contains("Network config up to date (ledger 200)"),
        "--summary should emit exactly the one-line summary; got: {stdout}"
    );
}

#[test]
fn test_config_diff_against_previous_json_uses_the_live_envelope() {
    // Both diff modes emit `{ diff, stale_estimates }`, so a consumer sees one
    // schema whether the newer side came from the network or from disk.
    let home = temp_home("diff-prev-json");
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:01+00:00", 100);
    write_snapshot(&home, "not-a-network", "2026-01-01T00:00:02+00:00", 200);

    let (stdout, stderr, code) = run_cli_quiet(
        &[
            "config",
            "diff",
            "--network",
            "not-a-network",
            "--against-previous",
            "--json",
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "--json should exit 0 here; stderr: {stderr}");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("valid JSON; {e}: {stdout}"));
    assert_eq!(parsed["diff"]["old_snapshot"]["ledger"], 100);
    assert_eq!(parsed["diff"]["new_snapshot"]["ledger"], 200);
    assert_eq!(parsed["diff"]["has_pricing_changes"], false);
    assert_eq!(parsed["stale_estimates"].as_array().map(Vec::len), Some(0));
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

// ─────────────────────────────────────────────────────────────────────────
// `wasm-info` — contractmeta display
// ─────────────────────────────────────────────────────────────────────────

/// Encodes one `ScMetaEntry::ScMetaV0` union value (XDR): 4-byte
/// discriminant 0, then `{ key, val }` as length-prefixed strings, each
/// padded to a 4-byte boundary (XDR string padding).
fn xdr_meta_entry(key: &str, val: &str) -> Vec<u8> {
    let mut out = 0u32.to_be_bytes().to_vec();
    for s in [key, val] {
        out.extend_from_slice(&(s.len() as u32).to_be_bytes());
        out.extend_from_slice(s.as_bytes());
        let padding = (4 - s.len() % 4) % 4;
        out.extend_from_slice(&[0u8; 4][..padding]);
    }
    out
}

/// Wraps `payload` in a WASM custom section (id 0) named `name`.
fn custom_section(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut content = Vec::new();
    content.push(name.len() as u8);
    content.extend_from_slice(name.as_bytes());
    content.extend_from_slice(payload);

    let mut section = vec![0u8];
    let mut size = content.len() as u32;
    loop {
        let mut byte = (size & 0x7f) as u8;
        size >>= 7;
        if size != 0 {
            byte |= 0x80;
        }
        section.push(byte);
        if size == 0 {
            break;
        }
    }
    section.extend_from_slice(&content);
    section
}

#[test]
fn test_wasm_info_displays_contract_meta() {
    // Extend the bare fixture with a contractmeta section and point wasm-info
    // at it: name/version/description/author/SDK version must be shown
    // (table and JSON modes).
    let mut bytes = std::fs::read("tests/fixtures/minimal.wasm").expect("read fixture");
    let mut payload = Vec::new();
    payload.extend_from_slice(&xdr_meta_entry("name", "MetaContract"));
    payload.extend_from_slice(&xdr_meta_entry("version", "9.9.9"));
    payload.extend_from_slice(&xdr_meta_entry("description", "A meta description"));
    payload.extend_from_slice(&xdr_meta_entry("author", "Stellar Dev"));
    payload.extend_from_slice(&xdr_meta_entry("rs_sdk_version", "25.3.2"));
    bytes.extend_from_slice(&custom_section("contractmetav0", &payload));

    let home = temp_home("wasm-info-meta");
    let path = home.join("meta.wasm");
    std::fs::write(&path, &bytes).expect("write fixture");

    let (stdout, stderr, code) = run_cli_in_home(
        &["wasm-info", "--wasm", path.to_str().unwrap()],
        Some(&home),
    );
    assert_eq!(code, 0, "wasm-info should succeed; stderr: {stderr}");
    assert!(stdout.contains("Contract meta: present"));
    assert!(stdout.contains("name: MetaContract"));
    assert!(stdout.contains("version: 9.9.9"));
    assert!(stdout.contains("description: A meta description"));
    assert!(stdout.contains("author: Stellar Dev"));
    assert!(stdout.contains("sdk_version: 25.3.2"));

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["wasm-info", "--wasm", path.to_str().unwrap(), "--json"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    assert_eq!(parsed["contract_meta"]["name"], "MetaContract");
    assert_eq!(parsed["contract_meta"]["version"], "9.9.9");
    assert_eq!(parsed["contract_meta"]["description"], "A meta description");
    assert_eq!(parsed["contract_meta"]["author"], "Stellar Dev");
    assert_eq!(parsed["contract_meta"]["sdk_version"], "25.3.2");
}

#[test]
fn test_wasm_info_reports_absent_contract_meta() {
    let home = temp_home("wasm-info-no-meta");
    let (stdout, stderr, code) = run_cli_in_home(
        &["wasm-info", "--wasm", "tests/fixtures/minimal.wasm"],
        Some(&home),
    );
    assert_eq!(code, 0, "wasm-info should succeed; stderr: {stderr}");
    assert!(
        stdout.contains("Contract meta: absent"),
        "bare WASM should report absent meta; got: {stdout}"
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

// ── cache query tests ────────────────────────────────────────────────

#[test]
fn test_cache_query_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "query", "--help"]);
    assert_eq!(
        code, 0,
        "cache query --help should exit 0; stderr: {stderr}"
    );
    for flag in [
        "--network",
        "--function",
        "--wasm-hash",
        "--min-stroops",
        "--max-stroops",
        "--from",
        "--to",
        "--json",
    ] {
        assert!(
            stdout.contains(flag),
            "query help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_cache_query_empty_cache() {
    let home = temp_home("cache-query-empty");
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["cache", "query", "--network", "testnet"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .expect("failed to run cache query");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No cached estimates match the query."),
        "empty cache should report no results; got: {stdout}"
    );
}

#[test]
fn test_cache_query_empty_json() {
    let home = temp_home("cache-query-empty-json");
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["cache", "query", "--network", "testnet", "--json"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run cache query");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    assert_eq!(trimmed, "[]", "empty JSON should be []; got: {stdout}");
}

#[test]
fn test_cache_query_json_flag_accepted() {
    let home = temp_home("cache-query-json-flag");
    // Save a cached estimate first via the test helper
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["cache", "query", "--network", "testnet", "--json"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run cache query");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    // Should be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(trimmed).is_ok(),
        "output should be valid JSON; got: {stdout}"
    );
}

fn seed_cache_estimate(
    home: &Path,
    wasm_hash: &str,
    function: &str,
    network: &str,
    fee: i64,
    cpu: u64,
    timestamp: &str,
) {
    let args_hash = hex::encode(sha2::Sha256::digest(function.as_bytes()));
    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir).expect("create data dir");
    let db = dir.join("cache.db");
    let conn = rusqlite::Connection::open(&db).expect("open cache db");
    cache::ensure_cache_schema(&conn).expect("ensure cache schema");

    conn.execute(
        "INSERT OR REPLACE INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            cache::CACHE_SCHEMA_VERSION as i64,
            wasm_hash,
            function,
            args_hash,
            network,
            42i64,
            fee,
            cpu as i64,
            1024i64,
            timestamp,
        ],
    )
    .expect("insert test cache estimate");
}

#[test]
fn test_config_cache_query_help() {
    let (stdout, stderr, code) = run_cli(&["config", "cache", "query", "--help"]);
    assert_eq!(
        code, 0,
        "config cache query --help should exit 0; stderr: {stderr}"
    );
    for flag in [
        "--network",
        "--fn",
        "--wasm-hash",
        "--min-fee",
        "--max-fee",
        "--since",
        "--json",
    ] {
        assert!(
            stdout.contains(flag),
            "config cache query help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_config_cache_query_empty_cache() {
    let home = temp_home("config-cache-query-empty");
    let (stdout, stderr, code) = run_cli_in_home(&["config", "cache", "query"], Some(&home));
    assert_eq!(
        code, 0,
        "config cache query on empty cache should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("No cached estimates match the query."),
        "empty cache should report no results; got: {stdout}"
    );
}

fn run_cli_json_in_home(args: &[&str], home: &Path) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
    cmd.args(args);
    cmd.env("HOME", home);
    cmd.env("USERPROFILE", home);
    cmd.env("RUST_LOG", "error");
    let output = cmd.output().expect("failed to run CLI");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

#[test]
fn test_config_cache_query_empty_json() {
    let home = temp_home("config-cache-query-empty-json");
    let (stdout, stderr, code) =
        run_cli_json_in_home(&["config", "cache", "query", "--json"], &home);
    assert_eq!(
        code, 0,
        "config cache query --json should exit 0; stderr: {stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "[]",
        "empty JSON should be []; got: {stdout}"
    );
}

#[test]
fn test_config_cache_query_with_filters_table() {
    let home = temp_home("config-cache-query-table");
    seed_cache_estimate(
        &home,
        "1111111111111111111111111111111111111111111111111111111111111111",
        "transfer",
        "testnet",
        150_000,
        12_000,
        "2026-02-01T10:00:00Z",
    );
    seed_cache_estimate(
        &home,
        "2222222222222222222222222222222222222222222222222222222222222222",
        "approve",
        "testnet",
        250_000,
        24_000,
        "2026-02-02T10:00:00Z",
    );

    let (stdout, stderr, code) = run_cli_in_home(
        &["config", "cache", "query", "--fn", "transfer"],
        Some(&home),
    );
    assert_eq!(code, 0, "query should succeed; stderr: {stderr}");
    assert!(
        stdout.contains("Timestamp"),
        "table should contain Timestamp"
    );
    assert!(stdout.contains("Function"), "table should contain Function");
    assert!(stdout.contains("CPU"), "table should contain CPU");
    assert!(stdout.contains("Fee"), "table should contain Fee");
    assert!(stdout.contains("transfer"), "table should contain transfer");
    assert!(
        !stdout.contains("approve"),
        "table should NOT contain approve"
    );
}

#[test]
fn test_config_cache_query_with_filters_json() {
    let home = temp_home("config-cache-query-json");
    seed_cache_estimate(
        &home,
        "1111111111111111111111111111111111111111111111111111111111111111",
        "transfer",
        "testnet",
        150_000,
        12_000,
        "2026-02-01T10:00:00Z",
    );
    seed_cache_estimate(
        &home,
        "2222222222222222222222222222222222222222222222222222222222222222",
        "approve",
        "testnet",
        250_000,
        24_000,
        "2026-02-02T10:00:00Z",
    );

    let (stdout, stderr, code) = run_cli_json_in_home(
        &["config", "cache", "query", "--fn", "transfer", "--json"],
        &home,
    );
    assert_eq!(code, 0, "query --json should succeed; stderr: {stderr}");
    let val: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    let arr = val.as_array().expect("json array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["function"], "transfer");
    assert_eq!(arr[0]["total_stroops"], 150_000);
}

#[test]
fn test_config_cache_query_no_filters_returns_all_networks() {
    let home = temp_home("config-cache-query-all");
    seed_cache_estimate(
        &home,
        "1111111111111111111111111111111111111111111111111111111111111111",
        "fn_testnet",
        "testnet",
        150_000,
        12_000,
        "2026-02-01T10:00:00Z",
    );
    seed_cache_estimate(
        &home,
        "2222222222222222222222222222222222222222222222222222222222222222",
        "fn_mainnet",
        "mainnet",
        250_000,
        24_000,
        "2026-02-02T10:00:00Z",
    );

    let (stdout, stderr, code) =
        run_cli_json_in_home(&["config", "cache", "query", "--json"], &home);
    assert_eq!(code, 0, "query --json should succeed; stderr: {stderr}");
    let val: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    let arr = val.as_array().expect("json array");
    assert_eq!(
        arr.len(),
        2,
        "query without filters should return all cached entries"
    );
}

#[test]
fn test_config_cache_query_invalid_network_error() {
    let home = temp_home("config-cache-query-inv-net");
    let (_stdout, stderr, code) = run_cli_in_home(
        &["config", "cache", "query", "--network", "invalid_net_xyz"],
        Some(&home),
    );
    assert_ne!(code, 0, "invalid network must fail");
    assert!(
        stderr.contains("not configured for network") || stderr.contains("invalid_net_xyz"),
        "stderr should mention invalid network; got: {stderr}"
    );
}

#[test]
fn test_config_cache_query_invalid_fee_range_error() {
    let home = temp_home("config-cache-query-inv-fee");
    let (_stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "cache",
            "query",
            "--min-fee",
            "200000",
            "--max-fee",
            "100000",
        ],
        Some(&home),
    );
    assert_ne!(code, 0, "invalid fee range must fail");
    assert!(
        stderr.contains("cannot exceed max-fee"),
        "stderr should mention invalid fee range; got: {stderr}"
    );
}

#[test]
fn test_config_cache_query_invalid_since_error() {
    let home = temp_home("config-cache-query-inv-since");
    let (_stdout, stderr, code) = run_cli_in_home(
        &["config", "cache", "query", "--since", "bad-date-format"],
        Some(&home),
    );
    assert_ne!(code, 0, "invalid since date must fail");
    assert!(
        stderr.contains("invalid timestamp or date"),
        "stderr should mention invalid timestamp or date; got: {stderr}"
    );
}

#[test]
fn test_config_cache_query_invalid_wasm_hash_error() {
    let home = temp_home("config-cache-query-inv-hash");
    let (_stdout, stderr, code) = run_cli_in_home(
        &["config", "cache", "query", "--wasm-hash", "xyz_not_hex"],
        Some(&home),
    );
    assert_ne!(code, 0, "invalid wasm hash must fail");
    assert!(
        stderr.contains("hexadecimal"),
        "stderr should mention hexadecimal; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `cache clear` / `estimate --clear-cache` (Issue #24)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_cache_clear_help() {
    let (stdout, stderr, code) = run_cli(&["cache", "clear", "--help"]);
    assert_eq!(
        code, 0,
        "cache clear --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("--network"),
        "cache clear help should mention --network; got: {stdout}"
    );
}

#[test]
fn test_cache_clear_on_empty_cache_succeeds() {
    // Default network is testnet; a pristine cache reports zero cleared.
    let home = temp_home("cache-clear-empty");
    let (stdout, stderr, code) = run_cli_in_home(&["cache", "clear"], Some(&home));
    assert_eq!(
        code, 0,
        "cache clear on empty cache should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Cleared 0 cached estimate(s) for testnet."),
        "should report zero cleared for testnet; got: {stdout}"
    );
}

#[test]
fn test_cache_clear_removes_only_requested_network() {
    let home = temp_home("cache-clear-network");
    let now = chrono::Utc::now().to_rfc3339();
    // Distinct function names per row: the cache key is (wasm_hash, function,
    // args_hash) and does not include the network, so reusing "(wasm upload)"
    // on mainnet would overwrite the testnet row.
    seed_cache_entry_for(&home, "testnet", "(wasm upload)", 42, &now);
    seed_cache_entry_for(&home, "testnet", "increment", 42, &now);
    seed_cache_entry_for(&home, "mainnet", "mainnet_fn", 77, &now);

    // Default `cache clear` targets testnet only.
    let (stdout, stderr, code) = run_cli_in_home(&["cache", "clear"], Some(&home));
    assert_eq!(code, 0, "cache clear should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Cleared 2 cached estimate(s) for testnet."),
        "should report both testnet entries cleared; got: {stdout}"
    );

    // testnet is empty now; mainnet is untouched.
    let (stdout, _, code) =
        run_cli_in_home(&["cache", "query", "--network", "testnet"], Some(&home));
    assert_eq!(code, 0);
    assert!(
        stdout.contains("No cached estimates match the query."),
        "testnet should have no entries left; got: {stdout}"
    );
    let (stdout, _, _) = run_cli_in_home(&["cache", "query", "--network", "mainnet"], Some(&home));
    assert!(
        !stdout.contains("No cached estimates match the query.") && stdout.contains("mainnet_fn"),
        "mainnet entry should survive the testnet clear; got: {stdout}"
    );

    // An explicit --network clears only that network.
    let (stdout, stderr, code) =
        run_cli_in_home(&["cache", "clear", "--network", "mainnet"], Some(&home));
    assert_eq!(
        code, 0,
        "cache clear mainnet should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Cleared 1 cached estimate(s) for mainnet."),
        "should report the mainnet entry cleared; got: {stdout}"
    );
    let (stdout, _, _) = run_cli_in_home(&["cache", "query", "--network", "mainnet"], Some(&home));
    assert!(
        stdout.contains("No cached estimates match the query."),
        "mainnet should be empty after its own clear; got: {stdout}"
    );
}

#[test]
fn test_estimate_clear_cache_flag_accepted() {
    // --clear-cache must be a recognized estimate flag (the run fails on the
    // missing WASM file, not on the argument).
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--clear-cache"]);
    assert_ne!(code, 0, "missing WASM file should still error");
    assert!(
        !stderr.contains("unexpected argument"),
        "--clear-cache should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_clear_cache_wipes_network_before_simulation() {
    // A fresh testnet entry plus --cache-ttl would otherwise short-circuit on
    // a cache hit. With --clear-cache the entry is wiped first, so the run
    // falls through to the (dead) RPC endpoint — proving the clear ran before
    // the simulation. The mainnet entry must survive untouched.
    let home = temp_home("estimate-clear-cache");
    let now = chrono::Utc::now().to_rfc3339();
    // The testnet row must use the exact key `estimate` looks up for
    // minimal.wasm (function "(wasm upload)", no args); the mainnet row uses
    // a distinct function name so the two networks' rows coexist.
    seed_cache_entry_for(&home, "testnet", "(wasm upload)", 42, &now);
    seed_cache_entry_for(&home, "mainnet", "mainnet_fn", 77, &now);

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--cache-ttl",
            "1h",
            "--clear-cache",
            "--network",
            "testnet",
            "--rpc-url",
            DEAD_RPC,
        ],
        Some(&home),
    );
    assert_eq!(
        code, 1,
        "cleared cache should fall through to the dead RPC endpoint; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Cleared 1 cached estimate(s) for testnet."),
        "stdout should announce the clear; got: {stdout}"
    );
    assert!(
        !stdout.contains("Cache hit"),
        "--clear-cache must prevent a cache hit; got: {stdout}"
    );

    // testnet is empty; mainnet is untouched.
    let (stdout, _, _) = run_cli_in_home(&["cache", "query", "--network", "testnet"], Some(&home));
    assert!(
        stdout.contains("No cached estimates match the query."),
        "testnet should have no entries left; got: {stdout}"
    );
    let (stdout, _, _) = run_cli_in_home(&["cache", "query", "--network", "mainnet"], Some(&home));
    assert!(
        stdout.contains("mainnet_fn"),
        "mainnet entries should survive the testnet clear; got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Simulation footprint metrics tests (Issue #2)
// ─────────────────────────────────────────────────────────────────────────

/// Spawns a lightweight local HTTP mock JSON-RPC server on loopback to test
/// simulation response parsing end-to-end without touching external networks.
fn start_mock_rpc_server(
    live_tx_data: &'static str,
    min_fee: &'static str,
    ledger: u64,
) -> (String, std::sync::mpsc::Sender<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().expect("local addr");
    let (tx_stop, rx_stop) = std::sync::mpsc::channel::<()>();

    let live_tx_data = live_tx_data.to_string();
    let min_fee = min_fee.to_string();

    std::thread::spawn(move || {
        listener.set_nonblocking(true).expect("set nonblocking");
        loop {
            if rx_stop.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 4096];
                    let mut req_str = String::new();
                    loop {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                req_str.push_str(&String::from_utf8_lossy(&buf[..n]));
                                if let Some(header_end) = req_str.find("\r\n\r\n") {
                                    let body_start = header_end + 4;
                                    // HTTP clients serialize header names in
                                    // lowercase (`content-length`), so the
                                    // length must be matched
                                    // case-insensitively. Reading the whole
                                    // body before replying is essential:
                                    // closing the socket while the client is
                                    // still writing a large body (e.g. a WASM
                                    // upload envelope) surfaces as a failed
                                    // HTTP send.
                                    let content_length = req_str[..header_end]
                                        .lines()
                                        .find_map(|line| {
                                            let (name, value) = line.split_once(':')?;
                                            if name.trim().eq_ignore_ascii_case("content-length") {
                                                value.trim().parse::<usize>().ok()
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or(0);
                                    if req_str.len().saturating_sub(body_start) >= content_length {
                                        break;
                                    }
                                }
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(std::time::Duration::from_millis(5));
                            }
                            Err(_) => break,
                        }
                    }

                    let resp_body = if req_str.contains("simulateTransaction") {
                        if live_tx_data.is_empty() {
                            format!(
                                r#"{{"jsonrpc":"2.0","id":1,"result":{{"latestLedger":"{ledger}","minResourceFee":"{min_fee}"}}}}"#
                            )
                        } else {
                            format!(
                                r#"{{"jsonrpc":"2.0","id":1,"result":{{"latestLedger":"{ledger}","minResourceFee":"{min_fee}","transactionData":"{live_tx_data}"}}}}"#
                            )
                        }
                    } else if req_str.contains("getHealth") {
                        format!(
                            r#"{{"jsonrpc":"2.0","id":1,"result":{{"status":"healthy","latestLedger":{ledger}}}}}"#
                        )
                    } else if req_str.contains("getLedgerEntries") {
                        format!(
                            r#"{{"jsonrpc":"2.0","id":1,"result":{{"latestLedger":{ledger},"entries":[]}}}}"#
                        )
                    } else {
                        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#.to_string()
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    (format!("http://127.0.0.1:{}", addr.port()), tx_stop)
}

const LIVE_INCREMENT_TX_DATA: &str = "AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAEmU1Fc+h02S4iEBnpjdCESXpKHG/bOUxC3DeRWUy9+mQAAABQAAAABAAggFgAAAAAAAACIAAAAAAAAPEM=";

#[test]
fn test_estimate_fn_contract_fixture_populates_footprint_json() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-footprint-json");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");

    // Footprint metrics verification (Acceptance Criteria)
    assert_eq!(parsed["read_entries"], 1, "expected 1 read entry");
    assert!(
        parsed["write_entries"].as_u64().unwrap_or(0) >= 1,
        "expected write_entries >= 1"
    );
    assert_eq!(parsed["write_entries"], 1, "expected 1 write entry");
    assert_eq!(parsed["read_bytes"], 0, "expected 0 read bytes");
    assert_eq!(parsed["write_bytes"], 136, "expected 136 write bytes");
    assert_eq!(parsed["cpu_instructions"], 532_502);
    assert_eq!(parsed["fee"]["total_stroops"], 15_527);
}

#[test]
fn test_estimate_fn_contract_fixture_populates_footprint_table() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-footprint-table");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    // Verify table output contains the same resource metrics
    assert!(stdout.contains("Read Entries"));
    assert!(stdout.contains("Write Entries"));
    assert!(stdout.contains("Read Bytes"));
    assert!(stdout.contains("Write Bytes"));
    assert!(
        stdout.contains("136"),
        "table should display 136 write bytes"
    );
    assert!(
        stdout.contains("15527"),
        "table should display total fee 15527"
    );
}

#[test]
fn test_estimate_minimal_wasm_upload_zero_footprint() {
    // minimal.wasm (upload path, no footprint) still reports zeros without error
    let (rpc_url, _stop) = start_mock_rpc_server("", "1000", 100);
    let home = temp_home("estimate-minimal-upload");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--rpc-url",
            &rpc_url,
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");

    assert_eq!(parsed["read_entries"], 0);
    assert_eq!(parsed["write_entries"], 0);
    assert_eq!(parsed["read_bytes"], 0);
    assert_eq!(parsed["write_bytes"], 0);
}

/// Regression: the mock server must read the *entire* request body before
/// answering. HTTP clients serialize header names in lowercase
/// (`content-length`), and a large body — such as a WASM upload envelope —
/// spans several reads. Replying early closes the socket while the client is
/// still writing, which surfaces as "failed to send HTTP request" (observed
/// on Windows CI for the `--diff` and cache-quota end-to-end tests).
#[test]
fn test_mock_rpc_server_drains_full_request_body() {
    use std::io::{Read, Write};

    let (rpc_url, _stop) = start_mock_rpc_server("", "1000", 100);
    let addr = rpc_url.strip_prefix("http://").expect("http url");
    let mut stream = std::net::TcpStream::connect(addr).expect("connect mock server");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("set read timeout");

    // A body much larger than the server's read buffer, carrying the method
    // name in the body (as JSON-RPC does) rather than in the request path.
    let body = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"simulateTransaction\",\"params\":{{\"padding\":\"{}\"}}}}",
        "x".repeat(8_000)
    );
    let headers = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    // Headers and body are written separately so the server cannot observe
    // the complete request in its first read.
    stream.write_all(headers.as_bytes()).expect("write headers");
    let _ = stream.flush();
    std::thread::sleep(std::time::Duration::from_millis(50));
    stream.write_all(body.as_bytes()).expect("write body");
    let _ = stream.flush();

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");

    assert!(
        response.contains("minResourceFee"),
        "server must answer simulateTransaction once the full body is read; got: {response}"
    );
    assert!(
        !response.contains("Method not found"),
        "answering before draining the body yields a bogus method-not-found reply; got: {response}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `estimate --diff` — side-by-side comparison (Issue #332)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_help_lists_diff_flags() {
    let (stdout, stderr, code) = run_cli(&["estimate", "--help"]);
    assert_eq!(code, 0, "estimate --help should exit 0; stderr: {stderr}");
    for flag in ["--diff", "--wasm-new"] {
        assert!(
            stdout.contains(flag),
            "estimate help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_estimate_diff_requires_wasm_new() {
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/minimal.wasm",
        "--diff",
    ]);
    assert_ne!(code, 0, "--diff without --wasm-new must be rejected");
    assert!(
        stderr.contains("--wasm-new") || stderr.contains("required"),
        "stderr should name the missing flag; got: {stderr}"
    );
}

#[test]
fn test_estimate_wasm_new_requires_diff() {
    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/minimal.wasm",
        "--wasm-new",
        "tests/fixtures/contract.wasm",
    ]);
    assert_ne!(code, 0, "--wasm-new without --diff must be rejected");
    assert!(
        stderr.contains("--wasm-new requires --diff"),
        "stderr should explain the pairing; got: {stderr}"
    );
}

#[test]
fn test_estimate_diff_requires_function_signature_in_both_wasms() {
    for (old_wasm, new_wasm, missing_flag) in [
        (
            "tests/fixtures/minimal.wasm",
            "tests/fixtures/contract.wasm",
            "--wasm",
        ),
        (
            "tests/fixtures/contract.wasm",
            "tests/fixtures/minimal.wasm",
            "--wasm-new",
        ),
    ] {
        let (_, stderr, code) = run_cli(&[
            "estimate",
            "--wasm",
            old_wasm,
            "--wasm-new",
            new_wasm,
            "--diff",
            "--fn",
            "increment",
        ]);
        assert_ne!(code, 0, "missing signature from {missing_flag} must fail");
        assert!(
            stderr.contains(&format!("missing from {missing_flag} WASM")),
            "error should identify missing signature in {missing_flag}; got: {stderr}"
        );
    }
}

#[test]
fn test_estimate_diff_table_end_to_end() {
    let (rpc_url, _stop) = start_mock_rpc_server("", "1000", 100);
    let home = temp_home("estimate-diff-table");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--wasm-new",
            "tests/fixtures/contract.wasm",
            "--diff",
            "--rpc-url",
            &rpc_url,
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate --diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate --diff should succeed; stderr: {stderr}"
    );

    // The 4-column comparison table and every required resource row.
    for label in [
        "Resource",
        "Old",
        "New",
        "Change (+/- %)",
        "WASM Size",
        "CPU Instructions",
        "RAM Bytes",
        "Read Entries",
        "Write Entries",
        "Read Bytes",
        "Write Bytes",
        "Total Fee",
    ] {
        assert!(
            stdout.contains(label),
            "diff output should include {label}; got: {stdout}"
        );
    }
    assert!(
        stdout.contains("Old WASM SHA-256") && stdout.contains("New WASM SHA-256"),
        "diff should name both artifacts; got: {stdout}"
    );
}

#[test]
fn test_estimate_diff_json_structure() {
    let (rpc_url, _stop) = start_mock_rpc_server("", "1000", 100);
    let home = temp_home("estimate-diff-json");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/minimal.wasm",
            "--wasm-new",
            "tests/fixtures/contract.wasm",
            "--diff",
            "--rpc-url",
            &rpc_url,
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate --diff --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate --diff --json should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    assert_eq!(parsed["wasm_a"]["function"], "(wasm upload)");
    assert_eq!(parsed["wasm_b"]["function"], "(wasm upload)");
    assert_eq!(parsed["wasm_a"]["network"], "testnet");
    assert_eq!(parsed["wasm_b"]["network"], "testnet");
    assert!(parsed["wasm_a"]["fee"]["total_stroops"].is_number());
    assert!(parsed["wasm_b"]["fee"]["total_stroops"].is_number());

    assert_eq!(parsed["diff"]["identity"]["network"], "testnet");
    let rows = parsed["diff"]["rows"].as_array().expect("diff rows array");
    assert_eq!(rows.len(), 8, "one row per compared resource");
    assert_eq!(rows[0]["resource"], "WASM Size");
    // The two fixtures differ in size, so the WASM row must carry a delta.
    assert!(
        rows[0]["delta"].as_i64().unwrap_or(0) != 0,
        "WASM Size should differ between the fixtures: {}",
        rows[0]
    );
    assert!(
        rows[0]["direction"].is_string(),
        "each row carries a direction: {}",
        rows[0]
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Cache quotas & maintenance (Issues #333 / #334)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_cache_quota_flags_are_accepted() {
    // Global quota flags must parse in both positions and reach the run.
    let home = temp_home("cache-quota-flags");
    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "--max-cache-size-mb",
            "1",
            "cache",
            "stats",
            "--max-cache-entries",
            "25",
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "cache stats should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Byte quota:") && stdout.contains("1.0 MB"),
        "stats should report the configured byte quota; got: {stdout}"
    );
    assert!(
        stdout.contains("25"),
        "stats should report the configured entry quota; got: {stdout}"
    );
}

#[test]
fn test_cache_prune_empty_cache_reports_zero() {
    let home = temp_home("cache-prune-empty");
    let (stdout, stderr, code) = run_cli_in_home(&["cache", "prune"], Some(&home));
    assert_eq!(code, 0, "cache prune should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Evicted 0 cached estimate(s)."),
        "an under-quota cache should evict nothing; got: {stdout}"
    );
}

#[test]
fn test_cache_help_lists_stats_and_prune() {
    let (stdout, stderr, code) = run_cli(&["cache", "--help"]);
    assert_eq!(code, 0, "cache --help should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("stats"),
        "cache help should list stats; got: {stdout}"
    );
    assert!(
        stdout.contains("prune"),
        "cache help should list prune; got: {stdout}"
    );
}

#[test]
fn test_estimate_evicts_when_entry_quota_exceeded() {
    // With a 1-entry quota, two estimates on distinct keys leave exactly one
    // cached row behind — proven through the CLI surface, not the library.
    let (rpc_url, _stop) = start_mock_rpc_server("", "1000", 100);
    let home = temp_home("estimate-evict-quota");

    for wasm in [
        "tests/fixtures/minimal.wasm",
        "tests/fixtures/contract.wasm",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
            .args([
                "--max-cache-entries",
                "1",
                "estimate",
                "--wasm",
                wasm,
                "--rpc-url",
                &rpc_url,
                "--json",
            ])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("RUST_LOG", "error")
            .output()
            .expect("failed to run estimate");
        assert_eq!(
            output.status.code(),
            Some(0),
            "estimate should succeed; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // RUST_LOG=error keeps tracing output off stdout so the JSON parses.
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(["cache", "query", "--json"])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run cache query");
    assert_eq!(
        output.status.code(),
        Some(0),
        "cache query should exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON; got: {stdout}");
    assert_eq!(
        parsed.as_array().map(Vec::len),
        Some(1),
        "the entry quota should leave exactly one cached estimate; got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Shell completions
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_completions_help() {
    let (stdout, stderr, code) = run_cli(&["completions", "--help"]);
    assert_eq!(
        code, 0,
        "completions --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("bash"),
        "completions help should list bash option"
    );
    assert!(
        stdout.contains("zsh"),
        "completions help should list zsh option"
    );
    assert!(
        stdout.contains("fish"),
        "completions help should list fish option"
    );
    assert!(
        stdout.contains("powershell"),
        "completions help should list powershell option"
    );
    assert!(
        stdout.contains("elvish"),
        "completions help should list elvish option"
    );
}

#[test]
fn test_completions_bash() {
    let (stdout, stderr, code) = run_cli(&["completions", "bash"]);
    assert_eq!(code, 0, "completions bash should exit 0; stderr: {stderr}");
    assert!(!stdout.is_empty(), "completion script should not be empty");
    assert!(
        stdout.contains("soroban-cost-estimator"),
        "bash completion script should contain binary name"
    );
    assert!(
        stdout.contains("estimate"),
        "bash completion script should contain subcommand names"
    );
    for network in ["testnet", "mainnet", "futurenet", "local"] {
        assert!(
            stdout.contains(network),
            "bash completion script should include {network}"
        );
    }
}

#[test]
fn test_completions_zsh() {
    let (stdout, stderr, code) = run_cli(&["completions", "zsh"]);
    assert_eq!(code, 0, "completions zsh should exit 0; stderr: {stderr}");
    assert!(!stdout.is_empty(), "completion script should not be empty");
    assert!(
        stdout.contains("soroban-cost-estimator"),
        "zsh completion script should contain binary name"
    );
    assert!(
        stdout.contains("estimate"),
        "zsh completion script should contain subcommand names"
    );
}

#[test]
fn test_completions_fish() {
    let (stdout, stderr, code) = run_cli(&["completions", "fish"]);
    assert_eq!(code, 0, "completions fish should exit 0; stderr: {stderr}");
    assert!(!stdout.is_empty(), "completion script should not be empty");
    assert!(
        stdout.contains("soroban-cost-estimator"),
        "fish completion script should contain binary name"
    );
    assert!(
        stdout.contains("estimate"),
        "fish completion script should contain subcommand names"
    );
}

#[test]
fn test_completions_powershell() {
    let (stdout, stderr, code) = run_cli(&["completions", "powershell"]);
    assert_eq!(
        code, 0,
        "completions powershell should exit 0; stderr: {stderr}"
    );
    assert!(!stdout.is_empty(), "completion script should not be empty");
    assert!(
        stdout.contains("soroban-cost-estimator"),
        "powershell completion script should contain binary name"
    );
    assert!(
        stdout.contains("estimate"),
        "powershell completion script should contain subcommand names"
    );
}

#[test]
fn test_completions_elvish() {
    let (stdout, stderr, code) = run_cli(&["completions", "elvish"]);
    assert_eq!(
        code, 0,
        "completions elvish should exit 0; stderr: {stderr}"
    );
    assert!(!stdout.is_empty(), "completion script should not be empty");
    assert!(
        stdout.contains("soroban-cost-estimator"),
        "elvish completion script should contain binary name"
    );
    assert!(
        stdout.contains("estimate"),
        "elvish completion script should contain subcommand names"
    );
}

#[test]
fn test_completions_unsupported_shell() {
    let (_stdout, stderr, code) = run_cli(&["completions", "invalid_shell"]);
    assert_ne!(code, 0, "unsupported shell should exit non-zero");
    assert!(
        stderr.contains("invalid value 'invalid_shell'") || stderr.contains("unexpected argument"),
        "stderr should state invalid shell value; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Cost projections (`--project`)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_project_flag_custom_counts_table() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-project-table");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--project",
            "100,1000,10000",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    assert!(stdout.contains("Cost Projections:"));
    assert!(stdout.contains("Invocations"));
    assert!(stdout.contains("Total Stroops"));
    assert!(stdout.contains("Total XLM"));
    assert!(stdout.contains("USD"));
    assert!(stdout.contains("100"));
    assert!(stdout.contains("1,552,700"));
    assert!(stdout.contains("0.1552700"));
    assert!(stdout.contains("1,000"));
    assert!(stdout.contains("15,527,000"));
    assert!(stdout.contains("1.5527000"));
    assert!(stdout.contains("10,000"));
    assert!(stdout.contains("155,270,000"));
    assert!(stdout.contains("15.5270000"));
    assert!(stdout.contains('-'));
}

#[test]
fn test_estimate_project_flag_default_counts() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-project-default");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--project",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    assert!(stdout.contains("Cost Projections:"));
    assert!(stdout.contains("100"));
    assert!(stdout.contains("1,000"));
    assert!(stdout.contains("10,000"));
}

#[test]
fn test_estimate_project_flag_preserves_order() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-project-order");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--project",
            "10000,100,1000",
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    let projections = parsed["projections"].as_array().expect("projections array");
    assert_eq!(projections.len(), 3);
    assert_eq!(projections[0]["invocations"], 10000);
    assert_eq!(projections[1]["invocations"], 100);
    assert_eq!(projections[2]["invocations"], 1000);
}

#[test]
fn test_estimate_no_project_flag_omits_projections() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-no-project");

    // Table mode without --project
    let output_table = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout_table = String::from_utf8_lossy(&output_table.stdout);
    assert!(!stdout_table.contains("Cost Projections:"));

    // JSON mode without --project
    let output_json = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout_json = String::from_utf8_lossy(&output_json.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout_json.trim()).expect("valid JSON output; got: {stdout_json}");
    assert!(parsed.get("projections").is_none());
}

#[test]
fn test_estimate_project_json_structure() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-project-json");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "estimate",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--id",
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "--fn",
            "increment",
            "--arg",
            "1",
            "--rpc-url",
            &rpc_url,
            "--project",
            "100,1000,10000",
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    let projections = parsed["projections"].as_array().expect("projections array");
    assert_eq!(projections.len(), 3);

    assert_eq!(projections[0]["invocations"], 100);
    assert_eq!(projections[0]["total_stroops"], 1_552_700);
    assert_eq!(projections[0]["total_xlm"], "0.1552700");
    assert!(projections[0].get("usd").is_none());

    assert_eq!(projections[1]["invocations"], 1000);
    assert_eq!(projections[1]["total_stroops"], 15_527_000);
    assert_eq!(projections[1]["total_xlm"], "1.5527000");

    assert_eq!(projections[2]["invocations"], 10000);
    assert_eq!(projections[2]["total_stroops"], 155_270_000);
    assert_eq!(projections[2]["total_xlm"], "15.5270000");
}

#[test]
fn test_estimate_project_invalid_input_error() {
    let (_stdout, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--project",
        "abc",
    ]);
    assert_ne!(code, 0, "invalid projection count 'abc' should fail");
    assert!(
        stderr.contains("invalid projection count 'abc'"),
        "stderr should mention invalid count: {stderr}"
    );

    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--project",
        "100,abc,1000",
    ]);
    assert_ne!(
        code, 0,
        "invalid projection count '100,abc,1000' should fail"
    );
    assert!(
        stderr.contains("invalid projection count 'abc'"),
        "stderr should mention invalid count: {stderr}"
    );

    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--project",
        "0",
    ]);
    assert_ne!(code, 0, "projection count 0 should fail");
    assert!(
        stderr.contains("greater than zero"),
        "stderr should mention greater than zero: {stderr}"
    );

    let (_, stderr, code) = run_cli(&[
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--project",
        "100,100",
    ]);
    assert_ne!(code, 0, "duplicate projection count should fail");
    assert!(
        stderr.contains("duplicate projection count: 100"),
        "stderr should mention duplicate count: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `estimate-all --fn` filter (Issue #25)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_all_fn_flag_accepted() {
    let (_, stderr, code) = run_cli(&[
        "estimate-all",
        "--wasm",
        "test.wasm",
        "--fn",
        "increment",
        "--fn",
        "transfer",
    ]);
    assert_ne!(code, 0, "missing WASM file should still error");
    assert!(
        !stderr.contains("unexpected argument"),
        "--fn should be a recognized, repeatable argument; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_all_fn_unknown_function_errors() {
    // A typo must fail loudly, listing the available functions, before any
    // RPC call.
    let home = temp_home("estimate-all-fn-unknown");
    let (_, stderr, code) = run_cli_in_home(
        &[
            "estimate-all",
            "--wasm",
            "tests/fixtures/contract.wasm",
            "--fn",
            "no_such_function",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "an unknown --fn should exit 1; stderr: {stderr}");
    assert!(
        stderr.contains("not found in WASM"),
        "the error should say the function was not found; got: {stderr}"
    );
    assert!(
        stderr.contains("increment"),
        "the error should list the available functions; got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// `--no-cache` tests (Issue #271)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_estimate_no_cache_flag_accepted() {
    // The flag must be recognized (failure is the missing file, not the arg).
    let (_, stderr, code) = run_cli(&["estimate", "--wasm", "test.wasm", "--no-cache"]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("unrecognized"),
        "--no-cache should be a recognized argument; stderr: {stderr}"
    );
}

#[test]
fn test_estimate_all_no_cache_flag_accepted() {
    let (_, stderr, code) = run_cli(&["estimate-all", "--wasm", "test.wasm", "--no-cache"]);
    assert_ne!(code, 0, "should error on missing file, not invalid args");
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("unrecognized"),
        "--no-cache should be a recognized argument; stderr: {stderr}"
    );
}

/// `--no-cache` skips both the cache read and the cache write:
///
/// 1. a run with `--no-cache --cache-ttl` always simulates (never returns a
///    cache-hit payload) and leaves nothing behind on disk;
/// 2. a normal run populates the cache;
/// 3. a later `--cache-ttl` run *does* hit that cache entry — proving the
///    first run would have too, had `--no-cache` not bypassed it.
#[test]
fn test_no_cache_bypasses_cache_reads_and_writes() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("no-cache-bypass");
    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";

    let base: Vec<&str> = vec![
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--id",
        contract_id,
        "--fn",
        "increment",
        "--arg",
        "1",
        "--rpc-url",
        &rpc_url,
        "--json",
    ];

    // 1. Bypassed run: fresh simulation even though --cache-ttl is set.
    let mut args = base.clone();
    args.extend_from_slice(&["--no-cache", "--cache-ttl", "1h"]);
    let (stdout, stderr, code) = run_cli_quiet(&args, Some(&home));
    assert_eq!(
        code, 0,
        "no-cache estimate should succeed; stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("fresh JSON report; got: {stdout}");
    assert!(
        parsed.get("cache").is_none(),
        "--no-cache must not return a cache-hit payload; got: {stdout}"
    );
    assert_eq!(parsed["cpu_instructions"], 532_502);

    // ...and nothing was written to the cache.
    let (stdout, stderr, code) = run_cli_quiet(
        &["cache", "query", "--network", "testnet", "--json"],
        Some(&home),
    );
    assert_eq!(code, 0, "cache query should succeed; stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        "[]",
        "--no-cache must not persist an estimate; got: {stdout}"
    );

    // 2. A normal run does populate the cache.
    let (_, stderr, code) = run_cli_quiet(&base, Some(&home));
    assert_eq!(
        code, 0,
        "populating estimate should succeed; stderr: {stderr}"
    );
    let (stdout, _, _) = run_cli_quiet(
        &["cache", "query", "--network", "testnet", "--json"],
        Some(&home),
    );
    assert!(
        stdout.contains("increment"),
        "normal run should cache the estimate; got: {stdout}"
    );

    // 3. The cached entry is now visible to --cache-ttl...
    let mut cached_args = base.clone();
    cached_args.extend_from_slice(&["--cache-ttl", "1h"]);
    let (stdout, stderr, code) = run_cli_quiet(&cached_args, Some(&home));
    assert_eq!(code, 0, "cached estimate should succeed; stderr: {stderr}");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("cache-hit JSON; got: {stdout}");
    assert_eq!(
        parsed["cache"], "hit",
        "expected a cache hit; got: {stdout}"
    );

    // ...but `--no-cache` ignores it and simulates anyway.
    let mut bypassed_args = base.clone();
    bypassed_args.extend_from_slice(&["--no-cache", "--cache-ttl", "1h"]);
    let (stdout, stderr, code) = run_cli_quiet(&bypassed_args, Some(&home));
    assert_eq!(
        code, 0,
        "bypassed estimate should succeed; stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("fresh JSON report; got: {stdout}");
    assert!(
        parsed.get("cache").is_none(),
        "--no-cache must ignore the cached entry; got: {stdout}"
    );
    assert_eq!(parsed["cpu_instructions"], 532_502);
}

// ─────────────────────────────────────────────────────────────────────────
// `estimate --repeat` benchmarking (Issue #289)
// ─────────────────────────────────────────────────────────────────────────

/// Args shared by the `--repeat` tests: a deterministic contract invocation
/// served by the mock RPC server.
fn repeat_estimate_args(rpc_url: &str) -> Vec<String> {
    [
        "estimate",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--id",
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        "--fn",
        "increment",
        "--arg",
        "1",
        "--rpc-url",
        rpc_url,
        "--repeat",
        "3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn test_estimate_repeat_table_reports_statistics() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-repeat-table");

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(repeat_estimate_args(&rpc_url))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .expect("failed to run estimate --repeat");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate --repeat should succeed; stderr: {stderr}"
    );

    for label in [
        "Iteration count",
        "Min latency (ms)",
        "Max latency (ms)",
        "Mean latency (ms)",
        "Stddev latency (ms)",
        "CPU Instructions",
        "Total Fee (stroops)",
    ] {
        assert!(
            stdout.contains(label),
            "summary should include '{label}'; got: {stdout}"
        );
    }
    assert!(
        stdout.contains("532502"),
        "summary should report CPU instructions; got: {stdout}"
    );
    assert!(
        stdout.contains("15427"),
        "summary should report total fee; got: {stdout}"
    );
    assert!(
        stdout.contains("identical"),
        "deterministic runs should be marked identical; got: {stdout}"
    );
}

#[test]
fn test_estimate_repeat_json_reports_latency_array() {
    let (rpc_url, _stop) = start_mock_rpc_server(LIVE_INCREMENT_TX_DATA, "15427", 3_894_195);
    let home = temp_home("estimate-repeat-json");

    let mut args = repeat_estimate_args(&rpc_url);
    args.push("--json".to_string());

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(&args)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run estimate --repeat --json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "estimate --repeat --json should succeed; stderr: {stderr}"
    );

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    let _ = stderr;

    assert_eq!(parsed["iterations"], 3);
    assert_eq!(
        parsed["latencies_ms"].as_array().map(|a| a.len()),
        Some(3),
        "latencies_ms should hold one sample per run; got: {stdout}"
    );
    for key in [
        "min_latency_ms",
        "max_latency_ms",
        "mean_latency_ms",
        "stddev_latency_ms",
    ] {
        assert!(
            parsed.get(key).is_some(),
            "JSON should include statistical metric {key}; got: {stdout}"
        );
    }
    assert_eq!(parsed["cpu_instructions"], 532_502);
    assert_eq!(parsed["cpu_identical"], true);
    assert_eq!(parsed["total_fee_stroops"], 15_427);
    assert_eq!(parsed["fee_identical"], true);
}

fn snapshot_json_fee(network: &str, ledger: u32, fee: i64) -> String {
    format!(
        r#"{{
  "network": "{network}",
  "ledger": {ledger},
  "timestamp": "2026-01-01T00:00:00+00:00",
  "contract_compute": {{
    "ledger_max_instructions": 1000000,
    "tx_max_instructions": 100000,
    "fee_rate_per_instructions_increment": {fee},
    "tx_memory_limit": 100
  }},
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}}"#
    )
}

/// Writes a snapshot named `name` into `home`'s snapshots directory and
/// returns its full path.
fn write_snapshot_file(home: &Path, name: &str, contents: &str) -> PathBuf {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write snapshot fixture");
    path
}

#[test]
fn test_config_snapshot_help_lists_management_subcommands() {
    let (stdout, stderr, code) = run_cli(&["config", "snapshot", "--help"]);
    assert_eq!(code, 0, "snapshot --help should exit 0: {stderr}");
    for subcommand in ["delete", "diff"] {
        assert!(
            stdout.contains(subcommand),
            "snapshot help should list the {subcommand} subcommand; got: {stdout}"
        );
    }
}

#[test]
fn test_config_snapshot_delete_help() {
    let (stdout, stderr, code) = run_cli(&["config", "snapshot", "delete", "--help"]);
    assert_eq!(code, 0, "delete --help should exit 0: {stderr}");
    for flag in ["--older-than", "--dry-run", "--yes"] {
        assert!(
            stdout.contains(flag),
            "delete help should mention {flag}; got: {stdout}"
        );
    }
}

#[test]
fn test_config_snapshot_diff_help() {
    let (stdout, stderr, code) = run_cli(&["config", "snapshot", "diff", "--help"]);
    assert_eq!(code, 0, "diff --help should exit 0: {stderr}");
    assert!(
        stdout.contains("--json"),
        "diff help should mention --json; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_delete_removes_named_file() {
    let home = temp_home("snapshot-delete-named");
    let name = "testnet-2026-01-01T00-00-00+00-00.json";
    let path = write_snapshot_file(&home, name, &snapshot_json("testnet", 100));

    let (stdout, stderr, code) = run_cli_in_home(
        &["config", "snapshot", "delete", name, "--yes"],
        Some(&home),
    );
    assert_eq!(code, 0, "delete should succeed; stderr: {stderr}");
    assert!(!path.exists(), "the named snapshot should be gone");
    assert!(
        stdout.contains("Deleted snapshot"),
        "stdout should confirm the deletion; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_delete_missing_file_errors() {
    let home = temp_home("snapshot-delete-missing");
    let (_, stderr, code) = run_cli_in_home(
        &["config", "snapshot", "delete", "nope.json", "--yes"],
        Some(&home),
    );
    assert_eq!(code, 1, "deleting a missing snapshot should exit 1");
    assert!(
        stderr.contains("not found") && stderr.contains("nope.json"),
        "the error should name the missing snapshot; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_delete_requires_selector() {
    let home = temp_home("snapshot-delete-no-selector");
    let (_, stderr, code) =
        run_cli_in_home(&["config", "snapshot", "delete", "--yes"], Some(&home));
    assert_eq!(code, 1, "delete without a selector should exit 1");
    assert!(
        stderr.contains("nothing to delete"),
        "the error should explain what to pass; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_delete_dry_run_keeps_file() {
    let home = temp_home("snapshot-delete-dry-run");
    let name = "testnet-2026-01-01T00-00-00+00-00.json";
    let path = write_snapshot_file(&home, name, &snapshot_json("testnet", 100));

    let (stdout, stderr, code) = run_cli_in_home(
        &["config", "snapshot", "delete", name, "--dry-run"],
        Some(&home),
    );
    assert_eq!(code, 0, "a dry run should exit 0; stderr: {stderr}");
    assert!(path.exists(), "a dry run must not delete anything");
    assert!(
        stdout.contains("Dry run"),
        "stdout should announce the dry run; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_delete_older_than_purges_only_old() {
    let home = temp_home("snapshot-delete-older-than");
    let old = write_snapshot_file(
        &home,
        "testnet-2026-01-01T00-00-00+00-00.json",
        &snapshot_json_at("testnet", 100, "2026-01-01T00:00:00+00:00"),
    );
    let recent = write_snapshot_file(
        &home,
        "testnet-recent.json",
        &snapshot_json_at("testnet", 200, &chrono::Utc::now().to_rfc3339()),
    );

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "delete",
            "--older-than",
            "30",
            "--network",
            "testnet",
            "--yes",
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "the purge should succeed; stderr: {stderr}");
    assert!(!old.exists(), "the stale snapshot should be purged");
    assert!(recent.exists(), "the recent snapshot must be kept");
    assert!(
        stdout.contains("Deleted 1 of 1 snapshot file(s)"),
        "stdout should report exactly one deletion; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_delete_older_than_dry_run() {
    let home = temp_home("snapshot-delete-older-than-dry");
    let old = write_snapshot_file(
        &home,
        "testnet-2026-01-01T00-00-00+00-00.json",
        &snapshot_json_at("testnet", 100, "2026-01-01T00:00:00+00:00"),
    );

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "delete",
            "--older-than",
            "30",
            "--dry-run",
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "a dry run should exit 0; stderr: {stderr}");
    assert!(old.exists(), "a dry run must not delete anything");
    assert!(
        stdout.contains("Dry run"),
        "stdout should announce the dry run; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_diff_identical_exits_zero() {
    let home = temp_home("snapshot-diff-identical");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json("testnet", 100));
    let b = write_snapshot_file(&home, "b.json", &snapshot_json("testnet", 100));

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "identical snapshots should exit 0: {stderr}");
    assert!(
        stdout.contains("No changes detected"),
        "stdout should report no changes across identical snapshots; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_diff_pricing_change_exits_one() {
    let home = temp_home("snapshot-diff-pricing");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json_fee("testnet", 100, 100));
    let b = write_snapshot_file(&home, "b.json", &snapshot_json_fee("testnet", 101, 200));

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "pricing changes must exit 1 like config diff");
    assert!(
        stdout.contains("Pricing changes detected"),
        "stdout should warn about pricing changes; got: {stdout}"
    );
}

#[test]
fn test_config_snapshot_diff_json_flag() {
    let home = temp_home("snapshot-diff-json");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json_fee("testnet", 100, 100));
    let b = write_snapshot_file(&home, "b.json", &snapshot_json_fee("testnet", 101, 200));

    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--json",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "error")
        .output()
        .expect("failed to run config snapshot diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let status = output.status.code();
    assert_eq!(status, Some(1), "pricing changes exit 1 under --json");

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("valid JSON output; got: {stdout}");
    assert_eq!(parsed["has_pricing_changes"], true);
    assert_eq!(parsed["old_snapshot"]["ledger"], 100);
    assert_eq!(parsed["new_snapshot"]["ledger"], 101);
    assert_eq!(parsed["changes"].as_array().map(Vec::len), Some(1));
}

#[test]
fn test_config_snapshot_diff_missing_file_errors() {
    let home = temp_home("snapshot-diff-missing");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json("testnet", 100));

    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            "no/such/snapshot.json",
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "a missing snapshot should exit 1");
    assert!(
        stderr.contains("File not found") && stderr.contains("no/such/snapshot.json"),
        "the error should name the unreadable file; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_diff_malformed_file_errors() {
    let home = temp_home("snapshot-diff-malformed");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json("testnet", 100));
    let malformed = write_snapshot_file(&home, "broken.json", "{ not valid json");

    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            malformed.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 1, "a malformed snapshot should exit 1");
    assert!(
        stderr.contains("failed to parse snapshot") && stderr.contains("broken.json"),
        "the error should name the malformed file; got: {stderr}"
    );
}

#[test]
fn test_config_snapshot_diff_makes_no_network_calls() {
    // Both files load from disk; an unknown network name inside them must not
    // trigger any RPC resolution, so the command still succeeds offline.
    let home = temp_home("snapshot-diff-offline");
    let a = write_snapshot_file(&home, "a.json", &snapshot_json("not-a-network", 100));
    let b = write_snapshot_file(&home, "b.json", &snapshot_json("not-a-network", 100));

    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "snapshot",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
        Some(&home),
    );
    assert_eq!(code, 0, "offline diff should succeed: {stderr}");
    assert!(
        !stderr.contains("failed to locate RPC endpoint"),
        "offline diff must not resolve a network; got: {stderr}"
    );
}
