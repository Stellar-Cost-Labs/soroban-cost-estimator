use crate::config_snapshot::model::{
    ConfigSnapshot, ContractBandwidthV0, ContractComputeV0, ContractEventsV0,
    ContractHistoricalDataV0, ContractLedgerCostV0, StateArchivalV0,
};

/// A single changed field between two snapshots.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    pub field_path: String,
    pub old_value: String,
    pub new_value: String,
    pub is_pricing_change: bool,
}

/// The result of comparing two config snapshots.
#[derive(Debug, Clone)]
pub struct ConfigDiff {
    pub old_snapshot: SnapshotInfo,
    pub new_snapshot: SnapshotInfo,
    pub changes: Vec<FieldDiff>,
    pub has_pricing_changes: bool,
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub network: String,
    pub timestamp: String,
    pub ledger: u32,
}

/// Compares two config snapshots and returns a detailed diff.
pub fn diff_snapshots(old: &ConfigSnapshot, new: &ConfigSnapshot) -> ConfigDiff {
    let mut changes = Vec::new();

    compare_contract_compute(&mut changes, &old.contract_compute, &new.contract_compute);
    compare_ledger_cost(
        &mut changes,
        &old.contract_ledger_cost,
        &new.contract_ledger_cost,
    );
    compare_historical_data(
        &mut changes,
        &old.contract_historical_data,
        &new.contract_historical_data,
    );
    compare_events(&mut changes, &old.contract_events, &new.contract_events);
    compare_bandwidth(
        &mut changes,
        &old.contract_bandwidth,
        &new.contract_bandwidth,
    );
    compare_state_archival(&mut changes, &old.state_archival, &new.state_archival);

    let has_pricing_changes = changes.iter().any(|c| c.is_pricing_change);

    ConfigDiff {
        old_snapshot: SnapshotInfo {
            network: old.network.clone(),
            timestamp: old.timestamp.clone(),
            ledger: old.ledger,
        },
        new_snapshot: SnapshotInfo {
            network: new.network.clone(),
            timestamp: new.timestamp.clone(),
            ledger: new.ledger,
        },
        changes,
        has_pricing_changes,
    }
}

