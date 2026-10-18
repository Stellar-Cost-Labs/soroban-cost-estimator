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

/// Documented initial and current schema version for persisted [`ConfigSnapshot`] files.
///
/// Unversioned legacy snapshot files are treated as version `1` on read without
/// modifying the file on disk.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

pub const fn default_snapshot_schema_version() -> u32 {
    SNAPSHOT_SCHEMA_VERSION
}

/// Validates a snapshot schema version against [`SNAPSHOT_SCHEMA_VERSION`].
pub fn validate_snapshot_schema_version(version: u32) -> Result<(), String> {
    match version.cmp(&SNAPSHOT_SCHEMA_VERSION) {
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Greater => Err(format!(
            "unsupported future snapshot schema_version {version} (supported: {SNAPSHOT_SCHEMA_VERSION}); recommended action: upgrade soroban-cost-estimator or re-capture the snapshot with `soroban-cost-estimator config snapshot`"
        )),
        std::cmp::Ordering::Less => Err(format!(
            "invalid snapshot schema_version {version} (supported: {SNAPSHOT_SCHEMA_VERSION}); recommended action: re-capture the snapshot with `soroban-cost-estimator config snapshot`"
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireConfigSnapshot {
    #[serde(default = "default_snapshot_schema_version")]
    schema_version: u32,
    network: String,
    timestamp: String,
    ledger: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protocol_version: Option<u32>,
    contract_compute: Option<ContractComputeV0>,
    contract_ledger_cost: Option<ContractLedgerCostV0>,
    contract_historical_data: Option<ContractHistoricalDataV0>,
    contract_events: Option<ContractEventsV0>,
    contract_bandwidth: Option<ContractBandwidthV0>,
    state_archival: Option<StateArchivalV0>,
}

impl From<ConfigSnapshot> for WireConfigSnapshot {
    fn from(snapshot: ConfigSnapshot) -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            network: snapshot.network,
            timestamp: snapshot.timestamp,
            ledger: snapshot.ledger,
            protocol_version: snapshot.protocol_version,
            contract_compute: snapshot.contract_compute,
            contract_ledger_cost: snapshot.contract_ledger_cost,
            contract_historical_data: snapshot.contract_historical_data,
            contract_events: snapshot.contract_events,
            contract_bandwidth: snapshot.contract_bandwidth,
            state_archival: snapshot.state_archival,
        }
    }
}

impl TryFrom<WireConfigSnapshot> for ConfigSnapshot {
    type Error = String;

    fn try_from(wire: WireConfigSnapshot) -> Result<Self, Self::Error> {
        validate_snapshot_schema_version(wire.schema_version)?;
        Ok(Self {
            network: wire.network,
            timestamp: wire.timestamp,
            ledger: wire.ledger,
            protocol_version: wire.protocol_version,
            contract_compute: wire.contract_compute,
            contract_ledger_cost: wire.contract_ledger_cost,
            contract_historical_data: wire.contract_historical_data,
            contract_events: wire.contract_events,
            contract_bandwidth: wire.contract_bandwidth,
            state_archival: wire.state_archival,
        })
    }
}

/// A complete snapshot of the network's Soroban resource-pricing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "WireConfigSnapshot", into = "WireConfigSnapshot")]
pub struct ConfigSnapshot {
    pub network: String,
    pub timestamp: String,
    pub ledger: u32,
    /// Network protocol version reported by `getLatestLedger` when the
    /// snapshot was taken. `None` for snapshots saved before it was recorded
    /// or when the RPC call failed; omitted from JSON in that case so older
    /// files and outputs are unchanged.
    pub protocol_version: Option<u32>,
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

pub fn setting_unit_description(field_path: &str) -> Option<&'static str> {
    match field_path {
        "contract_compute.fee_rate_per_instructions_increment" => {
            Some("stroops per 10,000 CPU instructions")
        }
        "contract_ledger_cost.fee_disk_read_ledger_entry"
        | "contract_ledger_cost.fee_write_ledger_entry" => Some("stroops per entry"),
        "contract_ledger_cost.fee_disk_read1_kb"
        | "contract_ledger_cost.rent_fee1_kb_soroban_state_size_low"
        | "contract_ledger_cost.rent_fee1_kb_soroban_state_size_high"
        | "contract_historical_data.fee_historical1_kb"
        | "contract_events.fee_contract_events1_kb"
        | "contract_bandwidth.fee_tx_size1_kb" => Some("stroops per 1KB"),
        "state_archival.persistent_rent_rate_denominator"
        | "state_archival.temp_rent_rate_denominator" => Some("fractional fee scaling"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigSnapshot;

    const LEGACY: &str = r#"{"network":"testnet","timestamp":"t","ledger":7,
        "contract_compute":null,"contract_ledger_cost":null,
        "contract_historical_data":null,"contract_events":null,
        "contract_bandwidth":null,"state_archival":null}"#;

    #[test]
    fn snapshot_without_protocol_version_still_loads() {
        let snap: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        assert_eq!(snap.protocol_version, None);
        assert_eq!(snap.ledger, 7);
    }

    #[test]
    fn missing_protocol_version_is_not_written() {
        let snap: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("protocol_version"), "{json}");
    }

    #[test]
    fn protocol_version_round_trips() {
        let mut snap: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        snap.protocol_version = Some(23);
        let back: ConfigSnapshot =
            serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
        assert_eq!(back.protocol_version, Some(23));
    }
}
