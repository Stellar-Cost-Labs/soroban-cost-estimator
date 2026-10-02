//! Integration tests for `config snapshot list` (issue #275).
//!
//! Each test drives the real binary with `HOME` pointed at a per-test
//! temporary directory and writes snapshot files directly into
//! `~/.soroban-cost-estimator/snapshots/`. Fully offline.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sce-snaplist-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn snapshots_dir(home: &Path) -> PathBuf {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs the CLI. `RUST_LOG=error` keeps the startup `info!` line off stdout
/// so JSON output can be parsed directly (same convention as `cli_tests`).
fn run(home: &Path, args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("RUST_LOG", "error")
        .env_remove("SOROBAN_NETWORK")
        .env_remove("SOROBAN_JSON")
        .output()
        .expect("failed to run CLI");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Writes a structurally valid snapshot file. `protocol` of `None` produces
/// a file in the pre-#275 format, with no `protocol_version` key.
fn write_snapshot(
    home: &Path,
    filename: &str,
    network: &str,
    timestamp: &str,
    ledger: u32,
    protocol: Option<u32>,
) {
    let protocol_field = protocol
        .map(|p| format!("\"protocol_version\": {p},"))
        .unwrap_or_default();
    let body = format!(
        r#"{{
  "network": "{network}",
  "timestamp": "{timestamp}",
  "ledger": {ledger},
  {protocol_field}
  "contract_compute": {{
    "ledger_max_instructions": 1,
    "tx_max_instructions": 1,
    "fee_rate_per_instructions_increment": 25,
    "tx_memory_limit": 1
  }},
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}}"#
    );
    std::fs::write(snapshots_dir(home).join(filename), body).unwrap();
}

/// Two testnet snapshots and one mainnet snapshot, deliberately written out
/// of order.
fn seed(home: &Path) {
    write_snapshot(
        home,
        "testnet-2026-02-01T00-00-00+00-00.json",
        "testnet",
        "2026-02-01T00:00:00+00:00",
        2000,
        Some(23),
    );
    write_snapshot(
        home,
        "testnet-2026-01-01T00-00-00+00-00.json",
        "testnet",
        "2026-01-01T00:00:00+00:00",
        1000,
        Some(22),
    );
    write_snapshot(
        home,
        "mainnet-2026-01-15T00-00-00+00-00.json",
        "mainnet",
        "2026-01-15T00:00:00+00:00",
        5000,
        Some(22),
    );
}

fn json_list(home: &Path, extra: &[&str]) -> Vec<serde_json::Value> {
    let mut args = vec!["config", "snapshot", "list", "--json"];
    args.extend_from_slice(extra);
    let r = run(home, &args);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    serde_json::from_str::<Vec<serde_json::Value>>(&r.stdout)
        .unwrap_or_else(|e| panic!("stdout is not a JSON array ({e}): {}", r.stdout))
}

fn field<'a>(v: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    v.get(key)
        .unwrap_or_else(|| panic!("missing key {key} in {v}"))
}

// ── JSON: discovery and metadata ─────────────────────────────────────────

#[test]
fn all_json_lists_every_snapshot_with_correct_metadata_in_order() {
    let home = temp_home("all-json");
    seed(&home);
    let list = json_list(&home, &["--all"]);
    assert_eq!(list.len(), 3, "{list:?}");

    // Sorted by network, then timestamp (oldest first).
    let first = &list[0];
    assert_eq!(
        field(first, "filename"),
        "mainnet-2026-01-15T00-00-00+00-00.json"
    );
    assert_eq!(field(first, "network"), "mainnet");
    assert_eq!(field(first, "timestamp"), "2026-01-15T00:00:00+00:00");
    assert_eq!(field(first, "ledger_sequence"), 5000);
    assert_eq!(field(first, "protocol_version"), 22);

    let names: Vec<&str> = list
        .iter()
        .map(|v| field(v, "filename").as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "mainnet-2026-01-15T00-00-00+00-00.json",
            "testnet-2026-01-01T00-00-00+00-00.json",
            "testnet-2026-02-01T00-00-00+00-00.json",
        ]
    );
    assert_eq!(field(&list[2], "ledger_sequence"), 2000);
    assert_eq!(field(&list[2], "protocol_version"), 23);
}

