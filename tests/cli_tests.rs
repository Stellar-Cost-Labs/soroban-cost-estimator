use std::process::Command;

/// Helper to run the CLI binary and capture stdout/stderr/exit code.
fn run_cli(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args(args)
        .output()
        .expect("failed to run CLI");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

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
fn test_estimate_help() {
    let (stdout, stderr, code) = run_cli(&["estimate", "--help"]);
    assert_eq!(code, 0, "estimate --help should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("--wasm"),
        "estimate help should mention --wasm"
    );
    assert!(
        stdout.contains("--network"),
        "estimate help should mention --network"
    );
    assert!(stdout.contains("--fn"), "estimate help should mention --fn");
    assert!(
        stdout.contains("--json"),
        "estimate help should mention --json"
    );
}

#[test]
fn test_estimate_all_help() {
    let (stdout, stderr, code) = run_cli(&["estimate-all", "--help"]);
    assert_eq!(
        code, 0,
        "estimate-all --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("--wasm"),
        "estimate-all help should mention --wasm"
    );
    assert!(
        stdout.contains("--network"),
        "estimate-all help should mention --network"
    );
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
    assert!(
        stdout.contains("--network"),
        "snapshot help should mention --network"
    );
    assert!(
        stdout.contains("--out"),
        "snapshot help should mention --out"
    );
}

#[test]
fn test_config_diff_help() {
    let (stdout, stderr, code) = run_cli(&["config", "diff", "--help"]);
    assert_eq!(
        code, 0,
        "config diff --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("--network"),
        "diff help should mention --network"
    );
    assert!(
        stdout.contains("--against"),
        "diff help should mention --against"
    );
}

#[test]
fn test_watch_help() {
    let (stdout, stderr, code) = run_cli(&["watch", "--help"]);
    assert_eq!(code, 0, "watch --help should exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("--network"),
        "watch help should mention --network"
    );
    assert!(
        stdout.contains("--interval"),
        "watch help should mention --interval"
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

/// The fixture's only function needs an argument, so `estimate-all --json`
/// must still print a valid report object and exit non-zero.
#[test]
fn test_estimate_all_json_outputs_object_and_exits_nonzero_on_failure() {
    let (stdout, stderr, code) = run_cli(&[
        "estimate-all",
        "--wasm",
        "tests/fixtures/contract.wasm",
        "--json",
    ]);

    assert_eq!(
        code, 1,
        "any failed function must exit non-zero; stderr: {stderr}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be a valid JSON object");
    assert!(
        parsed.is_object(),
        "top-level JSON must be an object, not an array: {stdout}"
    );
    assert!(parsed["contract_wasm_hash"].is_string());
    assert_eq!(parsed["network"], "testnet");
    assert!(parsed["functions"].is_array());
    assert!(parsed["total_summary"].is_object());

    let functions = parsed["functions"].as_array().expect("functions array");
    assert_eq!(functions.len(), 1, "fixture exports one function");
    let entry = &functions[0];
    assert_eq!(entry["function_name"], "increment");
    assert_eq!(entry["status"], "failed");
    assert!(entry["error_message"].is_string());
    assert!(entry["resources"].is_null());
    assert!(entry["fee_breakdown"].is_null());

    assert_eq!(parsed["total_summary"]["total_functions"], 1);
    assert_eq!(parsed["total_summary"]["failed"], 1);
    assert_eq!(parsed["total_summary"]["successful"], 0);
}
