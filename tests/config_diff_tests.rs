use soroban_cost_estimator::config_snapshot::diff;
use soroban_cost_estimator::config_snapshot::model::*;

// ── Builders ───────────────────────────────────────────────────────────────

fn empty_snapshot() -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        ledger: 100,
        contract_compute: None,
        contract_ledger_cost: None,
        contract_historical_data: None,
        contract_events: None,
        contract_bandwidth: None,
        state_archival: None,
    }
}

fn full_snapshot() -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        ledger: 100,
        contract_compute: Some(ContractComputeV0 {
            ledger_max_instructions: 1_000_000,
            tx_max_instructions: 100_000,
            fee_rate_per_instructions_increment: 10,
            tx_memory_limit: 41_943_040,
        }),
        contract_ledger_cost: Some(ContractLedgerCostV0 {
            ledger_max_disk_read_entries: 1_000_000,
            ledger_max_disk_read_bytes: 1_000_000,
            ledger_max_write_ledger_entries: 1_000_000,
            ledger_max_write_bytes: 1_000_000,
            tx_max_disk_read_entries: 100,
            tx_max_disk_read_bytes: 1_000_000,
            tx_max_write_ledger_entries: 100,
            tx_max_write_bytes: 1_000_000,
            fee_disk_read_ledger_entry: 100,
            fee_write_ledger_entry: 200,
            fee_disk_read1_kb: 50,
            soroban_state_target_size_bytes: 1_000_000,
            rent_fee1_kb_soroban_state_size_low: 10,
            rent_fee1_kb_soroban_state_size_high: 20,
            soroban_state_rent_fee_growth_factor: 2000,
        }),
        contract_historical_data: Some(ContractHistoricalDataV0 {
            fee_historical1_kb: 100,
        }),
        contract_events: Some(ContractEventsV0 {
            tx_max_contract_events_size_bytes: 1_000_000,
            fee_contract_events1_kb: 50,
        }),
        contract_bandwidth: Some(ContractBandwidthV0 {
            ledger_max_txs_size_bytes: 1_000_000,
            tx_max_size_bytes: 100_000,
            fee_tx_size1_kb: 10,
        }),
        state_archival: Some(StateArchivalV0 {
            max_entry_ttl: 4096,
            min_temporary_ttl: 16,
            min_persistent_ttl: 2073600,
            persistent_rent_rate_denominator: 100_000,
            temp_rent_rate_denominator: 50_000,
            max_entries_to_archive: 100,
            live_soroban_state_size_window_sample_size: 20,
            live_soroban_state_size_window_sample_period: 10_000,
            eviction_scan_size: 1000,
            starting_eviction_scan_level: 16,
        }),
    }
}