#[test]
fn json_objects_have_exactly_the_documented_keys() {
    let home = temp_home("json-keys");
    seed(&home);
    for item in json_list(&home, &["--all"]) {
        let mut keys: Vec<&str> = item
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "filename",
                "ledger_sequence",
                "network",
                "protocol_version",
                "timestamp"
            ]
        );
    }
}

#[test]
fn json_output_contains_no_table_formatting() {
    let home = temp_home("json-no-table");
    seed(&home);
    let r = run(&home, &["config", "snapshot", "list", "--all", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let trimmed = r.stdout.trim();
    assert!(
        trimmed.starts_with('[') && trimmed.ends_with(']'),
        "{trimmed}"
    );
    for marker in ["+---", "+===", "| ", "Ledger Sequence", "snapshot(s)"] {
        assert!(
            !r.stdout.contains(marker),
            "found {marker:?} in {}",
            r.stdout
        );
    }
}

#[test]
fn legacy_snapshot_without_protocol_version_is_null_not_fabricated() {
    let home = temp_home("legacy");
    write_snapshot(
        &home,
        "testnet-old.json",
        "testnet",
        "2025-06-01T00:00:00+00:00",
        42,
        None,
    );
    let list = json_list(&home, &[]);
    assert_eq!(list.len(), 1);
    assert!(field(&list[0], "protocol_version").is_null());
    assert_eq!(field(&list[0], "ledger_sequence"), 42);

    let table = run(&home, &["config", "snapshot", "list"]).stdout;
    let row = table
        .lines()
        .find(|l| l.contains("testnet-old.json"))
        .unwrap();
    assert!(row.trim_end().ends_with("| -                |"), "{row}");
}

// ── Filtering ────────────────────────────────────────────────────────────

#[test]
fn default_lists_only_default_network() {
    let home = temp_home("default-net");
    seed(&home);
    let list = json_list(&home, &[]);
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|v| field(v, "network") == "testnet"));
}

#[test]
fn network_flag_filters_to_that_network() {
    let home = temp_home("net-flag");
    seed(&home);
    let list = json_list(&home, &["--network", "mainnet"]);
    assert_eq!(list.len(), 1);
    assert_eq!(field(&list[0], "network"), "mainnet");
}

#[test]
fn filtering_uses_stored_network_not_filename() {
    let home = temp_home("header-not-name");
    // Misleading filename: the file says it belongs to mainnet.
    write_snapshot(
        &home,
        "testnet-renamed.json",
        "mainnet",
        "2026-03-01T00:00:00+00:00",
        7,
        Some(23),
    );
    assert!(json_list(&home, &["--network", "testnet"]).is_empty());
    let mainnet = json_list(&home, &["--network", "mainnet"]);
    assert_eq!(mainnet.len(), 1);
    assert_eq!(field(&mainnet[0], "filename"), "testnet-renamed.json");
}

#[test]
fn non_json_files_are_ignored() {
    let home = temp_home("non-json");
    seed(&home);
    std::fs::write(snapshots_dir(&home).join("notes.txt"), "not a snapshot").unwrap();
    std::fs::create_dir_all(snapshots_dir(&home).join("subdir.json")).unwrap();
    assert_eq!(json_list(&home, &["--all"]).len(), 3);
}

// ── Table output ─────────────────────────────────────────────────────────

