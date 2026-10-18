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
mod skip_unchanged_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use soroban_cost_estimator::config_snapshot::store::{
        SNAPSHOT_EXIT_CODE_OK, SnapshotSaveOutcome, save_snapshot_if_changed_in,
    };

    use super::*;

    static SKIP_DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_skip_dir(label: &str) -> PathBuf {
        let id = SKIP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "sce-skip-unchanged-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp skip dir");
        dir
    }

    #[test]
    fn test_skip_unchanged_with_equal_values_different_timestamps_and_real_pricing_change() {
        let dir = temp_skip_dir("equal-and-changed");

        let mut initial = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
        initial.protocol_version = Some(22);

        let first_outcome =
            save_snapshot_if_changed_in(&dir, &initial, None, true).expect("save initial snapshot");
        assert!(first_outcome.is_saved());
        assert_eq!(first_outcome.exit_code(), SNAPSHOT_EXIT_CODE_OK);
        let initial_path = first_outcome.path().to_path_buf();

        // Equal config values and protocol metadata, but later timestamp and ledger
        let mut equal_later = make_snapshot("2026-01-02T12:30:00Z", 250, 100, 5);
        equal_later.protocol_version = Some(22);
        assert!(initial.has_same_normalized_config(&equal_later));

        let skipped_outcome = save_snapshot_if_changed_in(&dir, &equal_later, None, true)
            .expect("skip unchanged snapshot");
        assert!(!skipped_outcome.is_saved());
        assert_eq!(skipped_outcome.exit_code(), SNAPSHOT_EXIT_CODE_OK);
        assert_eq!(
            skipped_outcome,
            SnapshotSaveOutcome::SkippedUnchanged {
                latest_path: initial_path.clone(),
                latest_timestamp: "2026-01-01T00:00:00Z".to_string(),
                latest_ledger: 100,
            }
        );
        assert_eq!(std::fs::read_dir(&dir).expect("read dir").count(), 1);

        // Real pricing change writes a new snapshot even with skip_unchanged = true
        let mut pricing_changed = make_snapshot("2026-01-03T00:00:00Z", 400, 250, 5);
        pricing_changed.protocol_version = Some(22);
        assert!(!equal_later.has_same_normalized_config(&pricing_changed));

        let saved_pricing = save_snapshot_if_changed_in(&dir, &pricing_changed, None, true)
            .expect("save snapshot on pricing change");
        assert!(saved_pricing.is_saved());
        assert_eq!(std::fs::read_dir(&dir).expect("read dir").count(), 2);

        // Protocol version metadata change also writes a new snapshot
        let mut protocol_changed = make_snapshot("2026-01-04T00:00:00Z", 500, 250, 5);
        protocol_changed.protocol_version = Some(23);
        assert!(!pricing_changed.has_same_normalized_config(&protocol_changed));

        let saved_protocol = save_snapshot_if_changed_in(&dir, &protocol_changed, None, true)
            .expect("save snapshot on protocol version change");
        assert!(saved_protocol.is_saved());
        assert_eq!(std::fs::read_dir(&dir).expect("read dir").count(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
