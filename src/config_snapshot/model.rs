use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::rpc::config::ConfigSettingId;

/// Current schema version of the persisted snapshot file format.
///
/// Bump this whenever the JSON shape of [`ConfigSnapshot`] changes. Snapshots
/// written by an older tool are migrated forward implicitly (see
/// [`INITIAL_SNAPSHOT_SCHEMA_VERSION`]); snapshots written by a *newer* tool
/// are rejected by [`ensure_supported_snapshot_schema`] rather than being
/// silently misread.
///
/// Version history:
/// * `1` — initial version. Also the implicit version of snapshots saved
///   before the `schema_version` key existed, whose shape is unchanged.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Implicit schema version of snapshots written before the `schema_version`
/// key existed.
///
/// Such files have no `schema_version` field, so serde's `default` fills in
/// this value via [`initial_snapshot_schema_version`]. They require no
/// transformation to reach [`SNAPSHOT_SCHEMA_VERSION`] today; a future bump
/// would add that migration here.
pub const INITIAL_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// serde default for the `schema_version` field, applied to unversioned
/// (legacy) snapshot files.
fn initial_snapshot_schema_version() -> u32 {
    INITIAL_SNAPSHOT_SCHEMA_VERSION
}

/// Rejects a snapshot whose declared schema version cannot be interpreted by
/// this build.
///
/// The check is deliberately explicit instead of relying on serde defaults:
/// a file written by a newer release may omit fields this build requires, and
/// failing with the offending file name and a recommended action is much more
/// useful than a deserialization error.
///
/// Reading never rewrites the file, so a snapshot from a newer release is
/// left exactly as it is for the newer tool to keep using.
///
/// # Errors
/// [`AppError::SnapshotSchemaUnsupported`] when `version` is newer than
/// [`SNAPSHOT_SCHEMA_VERSION`], and [`AppError::SnapshotParse`] when it is
/// older than [`INITIAL_SNAPSHOT_SCHEMA_VERSION`] (not a version this format
/// ever used).
pub fn ensure_supported_snapshot_schema(file: &Path, version: u32) -> AppResult<()> {
    if version > SNAPSHOT_SCHEMA_VERSION {
        return Err(AppError::SnapshotSchemaUnsupported {
            file: file.display().to_string(),
            version,
            supported: SNAPSHOT_SCHEMA_VERSION,
        });
    }
    if version < INITIAL_SNAPSHOT_SCHEMA_VERSION {
        return Err(AppError::SnapshotParse(format!(
            "{} declares schema version {version}, but the initial snapshot schema version is \
             {INITIAL_SNAPSHOT_SCHEMA_VERSION}",
            file.display()
        )));
    }
    Ok(())
}

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
    /// Version of the persisted file schema this snapshot was written with.
    ///
    /// Serialized first so the version is immediately visible in a snapshot
    /// file. Unversioned legacy files deserialize to
    /// [`INITIAL_SNAPSHOT_SCHEMA_VERSION`]; loads never rewrite them.
    #[serde(default = "initial_snapshot_schema_version")]
    pub schema_version: u32,
    pub network: String,
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
    pub contract_compute: Option<ContractComputeV0>,
    pub contract_ledger_cost: Option<ContractLedgerCostV0>,
    pub contract_historical_data: Option<ContractHistoricalDataV0>,
    pub contract_events: Option<ContractEventsV0>,
    pub contract_bandwidth: Option<ContractBandwidthV0>,
    pub state_archival: Option<StateArchivalV0>,
}

impl ConfigSnapshot {
    /// Whether two snapshots record the *same configuration*.
    ///
    /// This compares normalized data fields — the network, the protocol
    /// version, and all six config settings — rather than serialized JSON, so
    /// field order and capture metadata never force a spurious difference.
    ///
    /// Deliberately ignored:
    /// * `timestamp` — every capture has a new one, even when nothing changed.
    /// * `ledger` — snapshots record the ledger at which the entries were last
    ///   modified, not the current ledger. An entry rewritten with identical
    ///   values has not changed the configuration.
    /// * `schema_version` — a file-format detail, not a pricing input.
    ///
    /// Used by `config snapshot --skip-unchanged` to avoid writing a duplicate
    /// file for a network whose pricing is stable.
    #[must_use]
    pub fn same_configuration(&self, other: &Self) -> bool {
        self.network == other.network
            && self.protocol_version == other.protocol_version
            && self.contract_compute == other.contract_compute
            && self.contract_ledger_cost == other.contract_ledger_cost
            && self.contract_historical_data == other.contract_historical_data
            && self.contract_events == other.contract_events
            && self.contract_bandwidth == other.contract_bandwidth
            && self.state_archival == other.state_archival
    }
}

