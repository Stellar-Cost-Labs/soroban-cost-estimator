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

/// The result of a batched config setting fetch: the settings themselves plus
/// the node's reported `latestLedger`.
///
/// The two ledgers mean different things and must not be confused. Each
/// setting's `last_modified_ledger` is frozen at the last governance change
/// that touched it, so for any given config entry it is typically thousands of
/// ledgers behind the chain head. `latest_ledger` is the network's **current**
/// ledger, which is what a caller recording "where the chain was" wants. See
/// #267.
#[derive(Debug, Clone)]
pub struct ConfigSettingsFetch {
    /// The six config setting entries, in canonical `ConfigSettingId` order.
    pub entries: Vec<ConfigSettingEntryRaw>,
    /// The node's `latestLedger`, or `None` if the node did not report one.
    pub latest_ledger: Option<u64>,
}

/// Fetches all 6 Soroban config setting entries in a single batched RPC call.
///
/// Sends all 6 `LedgerKey` values in one `getLedgerEntries` request, then
/// matches returned entries back to their config setting IDs by re-encoding
/// each key and matching against the response's `key` field.
///
/// Also returns the node's `latestLedger` — the network's **current** ledger —
/// so callers stamping a snapshot can record where the chain actually was,
/// rather than inferring it from the settings' `last_modified_ledger_seq`
/// (which is frozen between governance events, see #267).
///
/// # Network calls
/// Makes 1 `getLedgerEntries` RPC call (all 6 keys batched).
pub async fn fetch_all_config_settings(client: &RpcClient) -> AppResult<ConfigSettingsFetch> {
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

    let latest_ledger = response.latest_ledger;

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

    debug!(
        count = results.len(),
        ?latest_ledger,
        "all config settings fetched"
    );
    Ok(ConfigSettingsFetch {
        entries: results,
        latest_ledger,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Base64 XDR for a `ContractComputeV0` ledger entry, small enough to
    /// inline. Only its presence matters here — the tests below assert on
    /// request counts, key round-tripping, and ledgers, not on decoded values.
    const DUMMY_ENTRY_XDR: &str = "AAAAAAAAAAEAAAAH";

    /// The ledger values a stub reports by default.
    const DEFAULT_LAST_MODIFIED: u32 = 42;
    const DEFAULT_LATEST_LEDGER: u64 = 100;

    /// Spawns a JSON-RPC stub that answers `getLedgerEntries` by echoing back
    /// every requested key with a dummy payload, and counts the requests it
    /// received. Echoing the keys lets the client match entries back to config
    /// setting IDs the way a real node would.
    ///
    /// `last_modified` becomes each entry's `lastModifiedLedgerSeq` and
    /// `latest` the response's `latestLedger`; passing `None` for `latest`
    /// omits the field entirely, so the "node does not report it" case is
    /// genuinely absent from the payload.
    async fn spawn_ledger_entries_stub_with(
        last_modified: u32,
        latest: Option<u64>,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let addr = listener.local_addr().expect("no local address");
        let counter = Arc::new(AtomicUsize::new(0));
        let server_counter = Arc::clone(&counter);

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let counter = Arc::clone(&server_counter);
                tokio::spawn(async move {
                    let _ =
                        handle_ledger_entries_conn(stream, counter, last_modified, latest).await;
                });
            }
        });

        (format!("http://{addr}"), counter)
    }

    /// [`spawn_ledger_entries_stub_with`] with the default ledger values.
    #[allow(dead_code)]
    async fn spawn_ledger_entries_stub() -> (String, Arc<AtomicUsize>) {
        spawn_ledger_entries_stub_with(DEFAULT_LAST_MODIFIED, Some(DEFAULT_LATEST_LEDGER)).await
    }

    async fn handle_ledger_entries_conn(
        mut stream: TcpStream,
        counter: Arc<AtomicUsize>,
        last_modified: u32,
        latest: Option<u64>,
    ) -> std::io::Result<()> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let request = String::from_utf8_lossy(&buf).to_string();
        let body = request
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        let parsed: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);

        let entries: Vec<serde_json::Value> = parsed["params"]["keys"]
            .as_array()
            .map(|keys| {
                keys.iter()
                    .map(|key| {
                        serde_json::json!({
                            "key": key.as_str().unwrap_or_default(),
                            "xdr": DUMMY_ENTRY_XDR,
                            "lastModifiedLedgerSeq": last_modified,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        counter.fetch_add(1, Ordering::SeqCst);

        let mut result = serde_json::json!({ "entries": entries });
        // Only present the field when the scenario asks for it, so the
        // "node omits latestLedger" case really is missing from the payload.
        if let Some(latest) = latest {
            result["latestLedger"] = serde_json::json!(latest);
        }

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": result,
        })
        .to_string();

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).await?;
        stream.flush().await
    }

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

    /// Regression test for #267: the batched fetch must surface the node's
    /// `latestLedger` (the network's *current* ledger) alongside the entries,
    /// and must keep the entries' much older `last_modified_ledger` separate
    /// rather than conflating the two.
    #[tokio::test]
    async fn test_fetch_surfaces_current_ledger_distinct_from_last_modified() {
        // The values observed on testnet in #267.
        const LAST_MODIFIED: u32 = 3_470_630;
        const CURRENT: u64 = 4_635_340;

        let (url, _counter) = spawn_ledger_entries_stub_with(LAST_MODIFIED, Some(CURRENT)).await;
        let client = RpcClient::new(&url);

        let fetched = fetch_all_config_settings(&client)
            .await
            .expect("fetch should succeed");

        assert_eq!(
            fetched.latest_ledger,
            Some(CURRENT),
            "the node's current ledger must be reported verbatim"
        );
        assert!(
            fetched
                .entries
                .iter()
                .all(|e| e.last_modified_ledger == LAST_MODIFIED),
            "per-setting last_modified_ledger must be preserved as-is"
        );
        assert!(
            CURRENT > u64::from(LAST_MODIFIED),
            "the test is only meaningful while the two values differ"
        );
    }

    /// The current ledger cannot be reconstructed from the entries: two
    /// fetches against an advanced chain return different ledgers even though
    /// the settings themselves have not changed.
    #[tokio::test]
    async fn test_current_ledger_varies_while_settings_stay_frozen() {
        const LAST_MODIFIED: u32 = 3_470_630;

        let (url_a, _c1) = spawn_ledger_entries_stub_with(LAST_MODIFIED, Some(4_635_340)).await;
        let (url_b, _c2) = spawn_ledger_entries_stub_with(LAST_MODIFIED, Some(4_635_412)).await;

        let first = fetch_all_config_settings(&RpcClient::new(&url_a))
            .await
            .expect("first fetch should succeed");
        let second = fetch_all_config_settings(&RpcClient::new(&url_b))
            .await
            .expect("second fetch should succeed");

        assert_eq!(first.latest_ledger, Some(4_635_340));
        assert_eq!(second.latest_ledger, Some(4_635_412));
        assert_ne!(
            first.latest_ledger, second.latest_ledger,
            "the current ledger must move as the chain advances"
        );

        // The frozen per-setting values are identical, which is precisely why
        // they cannot stand in for the current ledger.
        let frozen: Vec<u32> = first
            .entries
            .iter()
            .map(|e| e.last_modified_ledger)
            .collect();
        let frozen_again: Vec<u32> = second
            .entries
            .iter()
            .map(|e| e.last_modified_ledger)
            .collect();
        assert_eq!(frozen, frozen_again);
    }

    /// A node that omits `latestLedger` must yield `None` rather than a
    /// fabricated value, so the caller can fall back explicitly.
    #[tokio::test]
    async fn test_missing_latest_ledger_is_reported_as_none() {
        let (url, _counter) = spawn_ledger_entries_stub_with(3_470_630, None).await;
        let client = RpcClient::new(&url);

        let fetched = fetch_all_config_settings(&client)
            .await
            .expect("fetch should still succeed");

        assert_eq!(
            fetched.latest_ledger, None,
            "an absent latestLedger must not be guessed at"
        );
        assert_eq!(fetched.entries.len(), 6);
    }
}
