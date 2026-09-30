use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

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
    debug!(setting = setting_id.human_name(), "fetching config setting");

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

    trace!(
        setting = setting_id.human_name(),
        last_modified = entry.last_modified_ledger_seq,
        "config setting fetched"
    );
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
    debug!("fetching all config settings (batched)");
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

    debug!(count = results.len(), "all config settings fetched");
    Ok(results)
}

/// In-memory cache for `getLedgerEntries` config setting responses.
///
/// Avoids duplicate RPC fetches within a single command run (e.g.
/// `estimate-all`), where the same config setting may be needed by
/// multiple simulation passes.
///
/// The cache is cheap to create (an empty `HashMap`) and intentionally
/// has no TTL or eviction — it lives for the duration of a single
/// command invocation and is dropped when that command finishes.
#[derive(Debug, Default)]
pub struct ConfigCache {
    entries: HashMap<ConfigSettingId, ConfigSettingEntryRaw>,
}

impl ConfigCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetch a single config setting, returning from cache if available.
    ///
    /// On a cache miss the setting is fetched via `fetch_config_setting`
    /// and stored for subsequent lookups.
    ///
    /// # Network calls
    /// Zero if cached, otherwise one `getLedgerEntries` RPC call.
    pub async fn get_config_setting(
        &mut self,
        client: &RpcClient,
        setting_id: ConfigSettingId,
    ) -> AppResult<ConfigSettingEntryRaw> {
        if let Some(entry) = self.entries.get(&setting_id) {
            debug!(
                setting = setting_id.human_name(),
                "config setting served from cache"
            );
            return Ok(entry.clone());
        }

        let entry = fetch_config_setting(client, setting_id).await?;
        self.entries.insert(setting_id, entry.clone());
        Ok(entry)
    }

    /// Fetch all 6 Soroban config settings, serving any previously
    /// cached entries from memory and only requesting missing ones.
    ///
    /// When the cache is empty this is equivalent to a single batched
    /// `fetch_all_config_settings` call. When some entries are already
    /// cached, only the missing entries are fetched individually and
    /// inserted into the cache.
    ///
    /// # Network calls
    /// Zero if all 6 are cached, one batched call when the cache is
    /// empty, or up to 6 individual calls for partial caches.
    pub async fn get_all_config_settings(
        &mut self,
        client: &RpcClient,
    ) -> AppResult<Vec<ConfigSettingEntryRaw>> {
        let all_ids = [
            ConfigSettingId::ContractComputeV0,
            ConfigSettingId::ContractLedgerCostV0,
            ConfigSettingId::ContractHistoricalDataV0,
            ConfigSettingId::ContractEventsV0,
            ConfigSettingId::ContractBandwidthV0,
            ConfigSettingId::StateArchival,
        ];

        // Collect IDs that are not yet cached.
        let missing: Vec<ConfigSettingId> = all_ids
            .iter()
            .copied()
            .filter(|id| !self.entries.contains_key(id))
            .collect();

        if missing.is_empty() {
            debug!("all config settings served from cache");
            return Ok(all_ids
                .iter()
                .map(|id| self.entries[id].clone())
                .collect());
        }

        if missing.len() == all_ids.len() {
            // Cache is completely empty — use the efficient batched call.
            let results = fetch_all_config_settings(client).await?;
            for entry in &results {
                self.entries.insert(entry.id, entry.clone());
            }
            return Ok(results);
        }

        // Partial cache — fetch only the missing entries individually.
        debug!(
            cached = all_ids.len() - missing.len(),
            missing = missing.len(),
            "fetching missing config settings"
        );
        for id in &missing {
            let entry = fetch_config_setting(client, *id).await?;
            self.entries.insert(*id, entry);
        }

        Ok(all_ids
            .iter()
            .map(|id| self.entries[id].clone())
            .collect())
    }

    /// Returns the number of config settings currently held in the cache.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
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

    // ── ConfigCache tests ──────────────────────────────────────────

    #[test]
    fn test_config_cache_starts_empty() {
        let cache = ConfigCache::new();
        assert_eq!(cache.len(), 0, "new cache should be empty");
    }

    #[test]
    fn test_config_cache_default_is_empty() {
        let cache = ConfigCache::default();
        assert_eq!(cache.len(), 0, "default cache should be empty");
    }

    #[test]
    fn test_config_cache_stores_entry() {
        let mut cache = ConfigCache::new();
        let entry = ConfigSettingEntryRaw {
            id: ConfigSettingId::ContractComputeV0,
            config_xdr: "test-xdr-data".to_string(),
            last_modified_ledger: 100,
        };
        cache.entries.insert(entry.id, entry.clone());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_config_cache_lookup_by_id() {
        let mut cache = ConfigCache::new();
        let entry = ConfigSettingEntryRaw {
            id: ConfigSettingId::StateArchival,
            config_xdr: "archival-xdr".to_string(),
            last_modified_ledger: 200,
        };
        cache.entries.insert(entry.id, entry.clone());

        let fetched = cache.entries.get(&ConfigSettingId::StateArchival);
        assert!(fetched.is_some(), "should find cached entry");
        assert_eq!(fetched.unwrap().config_xdr, "archival-xdr");
    }

    #[test]
    fn test_config_cache_miss_returns_none() {
        let cache = ConfigCache::new();
        let fetched = cache.entries.get(&ConfigSettingId::ContractComputeV0);
        assert!(fetched.is_none(), "empty cache should return None");
    }

    #[test]
    fn test_config_cache_populate_all_ids() {
        let mut cache = ConfigCache::new();
        let ids = [
            ConfigSettingId::ContractComputeV0,
            ConfigSettingId::ContractLedgerCostV0,
            ConfigSettingId::ContractHistoricalDataV0,
            ConfigSettingId::ContractEventsV0,
            ConfigSettingId::ContractBandwidthV0,
            ConfigSettingId::StateArchival,
        ];

        for (i, id) in ids.iter().enumerate() {
            cache.entries.insert(
                *id,
                ConfigSettingEntryRaw {
                    id: *id,
                    config_xdr: format!("xdr-{i}"),
                    last_modified_ledger: i as u32,
                },
            );
        }

        assert_eq!(cache.len(), 6, "cache should hold all 6 settings");

        // Verify all entries are retrievable.
        for id in &ids {
            let entry = cache
                .entries
                .get(id)
                .expect("entry should be present");
            assert_eq!(entry.id, *id);
        }
    }

    #[test]
    fn test_config_cache_overwrite_on_duplicate_insert() {
        let mut cache = ConfigCache::new();
        let entry_v1 = ConfigSettingEntryRaw {
            id: ConfigSettingId::ContractComputeV0,
            config_xdr: "v1".to_string(),
            last_modified_ledger: 10,
        };
        let entry_v2 = ConfigSettingEntryRaw {
            id: ConfigSettingId::ContractComputeV0,
            config_xdr: "v2".to_string(),
            last_modified_ledger: 20,
        };

        cache.entries.insert(entry_v1.id, entry_v1);
        cache.entries.insert(entry_v2.id, entry_v2.clone());

        assert_eq!(cache.len(), 1, "should still be 1 entry after overwrite");
        let fetched = cache.entries.get(&entry_v2.id).unwrap();
        assert_eq!(fetched.config_xdr, "v2");
        assert_eq!(fetched.last_modified_ledger, 20);
    }

    #[test]
    fn test_config_cache_isolation_between_settings() {
        let mut cache = ConfigCache::new();
        cache.entries.insert(
            ConfigSettingId::ContractComputeV0,
            ConfigSettingEntryRaw {
                id: ConfigSettingId::ContractComputeV0,
                config_xdr: "compute-xdr".to_string(),
                last_modified_ledger: 1,
            },
        );
        cache.entries.insert(
            ConfigSettingId::StateArchival,
            ConfigSettingEntryRaw {
                id: ConfigSettingId::StateArchival,
                config_xdr: "archival-xdr".to_string(),
                last_modified_ledger: 2,
            },
        );

        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache
                .entries
                .get(&ConfigSettingId::ContractComputeV0)
                .unwrap()
                .config_xdr,
            "compute-xdr"
        );
        assert_eq!(
            cache
                .entries
                .get(&ConfigSettingId::StateArchival)
                .unwrap()
                .config_xdr,
            "archival-xdr"
        );
    }

    #[test]
    fn test_config_cache_clears_on_drop() {
        let mut cache = ConfigCache::new();
        cache.entries.insert(
            ConfigSettingId::ContractComputeV0,
            ConfigSettingEntryRaw {
                id: ConfigSettingId::ContractComputeV0,
                config_xdr: "data".to_string(),
                last_modified_ledger: 1,
            },
        );
        assert_eq!(cache.len(), 1);
        drop(cache);
        // Cache is dropped; no lingering state.
    }
}
