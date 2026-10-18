/// Network resource-pricing configuration snapshot management.
pub mod diff;
pub mod history;
pub mod model;
pub mod store;

use tracing::{Instrument, debug, info_span, warn};

use crate::cache::{self, CachedEstimate};
use crate::error::{AppError, AppResult};
use crate::rpc::{self, client::RpcClient};
use crate::xdr_helper;

/// Result of fetching the live network configuration, comparing it against the
/// latest local snapshot (when available), and identifying stale cached
/// estimates.
#[derive(Debug, Clone)]
pub struct ConfigDiffResult {
    /// Latest local snapshot on disk for the network prior to this fetch, if any.
    pub old_snapshot: Option<model::ConfigSnapshot>,
    /// Freshly fetched live configuration snapshot from the RPC node.
    pub new_snapshot: model::ConfigSnapshot,
    /// Diff between `old_snapshot` and `new_snapshot`, when `old_snapshot` exists.
    pub diff: Option<diff::ConfigDiff>,
    /// Cached estimates for the network recorded before `new_snapshot.ledger`.
    pub stale_estimates: Vec<CachedEstimate>,
}

/// Fetch all live Soroban configuration settings from `client` and assemble a
/// [`model::ConfigSnapshot`] for `network`.
///
/// # Network calls
/// Makes one batched `getLedgerEntries` RPC call plus one `getLedgerEntries` /
/// protocol-version lookup via `client`.
pub async fn fetch_live_snapshot(
    client: &RpcClient,
    network: &str,
) -> Result<model::ConfigSnapshot, AppError> {
    let span = info_span!("fetch_config_snapshot", network);
    async {
        debug!("fetching all config settings");
        let raw_entries = rpc::config::fetch_all_config_settings(client).await?;
        debug!(entries = raw_entries.len(), "received config entries");

        let mut snapshot = xdr_helper::begin_snapshot(network, 0);
        for raw in &raw_entries {
            let config_entry =
                xdr_helper::decode_config_entry_xdr(&raw.config_xdr, client.verbose)?;
            xdr_helper::apply_config_entry(&mut snapshot, config_entry);
        }
        if let Some(latest) = raw_entries.iter().map(|e| e.last_modified_ledger).max() {
            snapshot.ledger = latest;
        }
        // The protocol version is informational (shown by `config snapshot
        // list`), so a failure here must not lose the snapshot itself.
        match rpc::config::fetch_protocol_version(client).await {
            Ok(version) => snapshot.protocol_version = Some(version),
            Err(e) => warn!(error = %e, "could not fetch protocol version; recording none"),
        }
        debug!(
            ledger = snapshot.ledger,
            protocol_version = ?snapshot.protocol_version,
            "config snapshot built"
        );
        Ok(snapshot)
    }
    .instrument(span)
    .await
}

