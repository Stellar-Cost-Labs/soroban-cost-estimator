use soroban_cost_estimator::config_snapshot::history::{
    build_change_log_from_snapshots, last_changed_from_log,
};
use soroban_cost_estimator::config_snapshot::model::*;

fn make_snapshot(
    timestamp: &str,
    ledger: u32,
    compute_fee: i64,
    bandwidth_fee: i64,
) -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: timestamp.to_string(),
        ledger,
        protocol_version: None,
        contract_compute: Some(ContractComputeV0 {
            ledger_max_instructions: 1_000_000,
            tx_max_instructions: 100_000,
            fee_rate_per_instructions_increment: compute_fee,
            tx_memory_limit: 100,
        }),
        contract_ledger_cost: None,
        contract_historical_data: None,
        contract_events: None,
        contract_bandwidth: Some(ContractBandwidthV0 {
            ledger_max_txs_size_bytes: 1_000_000,
            tx_max_size_bytes: 100_000,
            fee_tx_size1_kb: bandwidth_fee,
        }),
        state_archival: None,
    }
}

#[test]
fn test_empty_with_fewer_than_two_snapshots() {
    let snap = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    assert!(build_change_log_from_snapshots(&[]).is_empty());
    assert!(build_change_log_from_snapshots(&[snap]).is_empty());
}

#[test]
fn test_no_changes_across_identical_snapshots() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 100, 5);
    let log = build_change_log_from_snapshots(&[a, b]);
    assert!(log.is_empty());
}

#[test]
fn test_records_change_with_newer_snapshot_stamp() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let log = build_change_log_from_snapshots(&[a, b]);

    assert_eq!(log.len(), 1);
    assert_eq!(
        log[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(log[0].timestamp, "2026-01-02T00:00:00Z");
    assert_eq!(log[0].ledger, 200);
    assert_eq!(log[0].old_value, "100");
    assert_eq!(log[0].new_value, "200");
    assert!(log[0].is_pricing_change);
}

#[test]
fn test_sorts_out_of_order_input_snapshots_by_timestamp() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    // Passed in reverse order — the function must sort before diffing.
    let log = build_change_log_from_snapshots(&[b, a]);
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].old_value, "100");
    assert_eq!(log[0].new_value, "200");
}

#[test]
fn test_multiple_transitions_produce_chronological_entries() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 200, 10);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    assert_eq!(log.len(), 2);
    assert_eq!(log[0].timestamp, "2026-01-02T00:00:00Z");
    assert_eq!(
        log[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(log[1].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(log[1].field_path, "contract_bandwidth.fee_tx_size1_kb");
}

#[test]
fn test_last_changed_keeps_most_recent_entry_per_field() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 300, 5);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    // Two changes to the same field across the log; last_changed keeps only
    // the most recent one.
    let last = last_changed_from_log(&log);
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(last[0].old_value, "200");
    assert_eq!(last[0].new_value, "300");
}

#[test]
fn test_last_changed_tracks_independent_fields_separately() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 200, 10);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    let mut last = last_changed_from_log(&log);
    last.sort_by(|x, y| x.field_path.cmp(&y.field_path));
    assert_eq!(last.len(), 2);
    assert_eq!(last[0].field_path, "contract_bandwidth.fee_tx_size1_kb");
    assert_eq!(last[0].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(
        last[1].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(last[1].timestamp, "2026-01-02T00:00:00Z");
}

#[test]
fn test_atomic_snapshot_write_preserves_previous_file_on_write_failure() {
    use soroban_cost_estimator::config_snapshot::store::{
        load_snapshot_from_path, save_snapshot, write_snapshot_atomically_with,
    };
    use std::io::Write;

    let dir = std::env::temp_dir().join(format!("sce-atomic-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let dest = dir.join("testnet-snapshot.json");

    let initial = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    save_snapshot(&initial, Some(&dest.to_string_lossy())).expect("save initial snapshot");

    // Simulate a partial write followed by an I/O error before flush/rename
    let err = write_snapshot_atomically_with(&dest, b"{\"truncated\":", |file| {
        file.write_all(b"{\"truncated\":")?;
        Err(std::io::Error::new(
            std::io::ErrorKind::WriteZero,
            "simulated disk full mid-snapshot",
        ))
    });
    assert!(err.is_err(), "simulated write error must fail");

    // Previous snapshot must still be intact and readable
    let preserved = load_snapshot_from_path(&dest.to_string_lossy())
        .expect("previous snapshot must remain valid");
    assert_eq!(preserved.ledger, 100);

    // No temporary files should remain in the directory
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["testnet-snapshot.json".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_atomic_snapshot_write_cleans_up_temp_file_on_rename_failure() {
    use soroban_cost_estimator::config_snapshot::store::write_snapshot_atomically;

    let dir = std::env::temp_dir().join(format!("sce-atomic-rename-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");

    // Create a directory at `dest` so renaming a regular file over `dest` fails
    let dest_dir = dir.join("unrenamable-dir.json");
    std::fs::create_dir_all(&dest_dir).expect("create conflicting directory");

    let err = write_snapshot_atomically(&dest_dir, b"{}");
    assert!(err.is_err(), "renaming over a directory must fail");

    let leftover_temps: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftover_temps.is_empty(),
        "temporary files must be cleaned up after rename failure: {leftover_temps:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