fn compare_contract_compute(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<ContractComputeV0>,
    new: &Option<ContractComputeV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "contract_compute.ledger_max_instructions",
                &old.ledger_max_instructions,
                &new.ledger_max_instructions,
                false,
            );
            check(
                diffs,
                "contract_compute.tx_max_instructions",
                &old.tx_max_instructions,
                &new.tx_max_instructions,
                false,
            );
            check(
                diffs,
                "contract_compute.fee_rate_per_instructions_increment",
                &old.fee_rate_per_instructions_increment,
                &new.fee_rate_per_instructions_increment,
                true,
            );
            check(
                diffs,
                "contract_compute.tx_memory_limit",
                &old.tx_memory_limit,
                &new.tx_memory_limit,
                false,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "contract_compute".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "contract_compute".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn compare_ledger_cost(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<ContractLedgerCostV0>,
    new: &Option<ContractLedgerCostV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "contract_ledger_cost.ledger_max_disk_read_entries",
                &old.ledger_max_disk_read_entries,
                &new.ledger_max_disk_read_entries,
                false,
            );
            check(
                diffs,
                "contract_ledger_cost.ledger_max_disk_read_bytes",
                &old.ledger_max_disk_read_bytes,
                &new.ledger_max_disk_read_bytes,
                false,
            );
            check(
                diffs,
                "contract_ledger_cost.ledger_max_write_ledger_entries",
                &old.ledger_max_write_ledger_entries,
                &new.ledger_max_write_ledger_entries,
                false,
            );
            check(
                diffs,
                "contract_ledger_cost.ledger_max_write_bytes",
                &old.ledger_max_write_bytes,
                &new.ledger_max_write_bytes,
                false,
            );
            check(
                diffs,
                "contract_ledger_cost.fee_disk_read_ledger_entry",
                &old.fee_disk_read_ledger_entry,
                &new.fee_disk_read_ledger_entry,
                true,
            );
            check(
                diffs,
                "contract_ledger_cost.fee_write_ledger_entry",
                &old.fee_write_ledger_entry,
                &new.fee_write_ledger_entry,
                true,
            );
            check(
                diffs,
                "contract_ledger_cost.fee_disk_read1_kb",
                &old.fee_disk_read1_kb,
                &new.fee_disk_read1_kb,
                true,
            );
            check(
                diffs,
                "contract_ledger_cost.soroban_state_target_size_bytes",
                &old.soroban_state_target_size_bytes,
                &new.soroban_state_target_size_bytes,
                false,
            );
            check(
                diffs,
                "contract_ledger_cost.rent_fee1_kb_soroban_state_size_low",
                &old.rent_fee1_kb_soroban_state_size_low,
                &new.rent_fee1_kb_soroban_state_size_low,
                true,
            );
            check(
                diffs,
                "contract_ledger_cost.rent_fee1_kb_soroban_state_size_high",
                &old.rent_fee1_kb_soroban_state_size_high,
                &new.rent_fee1_kb_soroban_state_size_high,
                true,
            );
            check(
                diffs,
                "contract_ledger_cost.soroban_state_rent_fee_growth_factor",
                &old.soroban_state_rent_fee_growth_factor,
                &new.soroban_state_rent_fee_growth_factor,
                false,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "contract_ledger_cost".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "contract_ledger_cost".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn compare_historical_data(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<ContractHistoricalDataV0>,
    new: &Option<ContractHistoricalDataV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "contract_historical_data.fee_historical1_kb",
                &old.fee_historical1_kb,
                &new.fee_historical1_kb,
                true,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "contract_historical_data".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "contract_historical_data".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn compare_events(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<ContractEventsV0>,
    new: &Option<ContractEventsV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "contract_events.tx_max_contract_events_size_bytes",
                &old.tx_max_contract_events_size_bytes,
                &new.tx_max_contract_events_size_bytes,
                false,
            );
            check(
                diffs,
                "contract_events.fee_contract_events1_kb",
                &old.fee_contract_events1_kb,
                &new.fee_contract_events1_kb,
                true,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "contract_events".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "contract_events".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn compare_bandwidth(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<ContractBandwidthV0>,
    new: &Option<ContractBandwidthV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "contract_bandwidth.ledger_max_txs_size_bytes",
                &old.ledger_max_txs_size_bytes,
                &new.ledger_max_txs_size_bytes,
                false,
            );
            check(
                diffs,
                "contract_bandwidth.tx_max_size_bytes",
                &old.tx_max_size_bytes,
                &new.tx_max_size_bytes,
                false,
            );
            check(
                diffs,
                "contract_bandwidth.fee_tx_size1_kb",
                &old.fee_tx_size1_kb,
                &new.fee_tx_size1_kb,
                true,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "contract_bandwidth".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "contract_bandwidth".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn compare_state_archival(
    diffs: &mut Vec<FieldDiff>,
    old: &Option<StateArchivalV0>,
    new: &Option<StateArchivalV0>,
) {
    match (old, new) {
        (Some(old), Some(new)) => {
            check(
                diffs,
                "state_archival.max_entry_ttl",
                &old.max_entry_ttl,
                &new.max_entry_ttl,
                false,
            );
            check(
                diffs,
                "state_archival.min_temporary_ttl",
                &old.min_temporary_ttl,
                &new.min_temporary_ttl,
                false,
            );
            check(
                diffs,
                "state_archival.min_persistent_ttl",
                &old.min_persistent_ttl,
                &new.min_persistent_ttl,
                false,
            );
            check(
                diffs,
                "state_archival.persistent_rent_rate_denominator",
                &old.persistent_rent_rate_denominator,
                &new.persistent_rent_rate_denominator,
                true,
            );
            check(
                diffs,
                "state_archival.temp_rent_rate_denominator",
                &old.temp_rent_rate_denominator,
                &new.temp_rent_rate_denominator,
                true,
            );
            check(
                diffs,
                "state_archival.max_entries_to_archive",
                &old.max_entries_to_archive,
                &new.max_entries_to_archive,
                false,
            );
            check(
                diffs,
                "state_archival.live_soroban_state_size_window_sample_size",
                &old.live_soroban_state_size_window_sample_size,
                &new.live_soroban_state_size_window_sample_size,
                false,
            );
            check(
                diffs,
                "state_archival.live_soroban_state_size_window_sample_period",
                &old.live_soroban_state_size_window_sample_period,
                &new.live_soroban_state_size_window_sample_period,
                false,
            );
            check(
                diffs,
                "state_archival.eviction_scan_size",
                &old.eviction_scan_size,
                &new.eviction_scan_size,
                false,
            );
            check(
                diffs,
                "state_archival.starting_eviction_scan_level",
                &old.starting_eviction_scan_level,
                &new.starting_eviction_scan_level,
                false,
            );
        }
        (None, Some(_)) => diffs.push(FieldDiff {
            field_path: "state_archival".to_string(),
            old_value: "(missing)".to_string(),
            new_value: "(present)".to_string(),
            is_pricing_change: true,
        }),
        (Some(_), None) => diffs.push(FieldDiff {
            field_path: "state_archival".to_string(),
            old_value: "(present)".to_string(),
            new_value: "(missing)".to_string(),
            is_pricing_change: true,
        }),
        _ => {}
    }
}

fn check<T: PartialEq + std::fmt::Display>(
    diffs: &mut Vec<FieldDiff>,
    path: &str,
    old: &T,
    new: &T,
    is_pricing: bool,
) {
    if old != new {
        diffs.push(FieldDiff {
            field_path: path.to_string(),
            old_value: old.to_string(),
            new_value: new.to_string(),
            is_pricing_change: is_pricing,
        });
    }
}

/// Returns a copy of a diff containing only pricing-related changes.
pub fn pricing_only(diff: &ConfigDiff) -> ConfigDiff {
    let changes = diff
        .changes
        .iter()
        .filter(|change| change.is_pricing_change)
        .cloned()
        .collect();

    ConfigDiff {
        old_snapshot: diff.old_snapshot.clone(),
        new_snapshot: diff.new_snapshot.clone(),
        has_pricing_changes: !changes.is_empty(),
        changes,
    }
}

/// Formats a `ConfigDiff` as a human-readable string for display.
pub fn format_diff(diff: &ConfigDiff) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "Config diff: {} (ledger {}) → {} (ledger {})\n",
        diff.old_snapshot.timestamp,
        diff.old_snapshot.ledger,
        diff.new_snapshot.timestamp,
        diff.new_snapshot.ledger,
    ));
    output.push_str(&format!("Network: {}\n\n", diff.new_snapshot.network));

    if diff.changes.is_empty() {
        output.push_str("✅ No changes detected.\n");
        return output;
    }

    output.push_str(&format!(
        "Found {} field change(s):\n\n",
        diff.changes.len()
    ));

    for change in &diff.changes {
        let icon = if change.is_pricing_change {
            "💰"
        } else {
            "📋"
        };
        output.push_str(&format!("  {icon} {}\n", change.field_path));
        output.push_str(&format!("      Old: {}\n", change.old_value));
        output.push_str(&format!("      New: {}\n", change.new_value));
    }

    if diff.has_pricing_changes {
        output.push_str("\n⚠️  Pricing changes detected! Your cached estimates may be stale.\n");
    }

    output
}
