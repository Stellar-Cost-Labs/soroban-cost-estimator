use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::rpc::client::RpcClient;

/// Well-known `ConfigSettingID` values used by Soroban.
///
/// These correspond to the `CONFIG_SETTING` ledger entries that control
/// resource pricing on the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigSettingId {
    ContractComputeV0 = 0,
    ContractLedgerCostV0 = 1,
    ContractHistoricalDataV0 = 2,
    ContractEventsV0 = 3,
    ContractBandwidthV0 = 4,
    StateArchival = 5,
}

impl ConfigSettingId {
    /// Human-readable on-chain setting name, matching the Stellar XDR enum.
    pub fn human_name(self) -> &'static str {
        match self {
            ConfigSettingId::ContractComputeV0 => "CONFIG_SETTING_CONTRACT_COMPUTE_V0",
            ConfigSettingId::ContractLedgerCostV0 => "CONFIG_SETTING_CONTRACT_LEDGER_COST_V0",
            ConfigSettingId::ContractHistoricalDataV0 => {
                "CONFIG_SETTING_CONTRACT_HISTORICAL_DATA_V0"
            }
            ConfigSettingId::ContractEventsV0 => "CONFIG_SETTING_CONTRACT_EVENTS_V0",
            ConfigSettingId::ContractBandwidthV0 => "CONFIG_SETTING_CONTRACT_BANDWIDTH_V0",
            ConfigSettingId::StateArchival => "CONFIG_SETTING_STATE_ARCHIVAL",
        }
    }

    /// Returns the base64-encoded XDR `LedgerKey` for this config setting.
    ///
    /// Constructs a proper `LedgerKey::ConfigSetting` XDR struct using
    /// `stellar_xdr` and encodes it to base64, as required by the
    /// `getLedgerEntries` RPC method.
    pub fn ledger_key_b64(self) -> crate::error::AppResult<String> {
        use stellar_xdr::WriteXdr;

        let xdr_id = match self {
            ConfigSettingId::ContractComputeV0 => stellar_xdr::ConfigSettingId::ContractComputeV0,
            ConfigSettingId::ContractLedgerCostV0 => {
                stellar_xdr::ConfigSettingId::ContractLedgerCostV0
            }
            ConfigSettingId::ContractHistoricalDataV0 => {
                stellar_xdr::ConfigSettingId::ContractHistoricalDataV0
            }
            ConfigSettingId::ContractEventsV0 => stellar_xdr::ConfigSettingId::ContractEventsV0,
            ConfigSettingId::ContractBandwidthV0 => {
                stellar_xdr::ConfigSettingId::ContractBandwidthV0
            }
            ConfigSettingId::StateArchival => stellar_xdr::ConfigSettingId::StateArchival,
        };

        let key = stellar_xdr::LedgerKey::ConfigSetting(stellar_xdr::LedgerKeyConfigSetting {
            config_setting_id: xdr_id,
        });

        let xdr_bytes = key
            .to_xdr(stellar_xdr::Limits::none())
            .map_err(|e| crate::error::AppError::XdrEncode(format!("LedgerKey XDR: {e}")))?;

        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &xdr_bytes,
        ))
    }
}

/// Request payload for `getLedgerEntries`.
#[derive(Debug, Serialize)]
pub struct GetLedgerEntriesParams {
    pub keys: Vec<String>,
}

/// A single ledger entry returned by `getLedgerEntries`.
///
/// The Soroban RPC returns JSON fields in camelCase.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryResult {
    #[serde(default)]
    pub key: String,
    pub xdr: String,
    #[serde(default)]
    pub last_modified_ledger_seq: Option<u32>,
    #[serde(default)]
    pub live_until_ledger_seq: Option<u32>,
}

/// Response from `getLedgerEntries`.
///
/// The Soroban RPC returns JSON fields in camelCase.
/// `latestLedger` is a ledger sequence number (integer).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetLedgerEntriesResponse {
    pub entries: Vec<LedgerEntryResult>,
    #[serde(default)]
    pub latest_ledger: Option<u64>,
}

/// A raw decoded config setting entry from the ledger.
///
/// The XDR bytes are decoded from the base64 `xdr` field of the ledger entry.
#[derive(Debug, Clone)]
pub struct ConfigSettingEntryRaw {
    pub id: ConfigSettingId,
    pub config_xdr: String,
    pub last_modified_ledger: u32,
}

