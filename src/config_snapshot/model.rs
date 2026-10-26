use std::collections::BTreeMap;

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
    /// Network the snapshot was captured from (e.g. `testnet`).
    pub network: String,
    /// RFC 3339 timestamp of when the snapshot was taken.
    pub timestamp: String,
    /// The network's **current** ledger when the snapshot was taken, as
    /// reported by the node's `latestLedger`.
    ///
    /// This is deliberately *not* the newest `last_modified_ledger` across the
    /// config settings. Those settings only change on protocol-governance
    /// events, so that value is frozen between upgrades and would leave every
    /// snapshot reporting the same long-stale ledger.
    pub ledger: u32,
    /// Per-setting provenance: for each config setting, the ledger in which
    /// that setting last changed (the entry's `lastModifiedLedgerSeq`).
    ///
    /// Kept so the "when did this price last move?" question stays answerable
    /// from a snapshot. Defaults to empty when absent, which keeps snapshots
    /// written by earlier versions loadable.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings_last_modified: BTreeMap<String, u32>,
    /// Network protocol version reported by `getLatestLedger` when the
    /// snapshot was taken. `None` for snapshots saved before it was recorded
    /// or when the RPC call failed; omitted from JSON in that case so older
    /// files and outputs are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u32>,
    /// `ConfigSettingContractComputeV0`: CPU instruction limits and fees.
    pub contract_compute: Option<ContractComputeV0>,
    /// `ConfigSettingContractLedgerCostV0`: disk read/write limits and fees.
    pub contract_ledger_cost: Option<ContractLedgerCostV0>,
    /// `ConfigSettingContractHistoricalDataV0`: historical-data fees.
    pub contract_historical_data: Option<ContractHistoricalDataV0>,
    /// `ConfigSettingContractEventsV0`: contract-event size limits and fees.
    pub contract_events: Option<ContractEventsV0>,
    /// `ConfigSettingContractBandwidthV0`: transaction bandwidth limits and fees.
    pub contract_bandwidth: Option<ContractBandwidthV0>,
    /// `ConfigSettingStateArchival`: entry TTLs, rent rates, and eviction scan
    /// settings.
    pub state_archival: Option<StateArchivalV0>,
}

/// Top-level field names of [`ConfigSnapshot`] understood by this build.
///
/// Any other top-level field found in a stored snapshot is reported as an
/// *unknown* field rather than being silently dropped.
pub const KNOWN_SNAPSHOT_FIELDS: &[&str] = &[
    "network",
    "timestamp",
    "ledger",
    "protocol_version",
    "contract_compute",
    "contract_ledger_cost",
    "contract_historical_data",
    "contract_events",
    "contract_bandwidth",
    "state_archival",
];

/// A snapshot together with any top-level fields this build does not model.
///
/// Deserialization stays forward-compatible: `serde` ignores fields it does
/// not know, so a snapshot written by a newer version of the tool still
/// loads. [`SnapshotPayload::unknown_fields`] keeps those fields around so
/// a diff can surface them explicitly instead of pretending nothing
/// changed.
#[derive(Debug, Clone)]
pub struct SnapshotPayload {
    /// The deserialized snapshot.
    pub snapshot: ConfigSnapshot,
    /// Top-level fields present in the stored JSON that this build does not
    /// understand, keyed by field name.
    pub unknown_fields: std::collections::BTreeMap<String, serde_json::Value>,
}

impl SnapshotPayload {
    /// Wrap an already-parsed snapshot (no unknown fields).
    pub fn from_snapshot(snapshot: ConfigSnapshot) -> Self {
        Self {
            snapshot,
            unknown_fields: std::collections::BTreeMap::new(),
        }
    }

    /// Parse a snapshot from JSON text, capturing unrecognized top-level
    /// fields.
    ///
    /// Unknown fields never make parsing fail — they are collected in
    /// [`SnapshotPayload::unknown_fields`] so callers can report them.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        let snapshot: ConfigSnapshot = serde_json::from_value(value.clone())?;
        let unknown_fields = value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter(|(name, _)| !KNOWN_SNAPSHOT_FIELDS.contains(&name.as_str()))
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            snapshot,
            unknown_fields,
        })
    }

    /// Names of the unrecognized top-level fields, sorted.
    pub fn unknown_field_names(&self) -> Vec<String> {
        self.unknown_fields.keys().cloned().collect()
    }

    /// Whether the payload carries at least one unrecognized field.
    pub fn has_unknown_fields(&self) -> bool {
        !self.unknown_fields.is_empty()
    }
}

