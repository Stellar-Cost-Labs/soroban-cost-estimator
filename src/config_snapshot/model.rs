use serde::{Deserialize, Serialize};

use crate::rpc::config::ConfigSettingId;

/// Maps a [`ConfigSettingId`] to a friendly, human-readable setting name.
///
/// Every user-facing rendering of a config setting (diff tables, diff
/// headers, `--json` payloads) goes through this helper so raw enum
/// numbers like `0`, `1`, `4` never reach an output on their own.
///
/// Examples
/// --------
/// - `ConfigSettingId::ContractComputeV0` → `Contract Compute V0`
/// - `ConfigSettingId::ContractLedgerCostV0` → `Contract Ledger Cost V0`
/// - `ConfigSettingId::StateArchival` → `State Archival`
pub fn config_setting_human_name(id: &ConfigSettingId) -> &'static str {
    match id {
        ConfigSettingId::ContractComputeV0 => "Contract Compute V0",
        ConfigSettingId::ContractLedgerCostV0 => "Contract Ledger Cost V0",
        ConfigSettingId::ContractHistoricalDataV0 => "Contract Historical Data V0",
        ConfigSettingId::ContractEventsV0 => "Contract Events V0",
        ConfigSettingId::ContractBandwidthV0 => "Contract Bandwidth V0",
        ConfigSettingId::StateArchival => "State Archival",
    }
}

/// Maps a snapshot field-path prefix (e.g. `contract_compute`) to its
/// [`ConfigSettingId`], or `None` when the prefix names no known setting.
///
/// This is the inverse bridge from stored snapshot paths (which carry no
/// enum value) back to the id so [`config_setting_human_name`] can label
/// them consistently.
pub fn config_setting_id_for_prefix(prefix: &str) -> Option<ConfigSettingId> {
    match prefix {
        "contract_compute" => Some(ConfigSettingId::ContractComputeV0),
        "contract_ledger_cost" => Some(ConfigSettingId::ContractLedgerCostV0),
        "contract_historical_data" => Some(ConfigSettingId::ContractHistoricalDataV0),
        "contract_events" => Some(ConfigSettingId::ContractEventsV0),
        "contract_bandwidth" => Some(ConfigSettingId::ContractBandwidthV0),
        "state_archival" => Some(ConfigSettingId::StateArchival),
        _ => None,
    }
}

/// A complete snapshot of the network's Soroban resource-pricing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub network: String,
    pub timestamp: String,
    pub ledger: u32,
    pub contract_compute: Option<ContractComputeV0>,
    pub contract_ledger_cost: Option<ContractLedgerCostV0>,
    pub contract_historical_data: Option<ContractHistoricalDataV0>,
    pub contract_events: Option<ContractEventsV0>,
    pub contract_bandwidth: Option<ContractBandwidthV0>,
    pub state_archival: Option<StateArchivalV0>,
}

/// ConfigSettingContractComputeV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractComputeV0 {
    pub ledger_max_instructions: i64,
    pub tx_max_instructions: i64,
    pub fee_rate_per_instructions_increment: i64,
    pub tx_memory_limit: u32,
}

/// ConfigSettingContractLedgerCostV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractLedgerCostV0 {
    pub ledger_max_disk_read_entries: u32,
    pub ledger_max_disk_read_bytes: u32,
    pub ledger_max_write_ledger_entries: u32,
    pub ledger_max_write_bytes: u32,
    pub tx_max_disk_read_entries: u32,
    pub tx_max_disk_read_bytes: u32,
    pub tx_max_write_ledger_entries: u32,
    pub tx_max_write_bytes: u32,
    pub fee_disk_read_ledger_entry: i64,
    pub fee_write_ledger_entry: i64,
    pub fee_disk_read1_kb: i64,
    pub soroban_state_target_size_bytes: i64,
    pub rent_fee1_kb_soroban_state_size_low: i64,
    pub rent_fee1_kb_soroban_state_size_high: i64,
    pub soroban_state_rent_fee_growth_factor: u32,
}

