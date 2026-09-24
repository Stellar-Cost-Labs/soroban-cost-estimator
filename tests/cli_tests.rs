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
fn test_config_cache_import_help() {
    let (stdout, stderr, code) = run_cli(&["config", "cache", "import", "--help"]);
    assert_eq!(
        code, 0,
        "config cache import --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("--overwrite"),
        "import help should mention --overwrite"
    );
    assert!(
        stdout.contains("--merge"),
        "import help should mention --merge"
    );
}

#[test]
fn test_config_cache_help_lists_import() {
    let (stdout, stderr, code) = run_cli(&["config", "cache", "--help"]);
    assert_eq!(
        code, 0,
        "config cache --help should exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("import"),
        "config cache help should list import"
    );
}

#[test]
fn test_cache_import_prints_summary() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp =
        std::env::temp_dir().join(format!("sce_cli_import_{}_{}", std::process::id(), suffix));
    std::fs::create_dir_all(&tmp).expect("create temp home");

    let export_path = tmp.join("export.json");
    let export = serde_json::json!({
        "schema_version": 1,
        "exported_at": "2026-01-01T00:00:00+00:00",
        "entries": [{
            "wasm_hash": "abc123",
            "function": "cli_import_fn",
            "args_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "network": "testnet",
            "ledger": 42,
            "total_stroops": 100,
            "cpu_instructions": 10,
            "memory_bytes": 5,
            "timestamp": "2026-01-01T00:00:00+00:00"
        }]
    });
    std::fs::write(
        &export_path,
        serde_json::to_string_pretty(&export).expect("ser"),
    )
    .expect("write export");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "config",
            "cache",
            "import",
            export_path.to_str().expect("utf8 path"),
        ])
        .env("HOME", &tmp)
        .output()
        .expect("run CLI import");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "import should exit 0; stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        "Imported 1 new entries, skipped 0 existing entries",
        "summary line should match the acceptance criteria exactly"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_cache_import_corrupted_file_exits_nonzero() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "sce_cli_import_bad_{}_{}",
        std::process::id(),
        suffix
    ));
    std::fs::create_dir_all(&tmp).expect("create temp home");

    let export_path = tmp.join("corrupt.json");
    std::fs::write(&export_path, "not json at all").expect("write corrupt");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
        .args([
            "config",
            "cache",
            "import",
            export_path.to_str().expect("utf8 path"),
        ])
        .env("HOME", &tmp)
        .output()
        .expect("run CLI import");

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_ne!(code, 0, "corrupt import should exit non-zero");
    assert!(
        stderr.contains("corrupted"),
        "stderr should carry the informative error: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
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