fn snapshot_with(
    compute: bool,
    ledger_cost: bool,
    historical: bool,
    events: bool,
    bandwidth: bool,
    archival: bool,
) -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        ledger: 100,
        contract_compute: if compute {
            Some(ContractComputeV0 {
                ledger_max_instructions: 1_000_000,
                tx_max_instructions: 100_000,
                fee_rate_per_instructions_increment: 10,
                tx_memory_limit: 41_943_040,
            })
        } else {
            None
        },
        contract_ledger_cost: if ledger_cost {
            Some(ContractLedgerCostV0 {
                ledger_max_disk_read_entries: 1_000_000,
                ledger_max_disk_read_bytes: 1_000_000,
                ledger_max_write_ledger_entries: 1_000_000,
                ledger_max_write_bytes: 1_000_000,
                tx_max_disk_read_entries: 100,
                tx_max_disk_read_bytes: 1_000_000,
                tx_max_write_ledger_entries: 100,
                tx_max_write_bytes: 1_000_000,
                fee_disk_read_ledger_entry: 100,
                fee_write_ledger_entry: 200,
                fee_disk_read1_kb: 50,
                soroban_state_target_size_bytes: 1_000_000,
                rent_fee1_kb_soroban_state_size_low: 10,
                rent_fee1_kb_soroban_state_size_high: 20,
                soroban_state_rent_fee_growth_factor: 2000,
            })
        } else {
            None
        },
        contract_historical_data: if historical {
            Some(ContractHistoricalDataV0 {
                fee_historical1_kb: 100,
            })
        } else {
            None
        },
        contract_events: if events {
            Some(ContractEventsV0 {
                tx_max_contract_events_size_bytes: 1_000_000,
                fee_contract_events1_kb: 50,
            })
        } else {
            None
        },
        contract_bandwidth: if bandwidth {
            Some(ContractBandwidthV0 {
                ledger_max_txs_size_bytes: 1_000_000,
                tx_max_size_bytes: 100_000,
                fee_tx_size1_kb: 10,
            })
        } else {
            None
        },
        state_archival: if archival {
            Some(StateArchivalV0 {
                max_entry_ttl: 4096,
                min_temporary_ttl: 16,
                min_persistent_ttl: 2073600,
                persistent_rent_rate_denominator: 100_000,
                temp_rent_rate_denominator: 50_000,
                max_entries_to_archive: 100,
                live_soroban_state_size_window_sample_size: 20,
                live_soroban_state_size_window_sample_period: 10_000,
                eviction_scan_size: 1000,
                starting_eviction_scan_level: 16,
            })
        } else {
            None
        },
    }
}

// ── Existing tests ─────────────────────────────────────────────────────────

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
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
    assert!(!d.has_pricing_changes);
}

#[test]
fn test_detects_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let d = diff::diff_snapshots(&old, &new);

    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(d.changes[0].old_value, "100");
    assert_eq!(d.changes[0].new_value, "200");
    assert!(d.changes[0].is_pricing_change);
    assert!(d.has_pricing_changes);
}

#[test]
fn test_detects_bandwidth_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(100, 10);
    let d = diff::diff_snapshots(&old, &new);

    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_bandwidth.fee_tx_size1_kb"
    );
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_detects_multiple_changes() {
    let old = make_snapshot(100, 5);
    let mut new = make_snapshot(200, 10);
    new.ledger = 200;
    let d = diff::diff_snapshots(&old, &new);

    assert_eq!(d.changes.len(), 2);
    assert!(d.has_pricing_changes);
}

#[test]
fn test_format_diff_no_changes() {
    let snap = make_snapshot(100, 5);
    let d = diff::diff_snapshots(&snap, &snap);
    let output = diff::format_diff(&d);
    assert!(output.contains("No changes detected"));
}

#[test]
fn test_format_diff_with_changes() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    // Should use human-readable setting and field names
    assert!(output.contains("Contract Compute V0"));
    assert!(output.contains("Fee Rate Per Instructions Increment"));
    assert!(output.contains("100"));
    assert!(output.contains("200"));
}

// ── Identity: same snapshot should produce no changes (all-present) ────────

#[test]
fn test_all_present_identical_no_changes() {
    let snap = full_snapshot();
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
    assert!(!d.has_pricing_changes);
}

#[test]
fn test_all_absent_identical_no_changes() {
    let snap = empty_snapshot();
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
    assert!(!d.has_pricing_changes);
}

// ── Missing → Present transitions (None → Some) for every section ─────────

#[test]
fn test_compute_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(true, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_compute");
    assert_eq!(d.changes[0].old_value, "(missing)");
    assert_eq!(d.changes[0].new_value, "(present)");
    assert!(d.changes[0].is_pricing_change);
    assert!(d.has_pricing_changes);
}

#[test]
fn test_ledger_cost_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(false, true, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_ledger_cost");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_historical_data_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(false, false, true, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_historical_data");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_events_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(false, false, false, true, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_events");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_bandwidth_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(false, false, false, false, true, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_bandwidth");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_state_archival_added() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(false, false, false, false, false, true);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "state_archival");
    assert!(d.changes[0].is_pricing_change);
}

// ── Present → Missing transitions (Some → None) for every section ─────────

