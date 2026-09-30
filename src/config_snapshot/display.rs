//! Human-readable rendering of a stored configuration snapshot.
//!
//! Powers `config snapshot show`: organizes the snapshot's settings by
//! category, preceded by a metadata header (timestamp, ledger, protocol
//! version, network). Every captured section is rendered; a section the
//! snapshot does not carry (e.g. snapshots taken against an RPC server that
//! omits one of the six config entries) is shown as absent rather than
//! skipped, so "nothing listed" is never mistaken for "the network has no
//! such setting".

use comfy_table::Table;

use crate::config_snapshot::model::ConfigSnapshot;

/// Renders one category as a `Setting | Value` table followed by a blank
/// separator line.
///
/// A macro (not a generic function) because rows mix integer widths — the
/// config model stores limits in `u32` and fees in stroops (`i64`) — and a
/// single array literal must have one element type, so every value is
/// stringified independently before it reaches the table.
macro_rules! category_table {
    ($title:expr, $(($name:expr, $value:expr)),+ $(,)?) => {{
        let mut table = Table::new();
        table.set_header(vec!["Setting", "Value"]);
        $(
            table.add_row(vec![($name).to_string(), ($value).to_string()]);
        )+
        format!("{}\n{}\n\n", $title, table)
    }};
}

/// Formats a snapshot as a categorized, human-readable report.
///
/// The output is a metadata header followed by one table per captured
/// configuration category (Compute, Ledger Cost, Historical Data, Events,
/// Bandwidth, State Archival).
/// Length is intrinsic: every field of all six config categories is
/// rendered explicitly, so the function is linear in the config model.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn format_snapshot_details(snapshot: &ConfigSnapshot) -> String {
    let mut output = String::new();

    output.push_str(&format!("Network: {}\n", snapshot.network));
    output.push_str(&format!("Timestamp: {}\n", snapshot.timestamp));
    output.push_str(&format!("Ledger: {}\n", snapshot.ledger));
    output.push_str(&format!(
        "Protocol version: {}\n",
        match snapshot.network_protocol_version {
            Some(v) => v.to_string(),
            None => "(unknown)".to_string(),
        }
    ));
    if !snapshot.tags.is_empty() {
        output.push_str(&format!("Tags: {}\n", snapshot.tags.join(", ")));
    }

    output.push('\n');

    if let Some(compute) = &snapshot.contract_compute {
        output.push_str(&category_table!(
            "Compute",
            ("Ledger max instructions", compute.ledger_max_instructions),
            ("Tx max instructions", compute.tx_max_instructions),
            (
                "Fee rate per instructions increment (stroops per 10k)",
                compute.fee_rate_per_instructions_increment
            ),
            ("Tx memory limit (bytes)", compute.tx_memory_limit)
        ));
    } else {
        output.push_str("Compute: (not captured)\n");
    }

    if let Some(cost) = &snapshot.contract_ledger_cost {
        output.push_str(&category_table!(
            "Ledger Cost",
            (
                "Ledger max disk read entries",
                cost.ledger_max_disk_read_entries
            ),
            (
                "Ledger max disk read bytes",
                cost.ledger_max_disk_read_bytes
            ),
            (
                "Ledger max write ledger entries",
                cost.ledger_max_write_ledger_entries
            ),
            ("Ledger max write bytes", cost.ledger_max_write_bytes),
            ("Tx max disk read entries", cost.tx_max_disk_read_entries),
            ("Tx max disk read bytes", cost.tx_max_disk_read_bytes),
            (
                "Tx max write ledger entries",
                cost.tx_max_write_ledger_entries
            ),
            ("Tx max write bytes", cost.tx_max_write_bytes),
            (
                "Fee: disk read ledger entry (stroops)",
                cost.fee_disk_read_ledger_entry
            ),
            (
                "Fee: write ledger entry (stroops)",
                cost.fee_write_ledger_entry
            ),
            ("Fee: disk read 1KB (stroops)", cost.fee_disk_read1_kb),
            (
                "Soroban state target size (bytes)",
                cost.soroban_state_target_size_bytes
            ),
            (
                "Rent fee 1KB state size low (stroops)",
                cost.rent_fee1_kb_soroban_state_size_low
            ),
            (
                "Rent fee 1KB state size high (stroops)",
                cost.rent_fee1_kb_soroban_state_size_high
            ),
            (
                "State rent fee growth factor",
                cost.soroban_state_rent_fee_growth_factor
            )
        ));
    } else {
        output.push_str("Ledger Cost: (not captured)\n");
    }

    if let Some(historical) = &snapshot.contract_historical_data {
        output.push_str(&category_table!(
            "Historical Data",
            (
                "Fee: historical 1KB (stroops)",
                historical.fee_historical1_kb
            )
        ));
    } else {
        output.push_str("Historical Data: (not captured)\n");
    }

    if let Some(events) = &snapshot.contract_events {
        output.push_str(&category_table!(
            "Events",
            (
                "Tx max contract events size (bytes)",
                events.tx_max_contract_events_size_bytes
            ),
            (
                "Fee: contract events 1KB (stroops)",
                events.fee_contract_events1_kb
            )
        ));
    } else {
        output.push_str("Events: (not captured)\n");
    }

    if let Some(bandwidth) = &snapshot.contract_bandwidth {
        output.push_str(&category_table!(
            "Bandwidth",
            (
                "Ledger max txs size (bytes)",
                bandwidth.ledger_max_txs_size_bytes
            ),
            ("Tx max size (bytes)", bandwidth.tx_max_size_bytes),
            ("Fee: tx size 1KB (stroops)", bandwidth.fee_tx_size1_kb)
        ));
    } else {
        output.push_str("Bandwidth: (not captured)\n");
    }

    if let Some(archival) = &snapshot.state_archival {
        output.push_str(&category_table!(
            "State Archival",
            ("Max entry TTL", archival.max_entry_ttl),
            ("Min temporary TTL", archival.min_temporary_ttl),
            ("Min persistent TTL", archival.min_persistent_ttl),
            (
                "Persistent rent rate denominator",
                archival.persistent_rent_rate_denominator
            ),
            (
                "Temporary rent rate denominator",
                archival.temp_rent_rate_denominator
            ),
            ("Max entries to archive", archival.max_entries_to_archive),
            (
                "Live state size window sample size",
                archival.live_soroban_state_size_window_sample_size
            ),
            (
                "Live state size window sample period",
                archival.live_soroban_state_size_window_sample_period
            ),
            ("Eviction scan size", archival.eviction_scan_size),
            (
                "Starting eviction scan level",
                archival.starting_eviction_scan_level
            )
        ));
    } else {
        output.push_str("State Archival: (not captured)\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_snapshot::model::{
        ContractBandwidthV0, ContractComputeV0, ContractEventsV0, ContractHistoricalDataV0,
        ContractLedgerCostV0, StateArchivalV0,
    };

    fn full_snapshot() -> ConfigSnapshot {
        ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-08-04T07:15:38+00:00".to_string(),
            ledger: 3_470_630,
            network_protocol_version: Some(23),
            contract_compute: Some(ContractComputeV0 {
                ledger_max_instructions: 580_000_000,
                tx_max_instructions: 400_000_000,
                fee_rate_per_instructions_increment: 7,
                tx_memory_limit: 41_943_040,
            }),
            contract_ledger_cost: Some(ContractLedgerCostV0 {
                ledger_max_disk_read_entries: 200,
                ledger_max_disk_read_bytes: 4_000_000,
                ledger_max_write_ledger_entries: 200,
                ledger_max_write_bytes: 1_000_000,
                tx_max_disk_read_entries: 40,
                tx_max_disk_read_bytes: 1_000_000,
                tx_max_write_ledger_entries: 40,
                tx_max_write_bytes: 262_144,
                fee_disk_read_ledger_entry: 625,
                fee_write_ledger_entry: 2_000,
                fee_disk_read1_kb: 625,
                soroban_state_target_size_bytes: 4_000_000,
                rent_fee1_kb_soroban_state_size_low: 1_246,
                rent_fee1_kb_soroban_state_size_high: 12_461,
                soroban_state_rent_fee_growth_factor: 2_000,
            }),
            contract_historical_data: Some(ContractHistoricalDataV0 {
                fee_historical1_kb: 4_449,
            }),
            contract_events: Some(ContractEventsV0 {
                tx_max_contract_events_size_bytes: 10_000,
                fee_contract_events1_kb: 3_500,
            }),
            contract_bandwidth: Some(ContractBandwidthV0 {
                ledger_max_txs_size_bytes: 266_240,
                tx_max_size_bytes: 132_096,
                fee_tx_size1_kb: 406,
            }),
            state_archival: Some(StateArchivalV0 {
                max_entry_ttl: 6_312_000,
                min_temporary_ttl: 1_728,
                min_persistent_ttl: 631_200,
                persistent_rent_rate_denominator: 3_000,
                temp_rent_rate_denominator: 100,
                max_entries_to_archive: 600,
                live_soroban_state_size_window_sample_size: 30,
                live_soroban_state_size_window_sample_period: 100,
                eviction_scan_size: 100_000,
                starting_eviction_scan_level: 10,
            }),
            tags: vec!["protocol_upgrade_v23".to_string()],
        }
    }

    #[test]
    fn test_header_includes_metadata() {
        let output = format_snapshot_details(&full_snapshot());
        assert!(output.contains("Network: testnet"));
        assert!(output.contains("Timestamp: 2026-08-04T07:15:38+00:00"));
        assert!(output.contains("Ledger: 3470630"));
        assert!(output.contains("Protocol version: 23"));
        assert!(output.contains("Tags: protocol_upgrade_v23"));
    }

    #[test]
    fn test_all_categories_present_with_values() {
        let output = format_snapshot_details(&full_snapshot());
        for title in [
            "Compute",
            "Ledger Cost",
            "Historical Data",
            "Events",
            "Bandwidth",
            "State Archival",
        ] {
            assert!(
                output.contains(title),
                "category {title} missing from output"
            );
        }
        assert!(
            output.contains("fee_rate_per_instructions_increment")
                || output.contains("Fee rate per instructions increment")
        );
        assert!(output.contains("'7'") || output.contains("| 7 "));
        assert!(output.contains("Fee: tx size 1KB (stroops)"));
        assert!(output.contains("406"));
        assert!(output.contains("Max entry TTL"));
        assert!(output.contains("6312000") || output.contains("6,312,000"));
    }

    #[test]
    fn test_missing_sections_shown_as_not_captured() {
        let mut snap = full_snapshot();
        snap.contract_compute = None;
        snap.contract_bandwidth = None;
        let output = format_snapshot_details(&snap);
        assert!(output.contains("Compute: (not captured)"));
        assert!(output.contains("Bandwidth: (not captured)"));
        // The remaining categories must still render.
        assert!(output.contains("Ledger Cost"));
        assert!(output.contains("State Archival"));
    }

    #[test]
    fn test_empty_snapshot_still_renders_metadata() {
        let snap = ConfigSnapshot {
            network: "mainnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 1,
            network_protocol_version: None,
            contract_compute: None,
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
            tags: Vec::new(),
        };
        let output = format_snapshot_details(&snap);
        assert!(output.contains("Network: mainnet"));
        assert!(output.contains("Protocol version: (unknown)"));
        assert!(output.contains("(not captured)"));
    }
}
