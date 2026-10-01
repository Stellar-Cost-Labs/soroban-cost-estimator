use serde::Serialize;

use crate::config_snapshot::diff;
use crate::config_snapshot::diff::field_display_name;
use crate::config_snapshot::model::ConfigSnapshot;
use crate::config_snapshot::store;
use crate::error::AppResult;

/// A single field change observed between two consecutive snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct FieldHistoryEntry {
    pub field_path: String,
    pub timestamp: String,
    pub ledger: u32,
    pub old_value: String,
    pub new_value: String,
    pub is_pricing_change: bool,
    /// Percentage change from `old_value` to `new_value`, when both are
    /// numeric. `None` for non-numeric transitions (e.g. `(missing)` →
    /// `(present)`), where a percentage would be meaningless.
    pub delta_percent: Option<f64>,
}

/// Percentage change between two values when both parse as numbers.
///
/// A change away from zero is reported as a full repricing: the denominator
/// is floored at `f64::EPSILON` (mirroring [`diff::pricing_change_color`]),
/// so `0 → 50` yields a huge positive percentage rather than a division
/// panic. Non-numeric values (snapshot additions/removals) yield `None`.
fn delta_percent(old_value: &str, new_value: &str) -> Option<f64> {
    let old: f64 = old_value.parse().ok()?;
    let new: f64 = new_value.parse().ok()?;
    let denominator = old.abs().max(f64::EPSILON);
    Some(((new - old) / denominator) * 100.0)
}

/// Renders a delta percentage for the timeline table: `+50.0%`, `-25.0%`, or
/// `n/a` when the transition is not numeric.
fn format_delta_percent(delta: Option<f64>) -> String {
    match delta {
        Some(value) => format!("{value:+.1}%"),
        None => "n/a".to_string(),
    }
}

/// Builds a chronological change log from a set of snapshots.
///
/// Snapshots are sorted by timestamp, then compared pairwise (each snapshot
/// against its immediate predecessor). Each changed field produces one
/// [`FieldHistoryEntry`] stamped with the timestamp/ledger of the *newer*
/// snapshot in the pair — i.e. when the change was observed to have
/// occurred. Fewer than two snapshots yields an empty log, since there is
/// nothing to compare.
pub fn build_change_log_from_snapshots(snapshots: &[ConfigSnapshot]) -> Vec<FieldHistoryEntry> {
    let mut sorted: Vec<&ConfigSnapshot> = snapshots.iter().collect();
    // Timestamp first, ledger second: the filename ordering on disk uses both,
    // and the ledger tiebreaker keeps the timeline deterministic for snapshots
    // captured within the same second.
    sorted.sort_by(|a, b| (&a.timestamp, a.ledger).cmp(&(&b.timestamp, b.ledger)));

    let mut log = Vec::new();
    for pair in sorted.windows(2) {
        let (old, new) = (pair[0], pair[1]);
        let field_diff = diff::diff_snapshots(old, new);
        for change in field_diff.changes {
            let delta = delta_percent(&change.old_value, &change.new_value);
            log.push(FieldHistoryEntry {
                field_path: change.field_path,
                timestamp: new.timestamp.clone(),
                ledger: new.ledger,
                old_value: change.old_value,
                new_value: change.new_value,
                is_pricing_change: change.is_pricing_change,
                delta_percent: delta,
            });
        }
    }
    log
}

/// Filters a change log down to a single setting.
///
/// Matches the raw field path exactly, or any fragment of it —
/// `--setting fee_rate_per_instructions_increment` selects
/// `contract_compute.fee_rate_per_instructions_increment`, and
/// `--setting contract_bandwidth` selects every bandwidth field. `None` or a
/// blank filter returns the log unchanged.
pub fn filter_change_log(
    log: &[FieldHistoryEntry],
    setting: Option<&str>,
) -> Vec<FieldHistoryEntry> {
    let Some(needle) = setting.map(str::trim).filter(|s| !s.is_empty()) else {
        return log.to_vec();
    };
    log.iter()
        .filter(|entry| entry.field_path.contains(needle))
        .cloned()
        .collect()
}

/// Reduces a change log to the single most recent entry per field.
///
/// The input need not be sorted; entries are compared by timestamp.
/// Returned entries are sorted by field path for stable, readable output.
pub fn last_changed_from_log(entries: &[FieldHistoryEntry]) -> Vec<FieldHistoryEntry> {
    use std::collections::HashMap;

    let mut latest: HashMap<&str, &FieldHistoryEntry> = HashMap::new();
    for entry in entries {
        latest
            .entry(entry.field_path.as_str())
            .and_modify(|existing| {
                if entry.timestamp > existing.timestamp {
                    *existing = entry;
                }
            })
            .or_insert(entry);
    }

    let mut result: Vec<FieldHistoryEntry> = latest.into_values().cloned().collect();
    result.sort_by(|a, b| a.field_path.cmp(&b.field_path));
    result
}

