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
mod store_ordering_tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    use soroban_cost_estimator::config_snapshot::store::{
        list_snapshots_in, load_last_two_snapshots_in, load_latest_snapshot_in,
        parse_filename_timestamp,
    };
    use soroban_cost_estimator::error::AppError;

    use super::*;

    static TEST_DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        let id = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("sce-ordering-{label}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp snapshot dir");
        dir
    }

    fn write_named_snapshot(
        dir: &Path,
        network: &str,
        timestamp: &str,
        ledger: u32,
        compute_fee: i64,
    ) -> PathBuf {
        let mut snap = make_snapshot(timestamp, ledger, compute_fee, 5);
        snap.network = network.to_string();
        let filename = format!("{}-{}.json", network, timestamp.replace(':', "-"));
        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(&snap).expect("serialize snapshot");
        std::fs::write(&path, json).expect("write snapshot file");
        path
    }

    #[test]
    fn test_orders_snapshots_across_hour_day_and_year_boundaries() {
        let dir = temp_snapshot_dir("boundaries");
        // Write out of chronological order across year, month, day, and hour boundaries.
        let p_2026_day2_h01 =
            write_named_snapshot(&dir, "testnet", "2026-01-02T01:05:00Z", 500, 50);
        let p_2025_nye_late =
            write_named_snapshot(&dir, "testnet", "2025-12-31T23:59:59Z", 100, 10);
        let p_2026_day1_h23 =
            write_named_snapshot(&dir, "testnet", "2026-01-01T23:59:59Z", 400, 40);
        let p_2026_nyd_start =
            write_named_snapshot(&dir, "testnet", "2026-01-01T00:00:00Z", 200, 20);
        let p_2026_day1_h01 =
            write_named_snapshot(&dir, "testnet", "2026-01-01T01:00:00Z", 300, 30);

        let listed = list_snapshots_in(&dir, "testnet").expect("list snapshots");
        assert_eq!(
            listed,
            vec![
                p_2025_nye_late,
                p_2026_nyd_start,
                p_2026_day1_h01,
                p_2026_day1_h23,
                p_2026_day2_h01,
            ]
        );

        let latest = load_latest_snapshot_in(&dir, "testnet").expect("load latest");
        assert_eq!(latest.ledger, 500);
        assert_eq!(latest.timestamp, "2026-01-02T01:05:00Z");

        let (prev, curr) =
            load_last_two_snapshots_in(&dir, "testnet").expect("load last two snapshots");
        assert_eq!(prev.ledger, 400);
        assert_eq!(curr.ledger, 500);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_orders_snapshots_chronologically_across_timezone_offsets() {
        let dir = temp_snapshot_dir("offsets");
        // 2026-01-01T02:00:00+03:00 == 2025-12-31T23:00:00Z (oldest)
        // 2026-01-01T00:00:00Z       == 2026-01-01T00:00:00Z (middle)
        // 2025-12-31T20:00:00-05:00 == 2026-01-01T01:00:00Z (newest)
        let p_middle = write_named_snapshot(&dir, "testnet", "2026-01-01T00:00:00Z", 200, 20);
        let p_oldest = write_named_snapshot(&dir, "testnet", "2026-01-01T02:00:00+03:00", 100, 10);
        let p_newest = write_named_snapshot(&dir, "testnet", "2025-12-31T20:00:00-05:00", 300, 30);

        let listed = list_snapshots_in(&dir, "testnet").expect("list snapshots across offsets");
        assert_eq!(listed, vec![p_oldest, p_middle, p_newest]);

        let latest = load_latest_snapshot_in(&dir, "testnet").expect("load latest across offsets");
        assert_eq!(latest.ledger, 300);
        assert_eq!(latest.timestamp, "2025-12-31T20:00:00-05:00");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ignores_matching_filenames_with_invalid_embedded_timestamp_and_rejects_corrupt_json_timestamp()
     {
        let dir = temp_snapshot_dir("invalid-timestamps");
        let valid_old = write_named_snapshot(&dir, "testnet", "2026-01-01T00:00:00Z", 100, 10);
        let valid_new = write_named_snapshot(&dir, "testnet", "2026-01-02T00:00:00Z", 200, 20);

        // Matching prefix/suffix, but embedded filename timestamps are invalid.
        // Note that 'testnet-zzzz-invalid.json' would sort after 'testnet-2026-...' lexicographically.
        let bad_name_1 = dir.join("testnet-zzzz-invalid.json");
        std::fs::write(&bad_name_1, "{}").expect("write bad_name_1");
        let bad_name_2 = dir.join("testnet-2026-13-45T99-99-99Z.json");
        std::fs::write(&bad_name_2, "{}").expect("write bad_name_2");

        assert!(parse_filename_timestamp("testnet-zzzz-invalid.json", "testnet").is_none());
        assert!(parse_filename_timestamp("testnet-2026-13-45T99-99-99Z.json", "testnet").is_none());

        let listed = list_snapshots_in(&dir, "testnet").expect("list snapshots");
        assert_eq!(listed, vec![valid_old, valid_new]);

        let latest = load_latest_snapshot_in(&dir, "testnet").expect("load latest valid snapshot");
        assert_eq!(latest.ledger, 200);

        // Now write a file with a newer valid filename timestamp, but an invalid timestamp inside its JSON body.
        let bad_body_path = dir.join("testnet-2026-01-03T00-00-00Z.json");
        let mut bad_body_snap = make_snapshot("not-a-valid-rfc3339-timestamp", 300, 30, 5);
        bad_body_snap.network = "testnet".to_string();
        std::fs::write(
            &bad_body_path,
            serde_json::to_string_pretty(&bad_body_snap).expect("serialize"),
        )
        .expect("write bad body snapshot");

        let err = load_latest_snapshot_in(&dir, "testnet")
            .expect_err("corrupt embedded JSON timestamp must fail latest load");
        assert!(
            matches!(err, AppError::SnapshotParse(_)),
            "unexpected error: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ignores_unrelated_files_and_other_network_prefixes() {
        let dir = temp_snapshot_dir("unrelated-and-networks");
        let testnet_1 = write_named_snapshot(&dir, "testnet", "2026-01-01T00:00:00Z", 100, 10);
        let testnet_2 = write_named_snapshot(&dir, "testnet", "2026-01-02T00:00:00Z", 200, 20);
        let _mainnet = write_named_snapshot(&dir, "mainnet", "2026-01-03T00:00:00Z", 999, 99);
        let _futurenet =
            write_named_snapshot(&dir, "testnet-staging", "2026-01-04T00:00:00Z", 888, 88);

        // Unrelated files and subdirectories
        std::fs::write(dir.join("README.md"), "# notes").expect("write readme");
        std::fs::write(dir.join("testnet-2026-01-05T00-00-00Z.bak"), "{}").expect("write bak");
        std::fs::write(dir.join("bundle.json"), "{}").expect("write bundle");
        std::fs::create_dir_all(dir.join("testnet-2026-01-06T00-00-00Z.json"))
            .expect("create dir with .json suffix");

        let listed = list_snapshots_in(&dir, "testnet").expect("list testnet snapshots");
        assert_eq!(listed, vec![testnet_1, testnet_2]);

        let latest = load_latest_snapshot_in(&dir, "testnet").expect("load latest testnet");
        assert_eq!(latest.network, "testnet");
        assert_eq!(latest.ledger, 200);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
