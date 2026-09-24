use std::fs::OpenOptions;
use std::io::Write;

fn main() {
    let mut file = OpenOptions::new().append(true).open("tests/cli_tests.rs").unwrap();
    let content = r#"
#[test]
fn test_config_diff_watch_once_no_changes() {
    let home = temp_home("diff-watch-once-no-changes");
    let snapshot = snapshot_json("testnet", 1000);
    
    let local_path = home.join("snapshot.json");
    std::fs::write(&local_path, &snapshot).unwrap();

    let remote_path = home.join("remote.json");
    std::fs::write(&remote_path, &snapshot).unwrap();

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--watch",
            "--network",
            "testnet",
            "--against",
            local_path.to_str().unwrap(),
            "--rpc-url",
            &format!("file://{}", remote_path.display()),
        ],
        Some(&home),
    );

    assert_eq!(code, 0, "no changes should exit 0; stderr: {stderr}");
    assert!(stdout.contains("No config changes detected"), "should print no diff message; got {stdout}");
}

#[test]
fn test_config_diff_watch_once_with_changes() {
    let home = temp_home("diff-watch-once-with-changes");
    let local_snap = snapshot_json("testnet", 1000);
    let remote_snap = r#"{
  "network": "testnet",
  "ledger": 1001,
  "timestamp": "2026-01-01T00:00:00+00:00",
  "contract_compute": {
    "ledger_max_instructions": 1000000,
    "tx_max_instructions": 100000,
    "fee_rate_per_instructions_increment": 500,
    "tx_memory_limit": 41943040
  },
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}"#;
    
    let local_path = home.join("snapshot.json");
    std::fs::write(&local_path, &local_snap).unwrap();

    let remote_path = home.join("remote.json");
    std::fs::write(&remote_path, remote_snap).unwrap();

    let (stdout, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--watch",
            "--network",
            "testnet",
            "--against",
            local_path.to_str().unwrap(),
            "--rpc-url",
            &format!("file://{}", remote_path.display()),
        ],
        Some(&home),
    );

    assert_eq!(code, 1, "changes should exit 1; stderr: {stderr}");
    assert!(!stdout.contains("No config changes detected"), "should NOT print no diff message");
    assert!(stdout.contains("Contract Compute V0"), "should print diff format; got {stdout}");
}

#[test]
fn test_config_diff_watch_once_does_not_loop() {
    let home = temp_home("diff-watch-once-no-loop");
    let snapshot = snapshot_json("testnet", 1000);
    let local_path = home.join("snapshot.json");
    std::fs::write(&local_path, &snapshot).unwrap();

    let start = std::time::Instant::now();
    let (_, stderr, code) = run_cli_in_home(
        &[
            "config",
            "diff",
            "--watch",
            "--network",
            "testnet",
            "--against",
            local_path.to_str().unwrap(),
            "--rpc-url",
            "http://127.0.0.1:1", 
        ],
        Some(&home),
    );
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 5, "one-shot mode should not block/loop");
    assert_eq!(code, 1, "should exit with error code");
    assert!(stderr.contains("failed to locate RPC endpoint") || stderr.contains("error sending request") || stderr.contains("failed to fetch"), "should report network error; got: {stderr}");
}
"#;
    file.write_all(content.as_bytes()).unwrap();
}