/// Top-level field names of [`ConfigSnapshot`] understood by this build.
///
/// Any other top-level field found in a stored snapshot is reported as an
/// *unknown* field rather than being silently dropped.
pub const KNOWN_SNAPSHOT_FIELDS: &[&str] = &[
    "schema_version",
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
    use super::*;

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

    // ── Schema versioning (#508) ──────────────────────────────────────────

    #[test]
    fn unversioned_snapshot_is_the_documented_initial_version() {
        let snap: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        assert_eq!(snap.schema_version, INITIAL_SNAPSHOT_SCHEMA_VERSION);
        assert_eq!(INITIAL_SNAPSHOT_SCHEMA_VERSION, SNAPSHOT_SCHEMA_VERSION);
    }

    #[test]
    fn serialized_snapshot_declares_the_current_schema_version() {
        let snap: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(
            json.contains(&format!("\"schema_version\":{SNAPSHOT_SCHEMA_VERSION}")),
            "a new snapshot must carry its schema version: {json}"
        );
    }

    #[test]
    fn schema_version_is_a_known_field_not_an_unknown_one() {
        let payload = SnapshotPayload::parse(LEGACY).unwrap();
        assert!(
            !payload.has_unknown_fields(),
            "schema_version must be modelled, not reported as unknown: {:?}",
            payload.unknown_field_names()
        );
    }

    #[test]
    fn current_and_legacy_schema_versions_are_accepted() {
        for version in [INITIAL_SNAPSHOT_SCHEMA_VERSION, SNAPSHOT_SCHEMA_VERSION] {
            assert!(
                ensure_supported_snapshot_schema(Path::new("snapshot.json"), version).is_ok(),
                "version {version} must be readable"
            );
        }
    }

    #[test]
    fn future_schema_version_is_rejected_with_the_file_and_upgrade_hint() {
        let path = Path::new("/tmp/testnet-future.json");
        let error = ensure_supported_snapshot_schema(path, SNAPSHOT_SCHEMA_VERSION + 1)
            .expect_err("a newer schema must be rejected");
        let message = error.to_string();
        assert!(
            matches!(error, AppError::SnapshotSchemaUnsupported { .. }),
            "expected SnapshotSchemaUnsupported, got: {message}"
        );
        assert!(message.contains("/tmp/testnet-future.json"), "{message}");
        assert!(
            message.contains("upgrade soroban-cost-estimator"),
            "{message}"
        );
    }

    #[test]
    fn version_zero_is_rejected_as_invalid() {
        let error = ensure_supported_snapshot_schema(Path::new("snapshot.json"), 0)
            .expect_err("version 0 was never a valid snapshot schema");
        assert!(matches!(error, AppError::SnapshotParse(_)), "{error}");
        assert!(error.to_string().contains("schema version 0"), "{error}");
    }

    // ── Change detection for `--skip-unchanged` (#509) ────────────────────

    #[test]
    fn same_configuration_ignores_capture_metadata() {
        let a: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();
        let mut b = a.clone();
        b.timestamp = "2026-02-02T00:00:00Z".to_string();
        b.ledger = 99;
        b.schema_version = SNAPSHOT_SCHEMA_VERSION;
        assert!(a.same_configuration(&b));
    }

    #[test]
    fn same_configuration_detects_protocol_setting_and_network_changes() {
        let a: ConfigSnapshot = serde_json::from_str(LEGACY).unwrap();

        let mut protocol = a.clone();
        protocol.protocol_version = Some(24);
        assert!(!a.same_configuration(&protocol));

        let mut pricing = a.clone();
        pricing.contract_compute = Some(ContractComputeV0 {
            ledger_max_instructions: 1_000_000,
            tx_max_instructions: 100_000,
            fee_rate_per_instructions_increment: 25,
            tx_memory_limit: 1,
        });
        assert!(!a.same_configuration(&pricing));

        let mut other_network = a.clone();
        other_network.network = "mainnet".to_string();
        assert!(!a.same_configuration(&other_network));
    }
}
