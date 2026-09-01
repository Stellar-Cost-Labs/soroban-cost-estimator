use crate::config_snapshot::model::{
    ConfigSnapshot, ContractBandwidthV0, ContractComputeV0, ContractEventsV0,
    ContractHistoricalDataV0, ContractLedgerCostV0, StateArchivalV0,
};

/// Maps a config setting prefix to its human-readable name.
///
/// Matches the XDR enum variant names from `ConfigSettingId`.
pub fn setting_display_name(field_path: &str) -> &str {
    match field_path.split('.').next() {
        Some("contract_compute") => "Contract Compute V0",
        Some("contract_ledger_cost") => "Contract Ledger Cost V0",
        Some("contract_historical_data") => "Contract Historical Data V0",
        Some("contract_events") => "Contract Events V0",
        Some("contract_bandwidth") => "Contract Bandwidth V0",
        Some("state_archival") => "State Archival",
        _ => field_path,
    }
}

/// Converts a raw field name (snake_case) into a Title Case label.
///
/// Example: `fee_rate_per_instructions_increment` → `Fee Rate Per Instructions Increment`
pub fn humanize_field_name(field_path: &str) -> String {
    match field_path.find('.') {
        Some(pos) => {
            let suffix = &field_path[pos + 1..];
            suffix
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        None => field_path
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Returns a human-readable display string for a field path.
///
/// Combines the setting display name with the humanized field name:
/// `contract_compute.fee_rate_per_instructions_increment`
/// → `Contract Compute V0 > Fee Rate Per Instructions Increment`
pub fn field_display_name(field_path: &str) -> String {
    let setting = setting_display_name(field_path);
    if field_path.contains('.') {
        let field = humanize_field_name(field_path);
        format!("{setting} > {field}")
    } else {
        setting.to_string()
    }
}

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

/// Formats a `ConfigDiff` as a single-line summary for CI status lines.
///
/// Example: `2 pricing changes, 1 non-pricing changes` or
/// `0 pricing changes, 0 non-pricing changes`.
pub fn format_diff_summary(diff: &ConfigDiff) -> String {
    let pricing = diff.changes.iter().filter(|c| c.is_pricing_change).count();
    let non_pricing = diff.changes.len() - pricing;
    format!("{pricing} pricing changes, {non_pricing} non-pricing changes")
}

/// ANSI escape sequences used to color pricing-change indicators by severity.
const ANSI_RED: &str = "\u{1b}[31m";
const ANSI_YELLOW: &str = "\u{1b}[33m";
const ANSI_GREEN: &str = "\u{1b}[32m";
const ANSI_RESET: &str = "\u{1b}[0m";

/// Picks an ANSI color for a pricing change based on the relative magnitude
/// of the value change:
///
/// - `< 10%` — green (minor adjustment)
/// - `10% – 50%` — yellow (moderate adjustment)
/// - `> 50%` — red (major repricing)
///
/// Non-numeric transitions (e.g. a setting appearing or disappearing) cannot
/// be quantified and are treated as major changes, colored red.
pub fn pricing_change_color(old_value: &str, new_value: &str) -> &'static str {
    let (Ok(old), Ok(new)) = (old_value.parse::<f64>(), new_value.parse::<f64>()) else {
        return ANSI_RED;
    };
    // Avoid division by zero: a change from 0 to any nonzero value is a
    // major repricing.
    let denominator = old.abs().max(f64::EPSILON);
    let ratio = (new - old).abs() / denominator;
    if ratio < 0.10 {
        ANSI_GREEN
    } else if ratio < 0.50 {
        ANSI_YELLOW
    } else {
        ANSI_RED
    }
}

/// Formats a `ConfigDiff` as a human-readable string for display.
///
/// Pricing changes are colored red/yellow/green by the magnitude of the
/// value change (see [`pricing_change_color`]); non-pricing changes are
/// left uncolored.
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
        let display = field_display_name(&change.field_path);
        if change.is_pricing_change {
            let color = pricing_change_color(&change.old_value, &change.new_value);
            output.push_str(&format!("  {color}{icon} {display}{ANSI_RESET}\n"));
            output.push_str(&format!("      Old: {}\n", change.old_value));
            output.push_str(&format!(
                "      New: {color}{}{ANSI_RESET}\n",
                change.new_value
            ));
        } else {
            output.push_str(&format!("  {icon} {display}\n"));
            output.push_str(&format!("      Old: {}\n", change.old_value));
            output.push_str(&format!("      New: {}\n", change.new_value));
        }
    }

    if diff.has_pricing_changes {
        output.push_str("\n⚠️  Pricing changes detected! Your cached estimates may be stale.\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_snapshot::model::*;

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
    fn test_setting_display_name() {
        assert_eq!(
            setting_display_name("contract_compute.foo"),
            "Contract Compute V0"
        );
        assert_eq!(
            setting_display_name("contract_ledger_cost.bar"),
            "Contract Ledger Cost V0"
        );
        assert_eq!(
            setting_display_name("contract_historical_data.baz"),
            "Contract Historical Data V0"
        );
        assert_eq!(
            setting_display_name("contract_events.qux"),
            "Contract Events V0"
        );
        assert_eq!(
            setting_display_name("contract_bandwidth.quux"),
            "Contract Bandwidth V0"
        );
        assert_eq!(
            setting_display_name("state_archival.corge"),
            "State Archival"
        );
    }

    #[test]
    fn test_humanize_field_name() {
        assert_eq!(
            humanize_field_name("contract_compute.fee_rate_per_instructions_increment"),
            "Fee Rate Per Instructions Increment"
        );
        assert_eq!(
            humanize_field_name("contract_bandwidth.fee_tx_size1_kb"),
            "Fee Tx Size1 Kb"
        );
        assert_eq!(
            humanize_field_name("state_archival.max_entry_ttl"),
            "Max Entry Ttl"
        );
    }

    #[test]
    fn test_field_display_name() {
        assert_eq!(
            field_display_name("contract_compute.fee_rate_per_instructions_increment"),
            "Contract Compute V0 > Fee Rate Per Instructions Increment"
        );
        assert_eq!(
            field_display_name("state_archival.max_entry_ttl"),
            "State Archival > Max Entry Ttl"
        );
    }

    #[test]
    fn test_format_diff_uses_human_readable_names() {
        let old = make_snapshot(100, 5);
        let new = make_snapshot(200, 5);
        let diff = diff_snapshots(&old, &new);
        let output = format_diff(&diff);
        // Should show human-readable setting name, not raw prefix
        assert!(output.contains("Contract Compute V0"));
        assert!(
            output.contains("Fee Rate Per Instructions Increment"),
            "field names should be humanized: {output}"
        );
        // Should NOT show raw snake_case path
        assert!(!output.contains("contract_compute.fee_rate_per_instructions_increment"));
    }

    #[test]
    fn test_format_diff_summary_counts() {
        let old = make_snapshot(100, 5);
        // Change the compute fee (pricing) and the bandwidth fee (pricing).
        let mut new = make_snapshot(200, 10);
        // Change a non-pricing field too, via the compute struct.
        if let Some(compute) = &mut new.contract_compute {
            compute.ledger_max_instructions = 2_000_000;
        }
        let diff = diff_snapshots(&old, &new);
        assert_eq!(
            format_diff_summary(&diff),
            "2 pricing changes, 1 non-pricing changes"
        );
    }

    #[test]
    fn test_format_diff_summary_no_changes() {
        let snap = make_snapshot(100, 5);
        let diff = diff_snapshots(&snap, &snap);
        assert_eq!(
            format_diff_summary(&diff),
            "0 pricing changes, 0 non-pricing changes"
        );
    }

    #[test]
    fn test_format_diff_multiple_settings_humanized() {
        let old = make_snapshot(100, 5);
        let new = make_snapshot(200, 10);
        let diff = diff_snapshots(&old, &new);
        let output = format_diff(&diff);
        assert!(output.contains("Contract Compute V0"));
        assert!(output.contains("Contract Bandwidth V0"));
    }

    // ── ANSI pricing-change colors (#81) ──────────────────────────────

    #[test]
    fn test_pricing_change_color_small_change_is_green() {
        assert_eq!(pricing_change_color("100", "105"), ANSI_GREEN);
    }

    #[test]
    fn test_pricing_change_color_moderate_change_is_yellow() {
        // 120/100 = 20% change → yellow band
        assert_eq!(pricing_change_color("100", "120"), ANSI_YELLOW);
    }

    #[test]
    fn test_pricing_change_color_large_change_is_red() {
        // 160/100 = 60% change → red band
        assert_eq!(pricing_change_color("100", "160"), ANSI_RED);
    }

    #[test]
    fn test_pricing_change_color_boundary_10_percent_is_yellow() {
        // Exactly 10% is no longer green (green is strictly < 10%)
        assert_eq!(pricing_change_color("100", "110"), ANSI_YELLOW);
    }

    #[test]
    fn test_pricing_change_color_boundary_50_percent_is_red() {
        // Exactly 50% is no longer yellow (yellow is strictly < 50%)
        assert_eq!(pricing_change_color("100", "150"), ANSI_RED);
    }

    #[test]
    fn test_pricing_change_color_zero_to_nonzero_is_red() {
        // Division-by-zero guard: 0 → any nonzero value is a major repricing
        assert_eq!(pricing_change_color("0", "50"), ANSI_RED);
    }

    #[test]
    fn test_pricing_change_color_non_numeric_is_red() {
        assert_eq!(pricing_change_color("(missing)", "(present)"), ANSI_RED);
        assert_eq!(pricing_change_color("(present)", "(missing)"), ANSI_RED);
    }

    #[test]
    fn test_format_diff_colors_pricing_changes() {
        let old = make_snapshot(100, 5);
        let new = make_snapshot(160, 5); // +60% compute fee → red
        let diff = diff_snapshots(&old, &new);
        let output = format_diff(&diff);
        assert!(
            output.contains(ANSI_RED),
            "large pricing change should be red: {output}"
        );
        assert!(
            output.contains(ANSI_RESET),
            "color should be reset after each change"
        );
    }

    #[test]
    fn test_format_diff_colors_small_pricing_change_green() {
        let old = make_snapshot(100, 5);
        let new = make_snapshot(105, 5); // +5% compute fee → green
        let diff = diff_snapshots(&old, &new);
        let output = format_diff(&diff);
        assert!(
            output.contains(ANSI_GREEN),
            "small pricing change should be green: {output}"
        );
    }

    #[test]
    fn test_format_diff_no_color_for_non_pricing_changes() {
        let old = make_snapshot(100, 5);
        let mut new = make_snapshot(100, 5);
        // Only touch a non-pricing field (ledger_max_instructions).
        if let Some(compute) = &mut new.contract_compute {
            compute.ledger_max_instructions = 2_000_000;
        }
        let diff = diff_snapshots(&old, &new);
        let output = format_diff(&diff);
        assert!(
            !output.contains(ANSI_RED)
                && !output.contains(ANSI_GREEN)
                && !output.contains(ANSI_YELLOW),
            "non-pricing changes should not be colored: {output}"
        );
    }

    #[test]
    fn test_format_diff_no_changes_no_ansi() {
        let snap = make_snapshot(100, 5);
        let diff = diff_snapshots(&snap, &snap);
        let output = format_diff(&diff);
        assert!(
            !output.contains("\u{1b}["),
            "no-change output should have no ANSI codes: {output}"
        );
    }
}