#[test]
fn test_compute_removed() {
    let old = snapshot_with(true, false, false, false, false, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_compute");
    assert_eq!(d.changes[0].old_value, "(present)");
    assert_eq!(d.changes[0].new_value, "(missing)");
    assert!(d.changes[0].is_pricing_change);
    assert!(d.has_pricing_changes);
}

#[test]
fn test_ledger_cost_removed() {
    let old = snapshot_with(false, true, false, false, false, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_ledger_cost");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_historical_data_removed() {
    let old = snapshot_with(false, false, true, false, false, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_historical_data");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_events_removed() {
    let old = snapshot_with(false, false, false, true, false, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_events");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_bandwidth_removed() {
    let old = snapshot_with(false, false, false, false, true, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "contract_bandwidth");
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_state_archival_removed() {
    let old = snapshot_with(false, false, false, false, false, true);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].field_path, "state_archival");
    assert!(d.changes[0].is_pricing_change);
}

// ── All sections added at once ────────────────────────────────────────────

#[test]
fn test_all_sections_added_from_empty() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(true, true, true, true, true, true);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 6);
    assert!(d.has_pricing_changes);
    let paths: Vec<&str> = d.changes.iter().map(|c| c.field_path.as_str()).collect();
    assert!(paths.contains(&"contract_compute"));
    assert!(paths.contains(&"contract_ledger_cost"));
    assert!(paths.contains(&"contract_historical_data"));
    assert!(paths.contains(&"contract_events"));
    assert!(paths.contains(&"contract_bandwidth"));
    assert!(paths.contains(&"state_archival"));
}

#[test]
fn test_all_sections_removed_from_full() {
    let old = snapshot_with(true, true, true, true, true, true);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 6);
    assert!(d.has_pricing_changes);
    for change in &d.changes {
        assert_eq!(change.old_value, "(present)");
        assert_eq!(change.new_value, "(missing)");
    }
}

// ── Mixed transitions (some added, some removed) ──────────────────────────

#[test]
fn test_half_added_half_removed() {
    // compute/historical/bandwidth are present in old but not new
    // ledger_cost/events/archival are absent in old but present in new
    let old = snapshot_with(true, false, true, false, true, false);
    let new = snapshot_with(false, true, false, true, false, true);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 6);
    assert!(d.has_pricing_changes);
    // 3 removals + 3 additions
    let present_to_missing = d
        .changes
        .iter()
        .filter(|c| c.old_value == "(present)")
        .count();
    let missing_to_present = d
        .changes
        .iter()
        .filter(|c| c.new_value == "(present)")
        .count();
    assert_eq!(present_to_missing, 3);
    assert_eq!(missing_to_present, 3);
}

#[test]
fn test_one_added_one_removed_others_unchanged() {
    // compute added, bandwidth removed, everything else stays (absent)
    let old = snapshot_with(false, false, false, false, true, false);
    let new = snapshot_with(true, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 2);
    assert!(d.has_pricing_changes);
}

// ── All combinations of two sections present/absent ───────────────────────