/// Fetches a specific config setting entry from the ledger.
///
/// # Network calls
/// Makes one `getLedgerEntries` RPC call.
pub async fn fetch_config_setting(
    client: &RpcClient,
    setting_id: ConfigSettingId,
) -> AppResult<ConfigSettingEntryRaw> {
    let key_b64 = setting_id.ledger_key_b64()?;

    let params = GetLedgerEntriesParams {
        keys: vec![key_b64],
    };

    let response: GetLedgerEntriesResponse = client
        .call("getLedgerEntries", serde_json::to_value(params)?)
        .await?;

    let entry = response
        .entries
        .into_iter()
        .next()
        .ok_or_else(|| AppError::ConfigSettingNotFound(setting_id.human_name().to_string()))?;

    Ok(ConfigSettingEntryRaw {
        id: setting_id,
        config_xdr: entry.xdr,
        last_modified_ledger: entry.last_modified_ledger_seq.unwrap_or(0),
    })
}

/// Fetches all 6 Soroban config setting entries in a single batched RPC call.
///
/// Sends all 6 `LedgerKey` values in one `getLedgerEntries` request, then
/// matches returned entries back to their config setting IDs by re-encoding
/// each key and matching against the response's `key` field.
///
/// # Network calls
/// Makes 1 `getLedgerEntries` RPC call (all 6 keys batched).
pub async fn fetch_all_config_settings(
    client: &RpcClient,
) -> AppResult<Vec<ConfigSettingEntryRaw>> {
    let ids = [
        ConfigSettingId::ContractComputeV0,
        ConfigSettingId::ContractLedgerCostV0,
        ConfigSettingId::ContractHistoricalDataV0,
        ConfigSettingId::ContractEventsV0,
        ConfigSettingId::ContractBandwidthV0,
        ConfigSettingId::StateArchival,
    ];

    // Build all 6 keys
    let mut id_keys: Vec<(ConfigSettingId, String)> = Vec::with_capacity(ids.len());
    for id in &ids {
        let key = id.ledger_key_b64()?;
        id_keys.push((*id, key));
    }

    let params = GetLedgerEntriesParams {
        keys: id_keys.iter().map(|(_, k)| k.clone()).collect(),
    };

    let response: GetLedgerEntriesResponse = client
        .call("getLedgerEntries", serde_json::to_value(params)?)
        .await?;

    // Build a lookup: entry key base64 → ConfigSettingEntryRaw
    let mut entry_by_key: std::collections::HashMap<String, ConfigSettingEntryRaw> =
        std::collections::HashMap::new();
    for entry in response.entries {
        let raw = ConfigSettingEntryRaw {
            id: ConfigSettingId::ContractComputeV0, // placeholder; corrected after key matching below
            config_xdr: entry.xdr,
            last_modified_ledger: entry.last_modified_ledger_seq.unwrap_or(0),
        };
        entry_by_key.insert(entry.key, raw);
    }

    // Match keys to IDs and collect in order
    let mut results = Vec::with_capacity(ids.len());
    for (id, key_b64) in &id_keys {
        if let Some(mut raw) = entry_by_key.remove(key_b64) {
            raw.id = *id;
            results.push(raw);
        } else {
            return Err(AppError::ConfigSettingNotFound(id.human_name().to_string()));
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the XDR key encoding produces the expected base64 output
    /// for `ConfigSettingId::ContractComputeV0`.
    ///
    /// The XDR encodes `LedgerKey::ConfigSetting(...)` as:
    /// - 4 bytes: LedgerEntryType discriminant
    /// - 4 bytes: ConfigSettingId as u32 LE
    #[test]
    fn test_contract_compute_v0_key_encoding() {
        let key = ConfigSettingId::ContractComputeV0
            .ledger_key_b64()
            .expect("key encoding should succeed");
        // Decode and verify the XDR bytes
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key)
            .expect("base64 decode");
        assert_eq!(bytes.len(), 8, "LedgerKey XDR should be 8 bytes");
        // XDR uses big-endian (network byte order)
        let discriminant = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let config_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(
            discriminant, 8,
            "should be LedgerEntryType::ConfigSetting (8)"
        );
        assert_eq!(
            config_id, 1,
            "should be ConfigSettingId::ContractComputeV0 (1)"
        );
    }

    /// Verify each config setting ID produces a unique, non-empty key.
    #[test]
    fn test_all_config_setting_keys_are_unique() {
        let ids = [
            ConfigSettingId::ContractComputeV0,
            ConfigSettingId::ContractLedgerCostV0,
            ConfigSettingId::ContractHistoricalDataV0,
            ConfigSettingId::ContractEventsV0,
            ConfigSettingId::ContractBandwidthV0,
            ConfigSettingId::StateArchival,
        ];

        let mut keys = Vec::new();
        for id in &ids {
            let key = id.ledger_key_b64().expect("key encoding");
            assert!(!key.is_empty(), "key for {id:?} should not be empty");
            assert!(
                !keys.contains(&key),
                "key for {id:?} collides with another config setting"
            );
            keys.push(key);
        }
    }
}