#[test]
fn table_has_required_columns_and_rows() {
    let home = temp_home("table");
    seed(&home);
    let r = run(&home, &["config", "snapshot", "list", "--all"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for header in [
        "Filename",
        "Network",
        "Timestamp",
        "Ledger Sequence",
        "Protocol Version",
    ] {
        assert!(r.stdout.contains(header), "missing {header}: {}", r.stdout);
    }
    let row = r
        .stdout
        .lines()
        .find(|l| l.contains("mainnet-2026-01-15T00-00-00+00-00.json"))
        .unwrap_or_else(|| panic!("no mainnet row: {}", r.stdout));
    for cell in ["mainnet", "2026-01-15T00:00:00+00:00", "5000", "22"] {
        assert!(row.contains(cell), "row missing {cell}: {row}");
    }
    assert!(r.stdout.contains("3 snapshot(s) for any network."));
    assert!(serde_json::from_str::<serde_json::Value>(&r.stdout).is_err());
}

#[test]
fn table_default_network_excludes_other_networks() {
    let home = temp_home("table-default");
    seed(&home);
    let r = run(&home, &["config", "snapshot", "list"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(!r.stdout.contains("mainnet-2026"), "{}", r.stdout);
    assert!(r.stdout.contains("2 snapshot(s) for network 'testnet'."));
}

// ── Empty ────────────────────────────────────────────────────────────────

#[test]
fn no_snapshot_dir_prints_friendly_message() {
    let home = temp_home("empty");
    let r = run(&home, &["config", "snapshot", "list"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout.contains(
            "No snapshots found for network 'testnet'. Run `config snapshot` to take one."
        ),
        "{}",
        r.stdout
    );
}

#[test]
fn no_matching_snapshots_prints_friendly_message() {
    let home = temp_home("empty-filtered");
    seed(&home);
    let r = run(
        &home,
        &["config", "snapshot", "list", "--network", "futurenet"],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout
            .contains("No snapshots found for network 'futurenet'.")
    );
}

#[test]
fn json_with_no_snapshots_is_empty_array() {
    let home = temp_home("empty-json");
    let r = run(&home, &["config", "snapshot", "list", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout.trim(), "[]");
    let r = run(&home, &["config", "snapshot", "list", "--all", "--json"]);
    assert_eq!(r.stdout.trim(), "[]");
}

// ── Errors ───────────────────────────────────────────────────────────────

#[test]
fn malformed_snapshot_fails_and_names_the_file() {
    let home = temp_home("malformed");
    seed(&home);
    std::fs::write(
        snapshots_dir(&home).join("testnet-broken.json"),
        "{ not json",
    )
    .unwrap();
    let r = run(&home, &["config", "snapshot", "list", "--all"]);
    assert_eq!(r.code, 1, "stdout: {}", r.stdout);
    assert!(
        r.stderr.contains("failed to parse snapshot") && r.stderr.contains("testnet-broken.json"),
        "{}",
        r.stderr
    );
}

#[test]
fn snapshot_missing_header_fields_fails() {
    let home = temp_home("missing-fields");
    std::fs::write(
        snapshots_dir(&home).join("testnet-partial.json"),
        r#"{"network":"testnet","timestamp":"2026-01-01T00:00:00+00:00"}"#,
    )
    .unwrap();
    let r = run(&home, &["config", "snapshot", "list"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("ledger"), "{}", r.stderr);
}

#[test]
fn unreadable_snapshot_fails_and_names_the_file() {
    let home = temp_home("unreadable");
    // Invalid UTF-8 cannot be read as text, whatever user the tests run as.
    std::fs::write(
        snapshots_dir(&home).join("testnet-binary.json"),
        [0xff, 0xfe, 0x00],
    )
    .unwrap();
    let r = run(&home, &["config", "snapshot", "list"]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("cannot read") && r.stderr.contains("testnet-binary.json"),
        "{}",
        r.stderr
    );
}

#[test]
fn network_and_all_together_are_rejected() {
    let home = temp_home("conflict");
    let r = run(
        &home,
        &[
            "config",
            "snapshot",
            "list",
            "--network",
            "testnet",
            "--all",
        ],
    );
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("cannot be used with"), "{}", r.stderr);
}

#[test]
fn list_help_documents_flags() {
    let home = temp_home("help");
    let r = run(&home, &["config", "snapshot", "list", "--help"]);
    assert_eq!(r.code, 0);
    for flag in ["--network", "--all", "--json"] {
        assert!(r.stdout.contains(flag), "missing {flag}: {}", r.stdout);
    }
}

// ── `config snapshot` itself is unchanged ────────────────────────────────

#[test]
fn bare_config_snapshot_still_takes_a_snapshot() {
    // An unknown network fails at endpoint resolution, before any network
    // traffic, which proves the leaf command still dispatches.
    let home = temp_home("bare");
    let r = run(&home, &["config", "snapshot", "--network", "nosuchnet"]);
    assert_eq!(r.code, 1, "{}", r.stdout);
    assert!(r.stderr.contains("nosuchnet"), "{}", r.stderr);
}

#[test]
fn snapshot_flags_cannot_be_mixed_with_list() {
    let home = temp_home("mixed");
    let r = run(&home, &["config", "snapshot", "--out", "x.json", "list"]);
    assert_eq!(r.code, 2, "{}", r.stdout);
}
