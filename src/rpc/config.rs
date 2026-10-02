use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
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

/// Every config setting this tool knows how to read, in canonical order.
///
/// The order is stable so anything derived from a full fetch (snapshot
/// building, entry iteration) is deterministic.
pub const ALL_CONFIG_SETTING_IDS: [ConfigSettingId; 6] = [
    ConfigSettingId::ContractComputeV0,
    ConfigSettingId::ContractLedgerCostV0,
    ConfigSettingId::ContractHistoricalDataV0,
    ConfigSettingId::ContractEventsV0,
    ConfigSettingId::ContractBandwidthV0,
    ConfigSettingId::StateArchival,
];

/// The network's Soroban config settings, fetched once and held in memory.
///
/// Command runs such as `estimate-all` need the same config settings
/// (`ContractComputeV0`, `ContractLedgerCostV0`, …) to price every function
/// they evaluate. Fetching them per function is wasteful: one batched
/// `getLedgerEntries` call covers all six settings, so a single [`NetworkConfig`]
/// can be shared by reference across an entire run instead of one fetch per
/// function.
///
/// Use [`ConfigCache`] to obtain a `NetworkConfig` that is guaranteed to be
/// fetched at most once per command run.
#[derive(Debug, Clone, Default)]
pub struct NetworkConfig {
    entries: HashMap<ConfigSettingId, ConfigSettingEntryRaw>,
}

impl NetworkConfig {
    /// Fetches every config setting in a single batched RPC call.
    ///
    /// All six `LedgerKey` values are sent in one `getLedgerEntries` request,
    /// then returned entries are matched back to their config setting IDs by
    /// re-encoding each key and matching against the response's `key` field.
    /// A setting missing from the response is an error: a partially populated
    /// config would silently price fees against the wrong rates.
    ///
    /// # Network calls
    /// Makes 1 `getLedgerEntries` RPC call (all 6 keys batched).
    pub async fn fetch(client: &RpcClient) -> AppResult<Self> {
        debug!("fetching all config settings (batched)");

        // Build all 6 keys
        let mut id_keys: Vec<(ConfigSettingId, String)> =
            Vec::with_capacity(ALL_CONFIG_SETTING_IDS.len());
        for id in &ALL_CONFIG_SETTING_IDS {
            let key = id.ledger_key_b64()?;
            id_keys.push((*id, key));
        }

        let params = GetLedgerEntriesParams {
            keys: id_keys.iter().map(|(_, k)| k.clone()).collect(),
        };

        let response: GetLedgerEntriesResponse = client
            .call("getLedgerEntries", serde_json::to_value(params)?)
            .await?;

        // Build a lookup: entry key base64 → (xdr, last modified ledger)
        let mut entry_by_key: HashMap<String, (String, u32)> = HashMap::new();
        for entry in response.entries {
            entry_by_key.insert(
                entry.key,
                (entry.xdr, entry.last_modified_ledger_seq.unwrap_or(0)),
            );
        }

        // Match keys to IDs and collect in canonical order
        let mut entries = HashMap::with_capacity(id_keys.len());
        for (id, key_b64) in &id_keys {
            let Some((config_xdr, last_modified_ledger)) = entry_by_key.remove(key_b64) else {
                return Err(AppError::ConfigSettingNotFound(id.human_name().to_string()));
            };
            entries.insert(
                *id,
                ConfigSettingEntryRaw {
                    id: *id,
                    config_xdr,
                    last_modified_ledger,
                },
            );
        }

        debug!(count = entries.len(), "all config settings fetched");
        Ok(Self { entries })
    }

    /// Returns the raw entry for `setting_id`, or
    /// [`AppError::ConfigSettingNotFound`] if it is not part of this config.
    pub fn get(&self, setting_id: ConfigSettingId) -> AppResult<&ConfigSettingEntryRaw> {
        self.entries
            .get(&setting_id)
            .ok_or_else(|| AppError::ConfigSettingNotFound(setting_id.human_name().to_string()))
    }

    /// Iterates the fetched entries in [`ALL_CONFIG_SETTING_IDS`] order,
    /// skipping any setting that is absent.
    pub fn iter(&self) -> impl Iterator<Item = &ConfigSettingEntryRaw> {
        ALL_CONFIG_SETTING_IDS
            .iter()
            .filter_map(move |id| self.entries.get(id))
    }

    /// Number of settings present in this config.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether this config holds no settings at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Per-command-run, in-memory cache of the network's config settings.
///
/// `estimate-all` prices N functions against the same six config settings.
/// Without a cache each function pays for its own `getLedgerEntries` round
/// trip; with one, the settings are fetched once and every function reuses
/// them, taking the run from `2 * N` network round trips down to `N + 1`.
///
/// The cache lives for as long as the command that created it, so a fresh
/// `estimate-all` still picks up a ledger's latest config — this is not a
/// persistent or cross-process cache.
///
/// A failed fetch is *not* cached: the next caller retries, so a transient RPC
/// error does not pin the run to a missing config.
#[derive(Debug, Default)]
pub struct ConfigCache {
    config: OnceCell<NetworkConfig>,
}

impl ConfigCache {
    /// Creates an empty cache; the first [`Self::get_or_fetch`] populates it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached config, fetching it on first use.
    ///
    /// # Network calls
    /// Makes 1 `getLedgerEntries` RPC call the first time it is called for the
    /// lifetime of this cache; zero on every subsequent call.
    pub async fn get_or_fetch(&self, client: &RpcClient) -> AppResult<&NetworkConfig> {
        if let Some(cached) = self.config.get() {
            trace!("config settings served from in-memory cache");
            return Ok(cached);
        }
        self.config
            .get_or_try_init(|| NetworkConfig::fetch(client))
            .await
    }

