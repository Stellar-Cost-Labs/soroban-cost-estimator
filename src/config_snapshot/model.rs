use serde::{Deserialize, Serialize};

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

impl std::fmt::Display for ConfigSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Network: {} (ledger {} at {})",
            self.network, self.ledger, self.timestamp
        )
    }
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

    #[test]
    fn test_config_snapshot_display() {
        let snapshot = ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 100,
            contract_compute: None,
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
        };
        assert_eq!(
            format!("{}", snapshot),
            "Network: testnet (ledger 100 at 2026-01-01T00:00:00Z)"
        );
    }
}
