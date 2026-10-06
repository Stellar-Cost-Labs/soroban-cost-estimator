use soroban_cost_estimator::config_snapshot::store::validate_all_snapshots;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sce-validate-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp home");
    dir
}

fn run_validate(args: &[&str], home: &Path) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("RUST_LOG", "error")
        .output()
        .expect("run snapshot validation command");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

fn valid_snapshot_json(network: &str) -> String {
    format!(
        r#"{{"network":"{network}","timestamp":"2026-01-01T00:00:00Z","ledger":1,"contract_compute":null,"contract_ledger_cost":null,"contract_historical_data":null,"contract_events":null,"contract_bandwidth":null,"state_archival":null}}"#
    )
}

#[test]
fn test_validate_all_snapshots_returns_ok_for_empty_network() {
    // With no snapshots saved for a nonexistent network, validate returns Ok(empty)
    let result = validate_all_snapshots("nonexistent_network_xyz_999");
    assert!(result.is_ok());
    let results = result.unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_validate_all_snapshots_returns_empty_list() {
    // Verify the return type and structure
    let result = validate_all_snapshots("testnet");
    assert!(result.is_ok());
    let results = result.unwrap();
    // Each result has path, filename, valid, error fields
    for status in &results {
        assert!(!status.filename.is_empty());
        assert!(!status.path.as_os_str().is_empty());
        // Either valid or has an error message
        assert!(status.valid || status.error.is_some());
    }
}

#[test]
fn validate_accepts_a_valid_explicit_snapshot_path() {
    let home = temp_home("valid-path");
    let snapshot = home.join("snapshot.json");
    std::fs::write(&snapshot, valid_snapshot_json("testnet")).expect("write snapshot");
    let snapshot_path = snapshot.to_string_lossy().into_owned();

    let (stdout, stderr, code) =
        run_validate(&["config", "snapshot", "validate", &snapshot_path], &home);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("Valid:"), "stdout: {stdout}");
    assert!(stdout.contains("snapshot.json"), "stdout: {stdout}");
}

#[test]
fn validate_rejects_malformed_and_incomplete_snapshot_files() {
    let home = temp_home("invalid-path");
    let malformed = home.join("malformed.json");
    std::fs::write(&malformed, "{ not json").expect("write malformed snapshot");
    let malformed_path = malformed.to_string_lossy().into_owned();

    let (malformed_stdout, _, malformed_code) =
        run_validate(&["config", "snapshot", "validate", &malformed_path], &home);
    assert_eq!(malformed_code, 1);
    assert!(malformed_stdout.contains("Invalid:"));
    assert!(malformed_stdout.contains("invalid snapshot JSON"));

    let incomplete = home.join("incomplete.json");
    std::fs::write(&incomplete, r#"{"network":"testnet"}"#).expect("write incomplete snapshot");
    let incomplete_path = incomplete.to_string_lossy().into_owned();
    let (incomplete_stdout, _, incomplete_code) =
        run_validate(&["config", "snapshot", "validate", &incomplete_path], &home);
    assert_eq!(incomplete_code, 1);
    assert!(incomplete_stdout.contains("Invalid:"));
    assert!(incomplete_stdout.contains("missing field"));

    let malformed_setting = home.join("malformed-setting.json");
    let snapshot = valid_snapshot_json("testnet").replace(
        "\"contract_compute\":null",
        "\"contract_compute\":{\"ledger_max_instructions\":1}",
    );
    std::fs::write(&malformed_setting, snapshot).expect("write malformed setting");
    let malformed_setting_path = malformed_setting.to_string_lossy().into_owned();
    let (setting_stdout, _, setting_code) = run_validate(
        &["config", "snapshot", "validate", &malformed_setting_path],
        &home,
    );
    assert_eq!(setting_code, 1);
    assert!(setting_stdout.contains("Invalid:"));
    assert!(setting_stdout.contains("missing field"));
}

#[test]
fn validate_all_reports_each_network_and_fails_when_any_snapshot_is_invalid() {
    let home = temp_home("all");
    let snapshots = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&snapshots).expect("create snapshots directory");
    std::fs::write(
        snapshots.join("testnet-2026-01-01.json"),
        valid_snapshot_json("testnet"),
    )
    .expect("write valid snapshot");
    std::fs::write(snapshots.join("mainnet-2026-01-01.json"), "[]")
        .expect("write invalid snapshot");

    let (stdout, stderr, code) = run_validate(&["config", "snapshot", "validate", "--all"], &home);

    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stdout.contains("Valid: "), "stdout: {stdout}");
    assert!(stdout.contains("Invalid: "), "stdout: {stdout}");
    assert!(stdout.contains("testnet-2026-01-01.json"));
    assert!(stdout.contains("mainnet-2026-01-01.json"));
}