/// ConfigSettingContractHistoricalDataV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractHistoricalDataV0 {
    pub fee_historical1_kb: i64,
}

/// ConfigSettingContractEventsV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractEventsV0 {
    pub tx_max_contract_events_size_bytes: u32,
    pub fee_contract_events1_kb: i64,
}

/// ConfigSettingContractBandwidthV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractBandwidthV0 {
    pub ledger_max_txs_size_bytes: u32,
    pub tx_max_size_bytes: u32,
    pub fee_tx_size1_kb: i64,
}

/// ConfigSettingStateArchival (StateArchivalSettings)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateArchivalV0 {
    pub max_entry_ttl: u32,
    pub min_temporary_ttl: u32,
    pub min_persistent_ttl: u32,
    pub persistent_rent_rate_denominator: i64,
    pub temp_rent_rate_denominator: i64,
    pub max_entries_to_archive: u32,
    pub live_soroban_state_size_window_sample_size: u32,
    pub live_soroban_state_size_window_sample_period: u32,
    pub eviction_scan_size: u32,
    pub starting_eviction_scan_level: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full set of config settings this tool tracks for the current
    /// Stellar Protocol (mirrors `rpc::config::fetch_all_config_settings`),
    /// paired with their raw `ConfigSettingId` numbers and expected names.
    const ALL_SETTINGS: [(ConfigSettingId, u32, &str); 6] = [
        (ConfigSettingId::ContractComputeV0, 0, "Contract Compute V0"),
        (
            ConfigSettingId::ContractLedgerCostV0,
            1,
            "Contract Ledger Cost V0",
        ),
        (
            ConfigSettingId::ContractHistoricalDataV0,
            2,
            "Contract Historical Data V0",
        ),
        (ConfigSettingId::ContractEventsV0, 3, "Contract Events V0"),
        (
            ConfigSettingId::ContractBandwidthV0,
            4,
            "Contract Bandwidth V0",
        ),
        (ConfigSettingId::StateArchival, 5, "State Archival"),
    ];

    /// AC: complete mapping coverage for all current Protocol config settings.
    #[test]
    fn test_config_setting_human_name_covers_all_settings() {
        let mut names = Vec::with_capacity(ALL_SETTINGS.len());
        for (id, raw_number, expected) in ALL_SETTINGS {
            let name = config_setting_human_name(&id);
            assert_eq!(
                name, expected,
                "friendly name for {id:?} must stay stable (raw id {raw_number})"
            );
            assert!(!name.is_empty(), "name for {id:?} must not be empty");
            assert_eq!(
                id as u32, raw_number,
                "raw id number for {id:?} (asserted so --json keeps exposing it)"
            );
            names.push(name);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            ALL_SETTINGS.len(),
            "every config setting must map to a unique name"
        );
    }

    /// Every tracked field-path prefix resolves to a known id whose friendly
    /// name comes from the same helper.
    #[test]
    fn test_config_setting_id_for_prefix_covers_all_settings() {
        let prefixes = [
            "contract_compute",
            "contract_ledger_cost",
            "contract_historical_data",
            "contract_events",
            "contract_bandwidth",
            "state_archival",
        ];
        let mut names = Vec::with_capacity(prefixes.len());
        for prefix in prefixes {
            let id = config_setting_id_for_prefix(prefix)
                .unwrap_or_else(|| panic!("prefix {prefix} should map to a ConfigSettingId"));
            names.push(config_setting_human_name(&id));
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            prefixes.len(),
            "each prefix must resolve to a distinct setting name"
        );

        assert!(
            config_setting_id_for_prefix("unknown_setting").is_none(),
            "unknown prefixes must not invent a setting id"
        );
        assert!(config_setting_id_for_prefix("").is_none());
    }
}