/// Return cached estimates for `network` recorded prior to `ledger`.
///
/// An unreadable or empty cache yields an empty vector rather than failing the
/// diff pipeline.
#[must_use]
pub fn stale_estimates_for(network: &str, ledger: u32) -> Vec<CachedEstimate> {
    cache::list_cached_estimates(network)
        .map(|estimates| {
            cache::find_stale_estimates(&estimates, ledger)
                .into_iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Execute the live configuration diff pipeline:
/// 1. Fetch live network config settings from `client` and construct a [`model::ConfigSnapshot`].
/// 2. Load the latest local snapshot for `network` (if one exists on disk).
/// 3. Compute the [`diff::ConfigDiff`] between the local baseline and live snapshot.
/// 4. Detect cached estimates for `network` that predate the live ledger sequence.
///
/// # Network calls
/// Delegates to [`fetch_live_snapshot`].
pub async fn run_live_config_diff(
    client: &RpcClient,
    network: &str,
) -> AppResult<ConfigDiffResult> {
    let old_snapshot = store::load_latest_snapshot(network).ok();
    let new_snapshot = fetch_live_snapshot(client, network).await?;
    let diff = old_snapshot
        .as_ref()
        .map(|old| diff::diff_snapshots(old, &new_snapshot));
    let stale_estimates = stale_estimates_for(network, new_snapshot.ledger);

    Ok(ConfigDiffResult {
        old_snapshot,
        new_snapshot,
        diff,
        stale_estimates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::config::ConfigSettingId;
    use base64::Engine;
    use stellar_xdr::{
        ConfigSettingContractBandwidthV0, ConfigSettingContractComputeV0,
        ConfigSettingContractEventsV0, ConfigSettingContractHistoricalDataV0,
        ConfigSettingContractLedgerCostV0, ConfigSettingEntry, Limits, StateArchivalSettings,
        WriteXdr,
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method},
    };

    fn encode_entry(entry: &ConfigSettingEntry) -> String {
        let data = stellar_xdr::LedgerEntryData::ConfigSetting(entry.clone());
        let bytes = data.to_xdr(Limits::none()).expect("xdr encode");
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[tokio::test]
    async fn test_run_live_config_diff_fetches_and_builds_snapshot() {
        let server = MockServer::start().await;

        let entries = [
            (
                ConfigSettingId::ContractComputeV0,
                ConfigSettingEntry::ContractComputeV0(ConfigSettingContractComputeV0 {
                    ledger_max_instructions: 100_000_000,
                    tx_max_instructions: 10_000_000,
                    fee_rate_per_instructions_increment: 25,
                    tx_memory_limit: 41_943_040,
                }),
            ),
            (
                ConfigSettingId::ContractLedgerCostV0,
                ConfigSettingEntry::ContractLedgerCostV0(ConfigSettingContractLedgerCostV0 {
                    ledger_max_disk_read_entries: 100,
                    ledger_max_disk_read_bytes: 100_000,
                    ledger_max_write_ledger_entries: 50,
                    ledger_max_write_bytes: 50_000,
                    tx_max_disk_read_entries: 40,
                    tx_max_disk_read_bytes: 40_000,
                    tx_max_write_ledger_entries: 25,
                    tx_max_write_bytes: 25_000,
                    fee_disk_read_ledger_entry: 6250,
                    fee_write_ledger_entry: 10000,
                    fee_disk_read1_kb: 1786,
                    soroban_state_target_size_bytes: 1_000_000,
                    rent_fee1_kb_soroban_state_size_low: 100,
                    rent_fee1_kb_soroban_state_size_high: 1000,
                    soroban_state_rent_fee_growth_factor: 10,
                }),
            ),
            (
                ConfigSettingId::ContractHistoricalDataV0,
                ConfigSettingEntry::ContractHistoricalDataV0(
                    ConfigSettingContractHistoricalDataV0 {
                        fee_historical1_kb: 300,
                    },
                ),
            ),
            (
                ConfigSettingId::ContractEventsV0,
                ConfigSettingEntry::ContractEventsV0(ConfigSettingContractEventsV0 {
                    tx_max_contract_events_size_bytes: 8198,
                    fee_contract_events1_kb: 200,
                }),
            ),
            (
                ConfigSettingId::ContractBandwidthV0,
                ConfigSettingEntry::ContractBandwidthV0(ConfigSettingContractBandwidthV0 {
                    ledger_max_txs_size_bytes: 1_000_000,
                    tx_max_size_bytes: 65_536,
                    fee_tx_size1_kb: 1624,
                }),
            ),
            (
                ConfigSettingId::StateArchival,
                ConfigSettingEntry::StateArchival(StateArchivalSettings {
                    max_entry_ttl: 6_312_000,
                    min_temporary_ttl: 16,
                    min_persistent_ttl: 120_960,
                    persistent_rent_rate_denominator: 2103,
                    temp_rent_rate_denominator: 4206,
                    max_entries_to_archive: 100,
                    live_soroban_state_size_window_sample_size: 30,
                    live_soroban_state_size_window_sample_period: 64,
                    eviction_scan_size: 100_000,
                    starting_eviction_scan_level: 7,
                }),
            ),
        ];

        let json_entries: Vec<serde_json::Value> = entries
            .iter()
            .map(|(id, entry)| {
                serde_json::json!({
                    "key": id.ledger_key_b64().expect("key"),
                    "xdr": encode_entry(entry),
                    "lastModifiedLedgerSeq": 777
                })
            })
            .collect();

        Mock::given(method("POST"))
            .and(body_string_contains("getLedgerEntries"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "entries": json_entries,
                    "latestLedger": 777
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("getLatestLedger"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "sequence": 777,
                    "protocolVersion": 22
                }
            })))
            .mount(&server)
            .await;

        let client = RpcClient::new(&server.uri());
        let res = run_live_config_diff(&client, "isolated-test-net-392")
            .await
            .expect("live config diff");

        assert_eq!(res.new_snapshot.network, "isolated-test-net-392");
        assert_eq!(res.new_snapshot.ledger, 777);
        assert_eq!(res.new_snapshot.protocol_version, Some(22));
        assert!(res.old_snapshot.is_none());
        assert!(res.diff.is_none());
        assert!(res.stale_estimates.is_empty());
    }
}
