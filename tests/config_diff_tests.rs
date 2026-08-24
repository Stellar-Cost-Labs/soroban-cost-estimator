use soroban_cost_estimator::config_snapshot::diff;
use soroban_cost_estimator::config_snapshot::model::*;

fn make_snapshot(compute_fee: i64, bandwidth_fee: i64) -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        ledger: 100,
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
fn test_no_changes() {
    let snap = make_snapshot(100, 5);
    let diff = diff::diff_snapshots(&snap, &snap);
    assert!(diff.changes.is_empty());
    assert!(!diff.has_pricing_changes);
}

#[test]
fn test_detects_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 1);
    assert_eq!(
        diff.changes[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(diff.changes[0].old_value, "100");
    assert_eq!(diff.changes[0].new_value, "200");
    assert!(diff.changes[0].is_pricing_change);
    assert!(diff.has_pricing_changes);
}

#[test]
fn test_detects_bandwidth_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(100, 10);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 1);
    assert_eq!(
        diff.changes[0].field_path,
        "contract_bandwidth.fee_tx_size1_kb"
    );
    assert!(diff.changes[0].is_pricing_change);
}

#[test]
fn test_detects_multiple_changes() {
    let old = make_snapshot(100, 5);
    let mut new = make_snapshot(200, 10);
    new.ledger = 200;
    let diff = diff::diff_snapshots(&old, &new);

    // fee_rate_per_instructions_increment and fee_tx_size1_kb changed
    assert_eq!(diff.changes.len(), 2);
    assert!(diff.has_pricing_changes);
}

#[test]
fn test_format_diff_no_changes() {
    let snap = make_snapshot(100, 5);
    let diff = diff::diff_snapshots(&snap, &snap);
    let output = diff::format_diff(&diff);
    assert!(output.contains("No changes detected"));
}

#[test]
fn test_format_diff_with_changes() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&diff);
    assert!(output.contains("fee_rate_per_instructions_increment"));
    assert!(output.contains("100"));
    assert!(output.contains("200"));
}

#[test]
fn test_percent_change_computed_for_numeric_fields() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 1);
    assert!((diff.changes[0].percent_change.unwrap() - 100.0).abs() < f64::EPSILON);
}

#[test]
fn test_has_significant_pricing_changes_zero_threshold_matches_all() {
    // +10% change: significant under any threshold up to 10.
    let old = make_snapshot(100, 5);
    let new = make_snapshot(110, 5);
    let diff = diff::diff_snapshots(&old, &new);

    assert!(diff.has_pricing_changes);
    assert!(diff.has_significant_pricing_changes(0.0));
    assert!(diff.has_significant_pricing_changes(10.0));
    assert!(!diff.has_significant_pricing_changes(10.1));
}

#[test]
fn test_threshold_ignores_small_changes_but_flags_large_ones() {
    let old = make_snapshot(1000, 5);
    // compute fee +2% (below a 10% threshold), bandwidth fee +40% (above).
    let new = make_snapshot(1020, 7);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 2);
    assert!(diff.has_significant_pricing_changes(10.0));

    // Without the bandwidth change, only the small compute bump remains.
    let only_small = diff::diff_snapshots(&make_snapshot(1000, 7), &make_snapshot(1020, 7));
    assert!(only_small.has_pricing_changes);
    assert!(!only_small.has_significant_pricing_changes(10.0));
}

#[test]
fn test_negative_changes_meet_threshold_by_magnitude() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(50, 5);
    let diff = diff::diff_snapshots(&old, &new);

    assert!((diff.changes[0].percent_change.unwrap() - (-50.0)).abs() < f64::EPSILON);
    assert!(diff.has_significant_pricing_changes(25.0));
    assert!(!diff.has_significant_pricing_changes(60.0));
}

#[test]
fn test_format_with_threshold_annotates_below_threshold() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(105, 5);
    let diff = diff::diff_snapshots(&old, &new);

    let output = diff::format_diff_with_threshold(&diff, 10.0);
    assert!(output.contains("below 10.0% notification threshold"));
    // The stale-estimates warning is suppressed for below-threshold changes.
    assert!(!output.contains("Pricing changes detected"));

    // Default formatter (0% threshold) still warns.
    let default_output = diff::format_diff(&diff);
    assert!(default_output.contains("Pricing changes detected"));
}
