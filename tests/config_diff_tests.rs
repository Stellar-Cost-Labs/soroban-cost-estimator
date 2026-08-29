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
fn test_format_diff_summary_counts_mixed() {
    let old = make_snapshot(100, 5);
    let mut new = make_snapshot(200, 10); // two pricing changes
    if let Some(compute) = &mut new.contract_compute {
        compute.ledger_max_instructions = 2_000_000; // non-pricing change
    }
    let diff = diff::diff_snapshots(&old, &new);
    assert_eq!(
        diff::format_diff_summary(&diff),
        "2 pricing changes, 1 non-pricing changes"
    );
}

#[test]
fn test_format_diff_summary_no_changes() {
    let snap = make_snapshot(100, 5);
    let diff = diff::diff_snapshots(&snap, &snap);
    assert_eq!(
        diff::format_diff_summary(&diff),
        "0 pricing changes, 0 non-pricing changes"
    );
}

#[test]
fn test_format_diff_with_changes() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&diff);
    // Should use human-readable setting and field names
    assert!(output.contains("Contract Compute V0"));
    assert!(output.contains("Fee Rate Per Instructions Increment"));
    assert!(output.contains("100"));
    assert!(output.contains("200"));
}
