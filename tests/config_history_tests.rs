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

#[cfg(test)]
mod duplicate_ledger_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use soroban_cost_estimator::config_snapshot::store::{
        DuplicateLedgerMatch, find_same_ledger_snapshot_in, is_same_ledger_duplicate,
        save_snapshot_with_duplicate_policy_in, snapshots_have_same_config,
    };

    use super::*;

    static DUP_DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dup_dir(label: &str) -> PathBuf {
        let id = DUP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sce-dup-ledger-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dup dir");
        dir
    }

    #[test]
    fn test_detects_and_skips_or_preserves_same_ledger_duplicate_snapshots() {
        let dir = temp_dup_dir("same-ledger-identical");
        let first = make_snapshot("2026-01-01T00:00:00Z", 500, 100, 5);
        let second_same_values = make_snapshot("2026-01-01T00:05:00Z", 500, 100, 5);

        assert!(snapshots_have_same_config(&first, &second_same_values));
        assert!(is_same_ledger_duplicate(&first, &second_same_values));

        let r1 = save_snapshot_with_duplicate_policy_in(&dir, &first, None, true)
            .expect("save initial snapshot");
        assert!(r1.saved);
        assert!(!r1.duplicate_ledger);
        assert!(r1.message.is_none());

        // With skip_duplicate_ledger = true, the second capture at the same ledger with identical values is skipped
        let r2 = save_snapshot_with_duplicate_policy_in(&dir, &second_same_values, None, true)
            .expect("skip duplicate snapshot");
        assert!(!r2.saved);
        assert!(r2.duplicate_ledger);
        assert!(!r2.config_changed_at_same_ledger);
        assert_eq!(r2.existing_path.as_deref(), Some(r1.path.as_path()));
        assert!(
            r2.message
                .as_deref()
                .is_some_and(|m| m.contains("Skipped duplicate snapshot"))
        );

        // Verify only 1 file exists on disk
        let count_after_skip = std::fs::read_dir(&dir).expect("read dir").count();
        assert_eq!(count_after_skip, 1);

        // With skip_duplicate_ledger = false, the duplicate is preserved on disk and reported clearly
        let r3 = save_snapshot_with_duplicate_policy_in(&dir, &second_same_values, None, false)
            .expect("preserve duplicate snapshot with warning");
        assert!(r3.saved);
        assert!(r3.duplicate_ledger);
        assert!(!r3.config_changed_at_same_ledger);
        assert!(
            r3.message
                .as_deref()
                .is_some_and(|m| m.contains("Duplicate snapshot captured"))
        );

        let count_after_preserve = std::fs::read_dir(&dir).expect("read dir").count();
        assert_eq!(count_after_preserve, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_preserves_same_ledger_snapshot_when_configuration_values_change() {
        let dir = temp_dup_dir("same-ledger-changed");
        let first = make_snapshot("2026-01-01T00:00:00Z", 777, 100, 5);
        // Same network and same ledger 777, but compute fee changed from 100 to 250
        let changed_at_same_ledger = make_snapshot("2026-01-01T00:01:00Z", 777, 250, 5);

        assert!(!snapshots_have_same_config(&first, &changed_at_same_ledger));
        assert!(!is_same_ledger_duplicate(&first, &changed_at_same_ledger));

        save_snapshot_with_duplicate_policy_in(&dir, &first, None, true)
            .expect("save initial snapshot");

        let matched = find_same_ledger_snapshot_in(&dir, &changed_at_same_ledger)
            .expect("check same ledger match");
        assert!(matches!(
            matched,
            Some(DuplicateLedgerMatch::SameLedgerChangedConfig { .. })
        ));

        // Even with skip_duplicate_ledger = true, changed configuration at the same ledger is preserved
        let report =
            save_snapshot_with_duplicate_policy_in(&dir, &changed_at_same_ledger, None, true)
                .expect("save changed snapshot at same ledger");
        assert!(report.saved);
        assert!(!report.duplicate_ledger);
        assert!(report.config_changed_at_same_ledger);
        assert!(
            report
                .message
                .as_deref()
                .is_some_and(|m| m.contains("configuration values changed"))
        );

        let file_count = std::fs::read_dir(&dir).expect("read dir").count();
        assert_eq!(file_count, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
