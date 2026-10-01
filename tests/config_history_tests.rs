use soroban_cost_estimator::config_snapshot::history::{
    build_change_log_from_snapshots, filter_change_log, last_changed_from_log,
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
fn test_timeline_entries_carry_delta_percent() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 150, 10);
    let log = build_change_log_from_snapshots(&[a, b]);

    let compute = log
        .iter()
        .find(|e| e.field_path == "contract_compute.fee_rate_per_instructions_increment")
        .unwrap_or_else(|| panic!("compute fee entry missing"));
    let bandwidth = log
        .iter()
        .find(|e| e.field_path == "contract_bandwidth.fee_tx_size1_kb")
        .unwrap_or_else(|| panic!("bandwidth fee entry missing"));

    // 100 → 150 is +50%; 5 → 10 is +100%.
    let compute_delta = compute.delta_percent.unwrap_or_default();
    assert!(
        (compute_delta - 50.0).abs() < 1e-9,
        "compute delta: {compute_delta}"
    );
    let bandwidth_delta = bandwidth.delta_percent.unwrap_or_default();
    assert!(
        (bandwidth_delta - 100.0).abs() < 1e-9,
        "bandwidth delta: {bandwidth_delta}"
    );
}

#[test]
fn test_filter_by_setting_full_path_and_fragment() {
    let log = build_change_log_from_snapshots(&[
        make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5),
        make_snapshot("2026-01-02T00:00:00Z", 200, 200, 10),
        make_snapshot("2026-01-03T00:00:00Z", 300, 250, 20),
    ]);
    assert_eq!(log.len(), 4, "two settings × two transitions");

    // Full field path selects only that setting's history.
    let by_path = filter_change_log(
        &log,
        Some("contract_compute.fee_rate_per_instructions_increment"),
    );
    assert_eq!(by_path.len(), 2);
    assert!(
        by_path
            .iter()
            .all(|e| e.field_path == "contract_compute.fee_rate_per_instructions_increment")
    );

    // A bare field-name fragment works too.
    let by_fragment = filter_change_log(&log, Some("fee_tx_size1_kb"));
    assert_eq!(by_fragment.len(), 2);
    assert!(
        by_fragment
            .iter()
            .all(|e| e.field_path.ends_with("fee_tx_size1_kb"))
    );

    // A blank or absent filter keeps the entire timeline.
    assert_eq!(filter_change_log(&log, None).len(), 4);
    assert_eq!(filter_change_log(&log, Some(" ")).len(), 4);

    // An unknown setting yields an empty timeline.
    assert!(filter_change_log(&log, Some("no_such_setting")).is_empty());
}

#[test]
fn test_chronological_ordering_is_deterministic() {
    // Out-of-order input must produce the same timeline as sorted input.
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 300, 10);
    assert_eq!(
        build_change_log_from_snapshots(&[a.clone(), b.clone(), c.clone()]),
        build_change_log_from_snapshots(&[c, b, a]),
        "timeline must not depend on input order"
    );
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