/// ConfigSettingContractComputeV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractComputeV0 {
    /// Maximum CPU instructions available to a whole ledger.
    pub ledger_max_instructions: i64,
    /// Maximum CPU instructions available to a single transaction.
    pub tx_max_instructions: i64,
    /// Fee in stroops charged per instruction increment.
    pub fee_rate_per_instructions_increment: i64,
    /// Maximum memory available to a single transaction, in bytes.
    pub tx_memory_limit: u32,
}

/// ConfigSettingContractLedgerCostV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractLedgerCostV0 {
    /// Maximum disk-read entries available to a whole ledger.
    pub ledger_max_disk_read_entries: u32,
    /// Maximum disk-read bytes available to a whole ledger.
    pub ledger_max_disk_read_bytes: u32,
    /// Maximum ledger entries a ledger may write.
    pub ledger_max_write_ledger_entries: u32,
    /// Maximum bytes a ledger may write.
    pub ledger_max_write_bytes: u32,
    /// Maximum disk-read entries available to a single transaction.
    pub tx_max_disk_read_entries: u32,
    /// Maximum disk-read bytes available to a single transaction.
    pub tx_max_disk_read_bytes: u32,
    /// Maximum ledger entries a single transaction may write.
    pub tx_max_write_ledger_entries: u32,
    /// Maximum bytes a single transaction may write.
    pub tx_max_write_bytes: u32,
    /// Fee in stroops per ledger entry read from disk.
    pub fee_disk_read_ledger_entry: i64,
    /// Fee in stroops per ledger entry written.
    pub fee_write_ledger_entry: i64,
    /// Fee in stroops per KiB read from disk.
    pub fee_disk_read1_kb: i64,
    /// Target size of the live Soroban state, in bytes.
    pub soroban_state_target_size_bytes: i64,
    /// Rent fee per KiB of Soroban state below the target size, in stroops.
    pub rent_fee1_kb_soroban_state_size_low: i64,
    /// Rent fee per KiB of Soroban state above the target size, in stroops.
    pub rent_fee1_kb_soroban_state_size_high: i64,
    /// Growth factor applied to the rent fee as state grows.
    pub soroban_state_rent_fee_growth_factor: u32,
}

/// ConfigSettingContractHistoricalDataV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractHistoricalDataV0 {
    /// Fee in stroops per KiB of historical data.
    pub fee_historical1_kb: i64,
}

/// ConfigSettingContractEventsV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractEventsV0 {
    /// Maximum total size of contract events emitted by one transaction.
    pub tx_max_contract_events_size_bytes: u32,
    /// Fee in stroops per KiB of contract events.
    pub fee_contract_events1_kb: i64,
}

/// ConfigSettingContractBandwidthV0
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractBandwidthV0 {
    /// Maximum total transaction size available to a whole ledger, in bytes.
    pub ledger_max_txs_size_bytes: u32,
    /// Maximum size of a single transaction, in bytes.
    pub tx_max_size_bytes: u32,
    /// Fee in stroops per KiB of transaction size.
    pub fee_tx_size1_kb: i64,
}

/// ConfigSettingStateArchival (StateArchivalSettings)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateArchivalV0 {
    /// Maximum time-to-live for any ledger entry, in ledgers.
    pub max_entry_ttl: u32,
    /// Minimum time-to-live for temporary entries, in ledgers.
    pub min_temporary_ttl: u32,
    /// Minimum time-to-live for persistent entries, in ledgers.
    pub min_persistent_ttl: u32,
    /// Denominator of the rent rate for persistent entries.
    pub persistent_rent_rate_denominator: i64,
    /// Denominator of the rent rate for temporary entries.
    pub temp_rent_rate_denominator: i64,
    /// Maximum number of entries archived per eviction scan.
    pub max_entries_to_archive: u32,
    /// Number of samples in the live Soroban state size window.
    pub live_soroban_state_size_window_sample_size: u32,
    /// Number of ledgers between live-state size samples.
    pub live_soroban_state_size_window_sample_period: u32,
    /// Number of entries examined per eviction scan.
    pub eviction_scan_size: u32,
    /// Initial eviction scan level (how aggressively entries are scanned).
    pub starting_eviction_scan_level: u32,
}

/// Human-readable unit for a config field path (e.g. `stroops per entry`).
///
/// Returns `None` for paths this build does not know a unit for.
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