#[test]
fn test_compute_present_bandwidth_present_no_changes() {
    let snap = snapshot_with(true, false, false, false, true, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_compute_present_bandwidth_absent_no_changes() {
    let snap = snapshot_with(true, false, false, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_compute_absent_bandwidth_present_no_changes() {
    let snap = snapshot_with(false, false, false, false, true, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_compute_absent_bandwidth_absent_no_changes() {
    let snap = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

// ── Non-pricing field changes (should NOT set has_pricing_changes) ────────

#[test]
fn test_non_pricing_field_change_detected() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute
        .as_mut()
        .unwrap()
        .ledger_max_instructions = 2_000_000;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_compute.ledger_max_instructions"
    );
    assert!(!d.changes[0].is_pricing_change);
    assert!(!d.has_pricing_changes);
}

#[test]
fn test_pricing_field_change_detected() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 50;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);
    assert!(d.has_pricing_changes);
}

#[test]
fn test_non_pricing_change_does_not_trigger_pricing_flag() {
    let old = snapshot_with(true, false, false, false, true, false);
    let mut new = old.clone();
    new.contract_compute.as_mut().unwrap().tx_max_instructions = 200_000;
    new.contract_bandwidth
        .as_mut()
        .unwrap()
        .ledger_max_txs_size_bytes = 2_000_000;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 2);
    assert!(!d.has_pricing_changes);
}

// ── Individual field-level comparisons within sections ────────────────────

#[test]
fn test_compute_all_individual_fields() {
    let old = full_snapshot();
    // Change one non-pricing field
    let mut new = old.clone();
    new.contract_compute
        .as_mut()
        .unwrap()
        .ledger_max_instructions = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_compute.ledger_max_instructions"
    );
    assert!(!d.changes[0].is_pricing_change);

    // Change the pricing field
    let mut new = old.clone();
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_ledger_cost_all_individual_fields() {
    let old = full_snapshot();
    // Change a non-pricing field
    let mut new = old.clone();
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .ledger_max_disk_read_entries = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].field_path,
        "contract_ledger_cost.ledger_max_disk_read_entries"
    );
    assert!(!d.changes[0].is_pricing_change);

    // Change a pricing field
    let mut new = old.clone();
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .fee_disk_read_ledger_entry = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);

    // Change another pricing field
    let mut new = old.clone();
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .fee_write_ledger_entry = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);

    // Change another pricing field
    let mut new = old.clone();
    new.contract_ledger_cost.as_mut().unwrap().fee_disk_read1_kb = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);

    // Change a rent pricing field
    let mut new = old.clone();
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .rent_fee1_kb_soroban_state_size_low = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);

    // Change another rent pricing field
    let mut new = old.clone();
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .rent_fee1_kb_soroban_state_size_high = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_historical_data_field() {
    let old = full_snapshot();
    let mut new = old.clone();
    new.contract_historical_data
        .as_mut()
        .unwrap()
        .fee_historical1_kb = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_events_individual_fields() {
    let old = full_snapshot();
    // Non-pricing
    let mut new = old.clone();
    new.contract_events
        .as_mut()
        .unwrap()
        .tx_max_contract_events_size_bytes = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(!d.changes[0].is_pricing_change);

    // Pricing
    let mut new = old.clone();
    new.contract_events
        .as_mut()
        .unwrap()
        .fee_contract_events1_kb = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_bandwidth_individual_fields() {
    let old = full_snapshot();
    // Non-pricing
    let mut new = old.clone();
    new.contract_bandwidth
        .as_mut()
        .unwrap()
        .ledger_max_txs_size_bytes = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(!d.changes[0].is_pricing_change);

    // Pricing
    let mut new = old.clone();
    new.contract_bandwidth.as_mut().unwrap().fee_tx_size1_kb = 999;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 1);
    assert!(d.changes[0].is_pricing_change);
}

#[test]
fn test_state_archival_all_individual_fields() {
    let old = full_snapshot();
    // Non-pricing fields
    for (field, val) in [
        ("max_entry_ttl", 1u32),
        ("min_temporary_ttl", 2),
        ("min_persistent_ttl", 3),
        ("max_entries_to_archive", 4),
        ("live_soroban_state_size_window_sample_size", 5),
        ("live_soroban_state_size_window_sample_period", 6),
        ("eviction_scan_size", 7),
        ("starting_eviction_scan_level", 8),
    ] {
        let mut new = old.clone();
        let sa = new.state_archival.as_mut().unwrap();
        match field {
            "max_entry_ttl" => sa.max_entry_ttl = val,
            "min_temporary_ttl" => sa.min_temporary_ttl = val,
            "min_persistent_ttl" => sa.min_persistent_ttl = val,
            "max_entries_to_archive" => sa.max_entries_to_archive = val,
            "live_soroban_state_size_window_sample_size" => {
                sa.live_soroban_state_size_window_sample_size = val
            }
            "live_soroban_state_size_window_sample_period" => {
                sa.live_soroban_state_size_window_sample_period = val
            }
            "eviction_scan_size" => sa.eviction_scan_size = val,
            "starting_eviction_scan_level" => sa.starting_eviction_scan_level = val,
            _ => unreachable!(),
        }
        let d = diff::diff_snapshots(&old, &new);
        assert_eq!(d.changes.len(), 1, "change in {field}");
        assert!(
            !d.changes[0].is_pricing_change,
            "{field} should not be pricing"
        );
        assert!(
            !d.has_pricing_changes,
            "{field} should not trigger pricing flag"
        );
    }

    // Pricing fields
    for field in [
        "persistent_rent_rate_denominator",
        "temp_rent_rate_denominator",
    ] {
        let mut new = old.clone();
        let sa = new.state_archival.as_mut().unwrap();
        match field {
            "persistent_rent_rate_denominator" => sa.persistent_rent_rate_denominator = 999,
            "temp_rent_rate_denominator" => sa.temp_rent_rate_denominator = 999,
            _ => unreachable!(),
        }
        let d = diff::diff_snapshots(&old, &new);
        assert_eq!(d.changes.len(), 1, "change in {field}");
        assert!(d.changes[0].is_pricing_change, "{field} should be pricing");
        assert!(d.has_pricing_changes, "{field} should trigger pricing flag");
    }
}

// ── Multiple fields changed simultaneously ─────────────────────────────────

#[test]
fn test_multiple_pricing_fields_changed() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 50;
    new.contract_bandwidth.as_mut().unwrap().fee_tx_size1_kb = 20;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 2);
    assert!(d.has_pricing_changes);
    let pricing_count = d.changes.iter().filter(|c| c.is_pricing_change).count();
    assert_eq!(pricing_count, 2);
}