    /// Returns the already-cached config, if one has been fetched.
    ///
    /// Never touches the network — a `None` here means nothing has populated the
    /// cache yet (or the fetch failed), which callers can handle without
    /// triggering a second request.
    pub fn peek(&self) -> Option<&NetworkConfig> {
        self.config.get()
    }

    /// Whether the config has already been fetched into this cache.
    pub fn is_populated(&self) -> bool {
        self.config.initialized()
    }
}

/// Fetches all 6 Soroban config setting entries in a single batched RPC call.
///
/// # Network calls
/// Makes 1 `getLedgerEntries` RPC call (all 6 keys batched).
pub async fn fetch_all_config_settings(
    client: &RpcClient,
) -> AppResult<Vec<ConfigSettingEntryRaw>> {
    let config = NetworkConfig::fetch(client).await?;
    Ok(config.iter().cloned().collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;

    /// Base64 XDR for a `ContractComputeV0` ledger entry, small enough to
    /// inline. Only its presence matters here — the tests below assert on
    /// request counts and key round-tripping, not on decoded values.
    const DUMMY_ENTRY_XDR: &str = "AAAAAAAAAAEAAAAH";

    /// Spawns a JSON-RPC stub that answers `getLedgerEntries` by echoing back
    /// every requested key with a dummy payload, and counts the requests it
    /// received. Echoing the keys lets the client match entries back to config
    /// setting IDs the way a real node would.
    async fn spawn_ledger_entries_stub() -> (String, Arc<AtomicUsize>) {
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
                    let _ = handle_ledger_entries_conn(stream, counter).await;
                });
            }
        });

        (format!("http://{addr}"), counter)
    }

    async fn handle_ledger_entries_conn(
        mut stream: TcpStream,
        counter: Arc<AtomicUsize>,
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
                            "lastModifiedLedgerSeq": 42,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        counter.fetch_add(1, Ordering::SeqCst);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "latestLedger": 100, "entries": entries },
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

    #[tokio::test]
    async fn test_network_config_fetch_uses_a_single_batched_call() {
        let (url, counter) = spawn_ledger_entries_stub().await;
        let client = RpcClient::new(&url);

        let config = NetworkConfig::fetch(&client)
            .await
            .expect("fetch should succeed");

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "all six settings must arrive in one batched getLedgerEntries call"
        );
        assert_eq!(config.len(), ALL_CONFIG_SETTING_IDS.len());
    }

    #[tokio::test]
    async fn test_network_config_maps_entries_to_their_setting_ids() {
        let (url, _counter) = spawn_ledger_entries_stub().await;
        let client = RpcClient::new(&url);

        let config = NetworkConfig::fetch(&client)
            .await
            .expect("fetch should succeed");

        for id in ALL_CONFIG_SETTING_IDS {
            let entry = config
                .get(id)
                .unwrap_or_else(|e| panic!("{} should be present: {e}", id.human_name()));
            assert_eq!(entry.id, id, "entry should carry its own setting id");
            assert_eq!(entry.config_xdr, DUMMY_ENTRY_XDR);
            assert_eq!(entry.last_modified_ledger, 42);
        }
    }

    #[tokio::test]
    async fn test_network_config_iterates_in_canonical_order() {
        let (url, _counter) = spawn_ledger_entries_stub().await;
        let client = RpcClient::new(&url);

/// Response from `getLatestLedger`.
///
/// The Soroban RPC returns JSON fields in camelCase.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetLatestLedgerResponse {
    pub sequence: u32,
    pub protocol_version: u32,
}

/// Fetches the protocol version the network is currently running.
///
/// # Network calls
/// Makes 1 `getLatestLedger` RPC call.
pub async fn fetch_protocol_version(client: &RpcClient) -> AppResult<u32> {
    let response: GetLatestLedgerResponse = client
        .call("getLatestLedger", serde_json::json!({}))
        .await?;
    debug!(
        protocol_version = response.protocol_version,
        sequence = response.sequence,
        "fetched latest ledger"
    );
    Ok(response.protocol_version)
}

#[cfg(test)]
mod tests {
    use super::GetLatestLedgerResponse;

    #[test]
    fn test_get_latest_ledger_response_parses_protocol_version() {
        // Shape returned by Stellar RPC's `getLatestLedger`.
        let body = r#"{"id":"c73c5eac58a441d4eb733c35253ae85f783e018f7be5ef974258fed067aabb36","protocolVersion":22,"sequence":2539605}"#;
        let resp: GetLatestLedgerResponse = serde_json::from_str(body).unwrap();
        assert_eq!(resp.protocol_version, 22);
        assert_eq!(resp.sequence, 2_539_605);
    }

    #[test]
    fn test_get_latest_ledger_response_requires_protocol_version() {
        assert!(serde_json::from_str::<GetLatestLedgerResponse>(r#"{"sequence":1}"#).is_err());
    }

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

    #[test]
    fn test_default_network_config_is_empty() {
        let config = NetworkConfig::default();
        assert!(config.is_empty());
        assert_eq!(config.len(), 0);
        assert_eq!(config.iter().count(), 0);
        assert!(
            config.get(ConfigSettingId::ContractComputeV0).is_err(),
            "an empty config has no settings to hand out"
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