/// Loads every stored snapshot for `network` and builds its change log.
///
/// # Network calls
/// None — pure file I/O via [`store::list_snapshots`].
pub fn load_change_log(network: &str) -> AppResult<Vec<FieldHistoryEntry>> {
    let paths = store::list_snapshots(network)?;
    let mut snapshots = Vec::with_capacity(paths.len());
    for path in paths {
        let snapshot = store::load_snapshot_from_path(&path.to_string_lossy())?;
        snapshots.push(snapshot);
    }
    Ok(build_change_log_from_snapshots(&snapshots))
}

/// Formats a "last changed" table: one line per field, most recent first.
pub fn format_last_changed(network: &str, entries: &[FieldHistoryEntry]) -> String {
    let mut output = String::new();
    output.push_str(&format!("Last changed per setting: {network}\n\n"));

    if entries.is_empty() {
        output.push_str("No changes recorded (need at least two snapshots).\n");
        return output;
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    for entry in &sorted {
        let icon = if entry.is_pricing_change {
            "💰"
        } else {
            "📋"
        };
        let display = field_display_name(&entry.field_path);
        output.push_str(&format!(
            "  {icon} {display} — last changed {} (ledger {}): {} → {} ({})\n",
            entry.timestamp,
            entry.ledger,
            entry.old_value,
            entry.new_value,
            format_delta_percent(entry.delta_percent)
        ));
    }

    output
}

/// Formats a change log as a chronological timeline table: one row per
/// change with its date, ledger, setting, old value, new value, and delta
/// percentage. Entries are shown in the order given, so callers pass the
/// chronological output of [`build_change_log_from_snapshots`].
pub fn format_timeline_table(network: &str, log: &[FieldHistoryEntry]) -> String {
    if log.is_empty() {
        return format!(
            "Config change history for {network}: no changes recorded (need at least two snapshots).\n"
        );
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec![
        "Date",
        "Ledger",
        "Setting Field",
        "Old Value",
        "New Value",
        "Delta %",
    ]);
    for entry in log {
        table.add_row(vec![
            comfy_table::Cell::new(entry.timestamp.as_str()),
            comfy_table::Cell::new(entry.ledger),
            comfy_table::Cell::new(field_display_name(&entry.field_path)),
            comfy_table::Cell::new(entry.old_value.as_str()),
            comfy_table::Cell::new(entry.new_value.as_str()),
            comfy_table::Cell::new(format_delta_percent(entry.delta_percent)),
        ]);
    }
    format!(
        "Config change history for {network} ({} change(s)):\n\n{table}\n",
        log.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_snapshot::model::{
        ContractBandwidthV0, ContractComputeV0, ContractLedgerCostV0, StateArchivalV0,
    };

    fn snapshot_at(
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
            contract_ledger_cost: Some(ContractLedgerCostV0 {
                ledger_max_disk_read_entries: 200,
                ledger_max_disk_read_bytes: 1_000_000,
                ledger_max_write_ledger_entries: 200,
                ledger_max_write_bytes: 1_000_000,
                tx_max_disk_read_entries: 40,
                tx_max_disk_read_bytes: 200_000,
                tx_max_write_ledger_entries: 40,
                tx_max_write_bytes: 200_000,
                fee_disk_read_ledger_entry: 625,
                fee_write_ledger_entry: 2_500,
                fee_disk_read1_kb: 625,
                soroban_state_target_size_bytes: 100_000_000,
                rent_fee1_kb_soroban_state_size_low: 5_000,
                rent_fee1_kb_soroban_state_size_high: 50_000,
                soroban_state_rent_fee_growth_factor: 2_000,
            }),
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: Some(ContractBandwidthV0 {
                ledger_max_txs_size_bytes: 1_000_000,
                tx_max_size_bytes: 100_000,
                fee_tx_size1_kb: bandwidth_fee,
            }),
            state_archival: Some(StateArchivalV0 {
                max_entry_ttl: 5_000_000,
                min_temporary_ttl: 16,
                min_persistent_ttl: 64,
                persistent_rent_rate_denominator: 1_000,
                temp_rent_rate_denominator: 100,
                max_entries_to_archive: 300,
                live_soroban_state_size_window_sample_size: 20,
                live_soroban_state_size_window_sample_period: 60,
                eviction_scan_size: 120,
                starting_eviction_scan_level: 0,
            }),
        }
    }

    #[test]
    fn delta_percent_numeric() {
        assert!((delta_percent("100", "150").unwrap_or_default() - 50.0).abs() < 1e-9);
        assert!((delta_percent("200", "100").unwrap_or_default() + 50.0).abs() < 1e-9);
    }

    #[test]
    fn delta_percent_zero_baseline_is_a_full_repricing() {
        let delta = delta_percent("0", "50").unwrap_or_default();
        assert!(delta > 100.0, "0 → 50 must not divide by zero: {delta}");
    }

    #[test]
    fn delta_percent_non_numeric_is_none() {
        assert_eq!(delta_percent("(missing)", "(present)"), None);
    }

    #[test]
    fn timeline_carries_delta_percent() {
        let a = snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5);
        let b = snapshot_at("2026-01-02T00:00:00Z", 200, 150, 10);
        let log = build_change_log_from_snapshots(&[a, b]);

        assert_eq!(log.len(), 2);
        let compute = log
            .iter()
            .find(|e| e.field_path == "contract_compute.fee_rate_per_instructions_increment")
            .unwrap_or_else(|| panic!("compute fee entry missing"));
        assert!((compute.delta_percent.unwrap_or_default() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn filter_matches_full_path_and_fragment() {
        let log = build_change_log_from_snapshots(&[
            snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5),
            snapshot_at("2026-01-02T00:00:00Z", 200, 200, 10),
            snapshot_at("2026-01-03T00:00:00Z", 300, 250, 20),
        ]);
        assert_eq!(log.len(), 4, "two settings × two transitions");

        // Full field path.
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

        // Bare field fragment.
        let by_fragment = filter_change_log(&log, Some("fee_tx_size1_kb"));
        assert_eq!(by_fragment.len(), 2);
        assert!(
            by_fragment
                .iter()
                .all(|e| e.field_path.ends_with("fee_tx_size1_kb"))
        );

        // No filter / blank filter both keep the whole log.
        assert_eq!(filter_change_log(&log, None).len(), 4);
        assert_eq!(filter_change_log(&log, Some("  ")).len(), 4);
    }

    #[test]
    fn filter_with_no_matches_is_empty() {
        let log = build_change_log_from_snapshots(&[
            snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5),
            snapshot_at("2026-01-02T00:00:00Z", 200, 200, 5),
        ]);
        assert!(filter_change_log(&log, Some("nonexistent_setting")).is_empty());
    }

    #[test]
    fn timeline_table_shows_all_columns_and_deltas() {
        let log = build_change_log_from_snapshots(&[
            snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5),
            snapshot_at("2026-01-02T00:00:00Z", 200, 200, 5),
        ]);
        let output = format_timeline_table("testnet", &log);
        for column in [
            "Date",
            "Ledger",
            "Setting Field",
            "Old Value",
            "New Value",
            "Delta %",
        ] {
            assert!(
                output.contains(column),
                "table header missing {column}: {output}"
            );
        }
        assert!(output.contains("+100.0%"), "delta shown: {output}");
        assert!(
            output.contains("Contract Compute V0"),
            "human-readable field: {output}"
        );
    }

    #[test]
    fn timeline_table_empty_log_reports_no_changes() {
        let output = format_timeline_table("testnet", &[]);
        assert!(output.contains("no changes recorded"));
    }

    #[test]
    fn timeline_json_serializes_with_delta_percent() {
        let log = build_change_log_from_snapshots(&[
            snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5),
            snapshot_at("2026-01-02T00:00:00Z", 200, 150, 5),
        ]);
        let json = serde_json::to_string(&log).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert!(json.contains("delta_percent"));
        assert!(json.contains("field_path"));
        let parsed: Vec<FieldHistoryEntry> =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("round-trip: {e}"));
        assert_eq!(parsed.len(), 1);
        assert!((parsed[0].delta_percent.unwrap_or_default() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn ledger_tiebreak_keeps_same_second_snapshots_deterministic() {
        // Two snapshots captured within the same second: ordering by
        // timestamp alone would be ambiguous, so the ledger breaks the tie.
        let earlier = snapshot_at("2026-01-01T00:00:00Z", 500, 100, 5);
        let later = snapshot_at("2026-01-01T00:00:00Z", 600, 200, 5);
        let forward = build_change_log_from_snapshots(&[earlier.clone(), later.clone()]);
        let backward = build_change_log_from_snapshots(&[later, earlier]);

        assert_eq!(forward.len(), 1);
        assert_eq!(forward, backward, "ordering must not depend on input order");
        assert_eq!(forward[0].ledger, 600);
        assert_eq!(forward[0].old_value, "100");
        assert_eq!(forward[0].new_value, "200");
    }

    #[test]
    fn first_snapshot_produces_no_entry_on_its_own() {
        // A single (initial) snapshot is a baseline, not a change: no entry
        // should be invented for it.
        let single =
            build_change_log_from_snapshots(&[snapshot_at("2026-01-01T00:00:00Z", 100, 100, 5)]);
        assert!(single.is_empty());
    }
}