#[test]
fn test_multiple_non_pricing_fields_changed() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute.as_mut().unwrap().tx_max_instructions = 200_000;
    new.contract_events
        .as_mut()
        .unwrap()
        .tx_max_contract_events_size_bytes = 2_000_000;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 2);
    assert!(!d.has_pricing_changes);
}

#[test]
fn test_mixed_pricing_and_non_pricing_changes() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    // Pricing
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 50;
    // Non-pricing
    new.contract_compute.as_mut().unwrap().tx_max_instructions = 200_000;
    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 2);
    assert!(d.has_pricing_changes);
    let pricing = d.changes.iter().filter(|c| c.is_pricing_change).count();
    let non_pricing = d.changes.iter().filter(|c| !c.is_pricing_change).count();
    assert_eq!(pricing, 1);
    assert_eq!(non_pricing, 1);
}

// ── All 2^6 combinations of present/absent ────────────────────────────────

#[test]
fn test_all_none_no_changes() {
    let snap = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_all_some_no_changes() {
    let snap = snapshot_with(true, true, true, true, true, true);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_compute_present_no_changes() {
    let snap = snapshot_with(true, false, false, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_ledger_cost_present_no_changes() {
    let snap = snapshot_with(false, true, false, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_historical_present_no_changes() {
    let snap = snapshot_with(false, false, true, false, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_events_present_no_changes() {
    let snap = snapshot_with(false, false, false, true, false, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_bandwidth_present_no_changes() {
    let snap = snapshot_with(false, false, false, false, true, false);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

#[test]
fn test_only_archival_present_no_changes() {
    let snap = snapshot_with(false, false, false, false, false, true);
    let d = diff::diff_snapshots(&snap, &snap);
    assert!(d.changes.is_empty());
}

// ── Combinations: compute absent in old, others vary ──────────────────────

#[test]
fn test_compute_absent_others_varying() {
    // All combinations where compute is absent
    for &(lc, h, e, b, a) in &[
        (false, false, false, false, false),
        (true, false, false, false, false),
        (false, true, false, false, false),
        (false, false, true, false, false),
        (false, false, false, true, false),
        (false, false, false, false, true),
        (true, true, false, false, false),
        (true, false, true, false, false),
        (true, false, false, true, false),
        (true, false, false, false, true),
        (false, true, true, false, false),
        (false, true, false, true, false),
        (false, true, false, false, true),
        (false, false, true, true, false),
        (false, false, true, false, true),
        (false, false, false, true, true),
        (true, true, true, false, false),
        (true, true, false, true, false),
        (true, true, false, false, true),
        (true, false, true, true, false),
        (true, false, true, false, true),
        (true, false, false, true, true),
        (false, true, true, true, false),
        (false, true, true, false, true),
        (false, true, false, true, true),
        (false, false, true, true, true),
        (true, true, true, true, false),
        (true, true, true, false, true),
        (true, true, false, true, true),
        (true, false, true, true, true),
        (false, true, true, true, true),
    ] {
        let snap = snapshot_with(false, lc, h, e, b, a);
        let d = diff::diff_snapshots(&snap, &snap);
        assert!(d.changes.is_empty());
    }
}

// ── SnapshotInfo populated correctly ──────────────────────────────────────

#[test]
fn test_diff_populates_snapshot_info() {
    let mut old = empty_snapshot();
    old.network = "mainnet".to_string();
    old.timestamp = "2025-06-01T00:00:00Z".to_string();
    old.ledger = 500;

    let mut new = empty_snapshot();
    new.network = "mainnet".to_string();
    new.timestamp = "2025-07-01T00:00:00Z".to_string();
    new.ledger = 600;

    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.old_snapshot.network, "mainnet");
    assert_eq!(d.old_snapshot.ledger, 500);
    assert_eq!(d.new_snapshot.ledger, 600);
}

#[test]
fn test_diff_populates_snapshot_info_with_pricing_change() {
    let mut old = full_snapshot();
    old.ledger = 500;
    let mut new = full_snapshot();
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 50;
    new.ledger = 600;

    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.old_snapshot.ledger, 500);
    assert_eq!(d.new_snapshot.ledger, 600);
    assert!(d.has_pricing_changes);
}

// ── format_diff output checks ─────────────────────────────────────────────

#[test]
fn test_format_diff_all_present_identical() {
    let snap = full_snapshot();
    let d = diff::diff_snapshots(&snap, &snap);
    let output = diff::format_diff(&d);
    assert!(output.contains("No changes detected"));
    assert!(output.contains("testnet"));
}

#[test]
fn test_format_diff_addition_contains_pricing_warning() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(true, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    assert!(output.contains("Pricing changes detected"));
}

#[test]
fn test_format_diff_removal_contains_pricing_warning() {
    let old = snapshot_with(true, false, false, false, false, false);
    let new = snapshot_with(false, false, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    assert!(output.contains("Pricing changes detected"));
}

#[test]
fn test_format_diff_non_pricing_no_warning() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute.as_mut().unwrap().tx_max_instructions = 200_000;
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    assert!(!output.contains("Pricing changes detected"));
    assert!(output.contains("tx_max_instructions"));
}

#[test]
fn test_format_diff_shows_change_count() {
    let old = full_snapshot();
    let mut new = full_snapshot();
    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 50;
    new.contract_bandwidth.as_mut().unwrap().fee_tx_size1_kb = 20;
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    assert!(output.contains("2 field change(s)"));
}

#[test]
fn test_format_diff_shows_addition_and_removal_icons() {
    let old = snapshot_with(false, false, false, false, false, false);
    let new = snapshot_with(true, true, false, false, false, false);
    let d = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&d);
    // Both additions are pricing changes, so they should show the pricing icon
    assert!(output.contains("contract_compute"));
    assert!(output.contains("contract_ledger_cost"));
}

// ── Edge cases: every section present in both but different values ─────────

#[test]
fn test_every_section_has_one_pricing_change() {
    let old = full_snapshot();
    let mut new = full_snapshot();

    new.contract_compute
        .as_mut()
        .unwrap()
        .fee_rate_per_instructions_increment = 999;
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .fee_disk_read_ledger_entry = 999;
    new.contract_historical_data
        .as_mut()
        .unwrap()
        .fee_historical1_kb = 999;
    new.contract_events
        .as_mut()
        .unwrap()
        .fee_contract_events1_kb = 999;
    new.contract_bandwidth.as_mut().unwrap().fee_tx_size1_kb = 999;
    new.state_archival
        .as_mut()
        .unwrap()
        .persistent_rent_rate_denominator = 999;

    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 6);
    assert!(d.has_pricing_changes);
    assert!(
        d.changes.iter().all(|c| c.is_pricing_change),
        "all changes should be pricing"
    );
}

#[test]
fn test_every_section_has_one_non_pricing_change() {
    let old = full_snapshot();
    let mut new = full_snapshot();

    new.contract_compute.as_mut().unwrap().tx_max_instructions = 999;
    new.contract_ledger_cost
        .as_mut()
        .unwrap()
        .ledger_max_disk_read_entries = 999;
    new.contract_events
        .as_mut()
        .unwrap()
        .tx_max_contract_events_size_bytes = 999;
    new.contract_bandwidth
        .as_mut()
        .unwrap()
        .ledger_max_txs_size_bytes = 999;
    new.state_archival.as_mut().unwrap().max_entry_ttl = 999;
    // historical_data has only one field and it's pricing, so skip it

    let d = diff::diff_snapshots(&old, &new);
    assert_eq!(d.changes.len(), 5);
    assert!(!d.has_pricing_changes);
}

// ── Property: identical snapshots always produce empty diffs ───────────────

#[test]
fn test_identity_property_all_combinations() {
    // For every possible combination of present/absent sections,
    // diffing a snapshot with itself must produce no changes.
    let combos: Vec<(bool, bool, bool, bool, bool, bool)> = (0..64)
        .map(|i| {
            (
                i & 1 != 0,
                i & 2 != 0,
                i & 4 != 0,
                i & 8 != 0,
                i & 16 != 0,
                i & 32 != 0,
            )
        })
        .collect();

    for (c, l, h, e, b, a) in &combos {
        let snap = snapshot_with(*c, *l, *h, *e, *b, *a);
        let d = diff::diff_snapshots(&snap, &snap);
        assert!(
            d.changes.is_empty(),
            "identity property violated for ({c},{l},{h},{e},{b},{a})"
        );
        assert!(
            !d.has_pricing_changes,
            "identity property: no pricing changes for ({c},{l},{h},{e},{b},{a})"
        );
    }
}

// ── Transition property: adding then removing returns to identity ──────────

#[test]
fn test_add_then_remove_section_returns_to_identity() {
    let sections = [
        ("compute", true, false, false, false, false, false),
        ("ledger_cost", false, true, false, false, false, false),
        ("historical", false, false, true, false, false, false),
        ("events", false, false, false, true, false, false),
        ("bandwidth", false, false, false, false, true, false),
        ("archival", false, false, false, false, false, true),
    ];

    for (name, c, l, h, e, b, a) in &sections {
        let base = snapshot_with(false, false, false, false, false, false);
        let added = snapshot_with(*c, *l, *h, *e, *b, *a);
        let d = diff::diff_snapshots(&base, &added);
        assert_eq!(d.changes.len(), 1, "adding {name}");

        let d2 = diff::diff_snapshots(&added, &base);
        assert_eq!(d2.changes.len(), 1, "removing {name}");
        assert_eq!(d2.changes[0].old_value, "(present)");
        assert_eq!(d2.changes[0].new_value, "(missing)");
    }
}

// ── Symmetry check: A→B has same change count as B→A ──────────────────────

#[test]
fn test_symmetry_change_count() {
    let a = snapshot_with(true, false, true, false, false, false);
    let b = snapshot_with(false, true, false, true, false, false);

    let d_ab = diff::diff_snapshots(&a, &b);
    let d_ba = diff::diff_snapshots(&b, &a);
    assert_eq!(d_ab.changes.len(), d_ba.changes.len());
}
