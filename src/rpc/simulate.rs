use serde::{Deserialize, Serialize};
use stellar_xdr::ReadXdr;
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};
use crate::rpc::client::RpcClient;

/// Parameters for the `simulateTransaction` RPC call.
#[derive(Debug, Serialize)]
pub struct SimulateTransactionParams {
    /// Base64-encoded TransactionEnvelope XDR.
    pub transaction: String,
    /// Optional resource configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_config: Option<Value>,
}

use serde_json::Value;

/// Response from the `simulateTransaction` RPC call.
///
/// Soroban RPC serializes its JSON fields in **camelCase** (`latestLedger`,
/// `minResourceFee`, `transactionData`, `cost.cpuInsns`, …), so the struct
/// uses `rename_all = "camelCase"` to match. Without it every field would
/// silently default to `None` and reports would render as all zeros with
/// ledger 0 — the exact failure seen when the invocation path was first
/// exercised against a live deployed contract.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    /// Transaction data XDR (base64-encoded SorobanTransactionData).
    #[serde(default)]
    pub transaction_data: Option<String>,
    /// Resource usage results.
    #[serde(default)]
    pub cost: Option<CostResult>,
    /// Error message if simulation failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Latest ledger sequence number. May arrive as a JSON string or number
    /// depending on the RPC version, so both are accepted.
    #[serde(default, deserialize_with = "deserialize_u64_flexible_opt")]
    pub latest_ledger: Option<u64>,
    /// Events emitted during simulation (base64-encoded XDR).
    #[serde(default)]
    pub events: Option<Vec<String>>,
    /// Minimum resource fee (base64-encoded int64 XDR).
    #[serde(default)]
    pub min_resource_fee: Option<String>,
    /// Restore fee if state archival entry needs to be restored.
    #[serde(default)]
    pub restore_fee: Option<String>,
    /// State changes produced by the simulation.
    #[serde(default)]
    pub state_changes: Option<Vec<Value>>,
}

/// Cost breakdown from the simulation result.
///
/// The RPC serializes these as `cpuInsns`/`memBytes` (camelCase). Values may
/// arrive as JSON strings or as JSON numbers depending on the RPC version,
/// so both are accepted.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CostResult {
    /// CPU instructions consumed.
    #[serde(deserialize_with = "deserialize_u64_flexible")]
    pub cpu_insns: u64,
    /// Memory bytes used.
    #[serde(deserialize_with = "deserialize_u64_flexible")]
    pub mem_bytes: u64,
}

/// Deserialize a u64 that may arrive as a JSON string or a JSON number.
fn deserialize_u64_flexible<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("expected u64")),
        _ => Err(serde::de::Error::custom("expected string or number")),
    }
}

/// Deserialize an optional u64 that may arrive as a JSON string, number, or
/// null.
fn deserialize_u64_flexible_opt<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(s) => {
            s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Number(n) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expected u64")),
        _ => Err(serde::de::Error::custom("expected string, number, or null")),
    }
}

/// Calls `simulateTransaction` with the given base64-encoded transaction envelope.
///
/// # Network calls
/// Makes one `simulateTransaction` RPC call to the Soroban RPC endpoint.
pub async fn simulate_transaction(
    client: &RpcClient,
    transaction_xdr: &str,
) -> AppResult<SimulateTransactionResponse> {
    debug!("calling simulateTransaction");
    let params = SimulateTransactionParams {
        transaction: transaction_xdr.to_string(),
        resource_config: None,
    };

    let response: SimulateTransactionResponse = client
        .call("simulateTransaction", serde_json::to_value(params)?)
        .await?;

    if let Some(ref error) = response.error {
        debug!(error, "simulation returned error");
        return Err(AppError::SimulationFailed(error.clone()));
    }

    trace!(
        has_cost = response.cost.is_some(),
        has_transaction_data = response.transaction_data.is_some(),
        latest_ledger = ?response.latest_ledger,
        "simulateTransaction succeeded"
    );
    Ok(response)
}

/// Extracts the authoritative total resource fee (stroops) from the
/// `transaction_data` field of a `simulateTransaction` response.
///
/// `SorobanTransactionData.resource_fee` is the total Soroban resource fee
/// the transaction must cover: the non-refundable portion (instructions,
/// ledger I/O, transaction size) plus the refundable portion (events, ledger
/// rent bumps). It is the XDR-derived authority for the total; the RPC's
/// `minResourceFee` field should carry the same value.
pub fn parse_transaction_data_resource_fee(
    transaction_data: &Option<String>,
) -> AppResult<Option<i64>> {
    match transaction_data {
        Some(data_b64) => {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data_b64)
                    .map_err(|e| AppError::XdrDecode(format!("transaction_data base64: {e}")))?;

            let data =
                stellar_xdr::SorobanTransactionData::from_xdr(&bytes, stellar_xdr::Limits::none())
                    .map_err(|e| {
                        AppError::XdrDecode(format!("SorobanTransactionData from_xdr: {e}"))
                    })?;

            Ok(Some(data.resource_fee))
        }
        None => Ok(None),
    }
}

/// Resource consumption derived from the simulation's `transactionData`.
///
/// Modern Soroban RPC versions no longer return a `cost` object from
/// `simulateTransaction`; the authoritative resource usage now lives inside
/// the `transactionData` XDR (`SorobanTransactionData.resources`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationResources {
    /// CPU instructions consumed.
    pub cpu_insns: u64,
    /// Number of ledger entries read.
    pub read_entries: usize,
    /// Number of ledger entries written.
    pub write_entries: usize,
    /// Disk bytes read.
    pub read_bytes: u64,
    /// Bytes written.
    pub write_bytes: u64,
}

/// Extracts resource consumption from the `transactionData` field of a
/// `simulateTransaction` response.
///
/// Modern RPC versions return the resources used by the simulation (CPU
/// instructions, footprint sizes, disk read/write bytes) inside
/// `SorobanTransactionData.resources`, while older versions returned a
/// separate `cost` object. This is the modern-shape source for those values.
pub fn parse_transaction_data_resources(
    transaction_data: &Option<String>,
) -> AppResult<Option<SimulationResources>> {
    match transaction_data {
        Some(data_b64) => {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data_b64)
                    .map_err(|e| AppError::XdrDecode(format!("transaction_data base64: {e}")))?;

            let data =
                stellar_xdr::SorobanTransactionData::from_xdr(&bytes, stellar_xdr::Limits::none())
                    .map_err(|e| {
                        AppError::XdrDecode(format!("SorobanTransactionData from_xdr: {e}"))
                    })?;

            Ok(Some(SimulationResources {
                cpu_insns: data.resources.instructions as u64,
                read_entries: data.resources.footprint.read_only.len(),
                write_entries: data.resources.footprint.read_write.len(),
                read_bytes: data.resources.disk_read_bytes as u64,
                write_bytes: data.resources.write_bytes as u64,
            }))
        }
        None => Ok(None),
    }
}

/// Extracts the resource fee from the simulation response in stroops.
///
/// Modern RPC versions return `min_resource_fee` as a plain decimal string
/// of stroops (e.g. `"15427"`); older versions returned a base64-encoded
/// XDR int64. Both forms are accepted.
pub fn parse_resource_fee(min_resource_fee: &Option<String>) -> AppResult<Option<i64>> {
    match min_resource_fee {
        Some(fee_str) => {
            // Modern form: plain decimal stroops.
            if let Ok(stroops) = fee_str.trim().parse::<i64>() {
                return Ok(Some(stroops));
            }

            // Legacy form: base64-encoded XDR int64 (big-endian, 8 bytes).
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, fee_str)
                .map_err(|e| {
                    AppError::General(format!("failed to decode resource fee base64: {e}"))
                })?;

            let fee_bytes: [u8; 8] = bytes[..8].try_into().map_err(|_| {
                AppError::General(format!(
                    "resource fee XDR invalid length: {} bytes",
                    bytes.len()
                ))
            })?;
            Ok(Some(i64::from_be_bytes(fee_bytes)))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::WriteXdr;

    /// Regression: Soroban RPC returns camelCase JSON. Without
    /// `rename_all = "camelCase"` every field would deserialize to `None`,
    /// producing an all-zero cost report with ledger 0.
    #[test]
    fn test_simulate_response_deserializes_camelcase() {
        let json = r#"{
            "transactionData": null,
            "cost": { "cpuInsns": "100000", "memBytes": "2000" },
            "error": null,
            "latestLedger": "3894195",
            "events": [],
            "minResourceFee": null,
            "restoreFee": null,
            "stateChanges": []
        }"#;
        let resp: SimulateTransactionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.latest_ledger, Some(3_894_195));
        let cost = resp.cost.expect("cost should parse from cpuInsns/memBytes");
        assert_eq!(cost.cpu_insns, 100_000);
        assert_eq!(cost.mem_bytes, 2_000);

        // The live testnet RPC returns numeric JSON values, not strings:
        // both forms must deserialize.
        let numeric_json = r#"{
            "cost": { "cpuInsns": 100000, "memBytes": 2000 },
            "latestLedger": 3894195
        }"#;
        let resp_num: SimulateTransactionResponse = serde_json::from_str(numeric_json).unwrap();
        assert_eq!(resp_num.latest_ledger, Some(3_894_195));
        let cost_num = resp_num
            .cost
            .expect("cost should parse from numeric cpuInsns/memBytes");
        assert_eq!(cost_num.cpu_insns, 100_000);
        assert_eq!(cost_num.mem_bytes, 2_000);
    }

    /// Round-trips a `SorobanTransactionData` through base64-XDR and asserts
    /// the `resource_fee` (the authoritative total resource fee) is extracted.
    #[test]
    fn test_parse_transaction_data_resource_fee_roundtrip() {
        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: stellar_xdr::VecM::default(),
                    read_write: stellar_xdr::VecM::default(),
                },
                instructions: 100_000,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 1_234_567,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let fee = parse_transaction_data_resource_fee(&Some(b64))
            .expect("parse should succeed")
            .expect("fee should be present");
        assert_eq!(fee, 1_234_567);
    }

    #[test]
    fn test_parse_transaction_data_resource_fee_absent() {
        assert_eq!(parse_transaction_data_resource_fee(&None).unwrap(), None);
    }

    #[test]
    fn test_parse_transaction_data_resource_fee_invalid() {
        assert!(parse_transaction_data_resource_fee(&Some("not-base64!".to_string())).is_err());
    }

    /// The `minResourceFee` field is a base64-encoded XDR int64 (big-endian).
    #[test]
    fn test_parse_resource_fee_roundtrip() {
        let fee: i64 = 42_000;
        let xdr = fee.to_xdr(stellar_xdr::Limits::none()).expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);
        assert_eq!(parse_resource_fee(&Some(b64)).unwrap(), Some(42_000));
        assert_eq!(parse_resource_fee(&None).unwrap(), None);
    }

    /// Modern RPC versions return `minResourceFee` as a plain decimal string
    /// of stroops (e.g. `"15427"`), not base64 XDR.
    #[test]
    fn test_parse_resource_fee_accepts_decimal_string() {
        assert_eq!(
            parse_resource_fee(&Some("15427".to_string())).unwrap(),
            Some(15_427)
        );
    }

    /// Regression: modern Soroban RPC responses no longer include a `cost`
    /// object — resource usage must be read from `transactionData` XDR.
    /// This fixture is a real `SorobanTransactionData` captured from a live
    /// testnet `simulateTransaction` call (increment contract, step=5),
    /// base64-encoded exactly as the RPC returns it.
    #[test]
    fn test_parse_transaction_data_resources_live_fixture() {
        // Captured from soroban-testnet.stellar.org on 2026-07-31.
        let live_transaction_data = "AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAEmU1Fc+h02S4iEBnpjdCESXpKHG/bOUxC3DeRWUy9+mQAAABQAAAABAAggFgAAAAAAAACIAAAAAAAAPEM=";

        let resources = parse_transaction_data_resources(&Some(live_transaction_data.to_string()))
            .expect("parse should succeed")
            .expect("resources should be present");

        // 0x082016 = 532,502 instructions; footprint: 1 read (contract code)
        // + 1 write (the incremented counter); 136 write bytes.
        assert_eq!(resources.cpu_insns, 532_502);
        assert_eq!(resources.read_entries, 1);
        assert_eq!(resources.write_entries, 1);
        assert_eq!(resources.read_bytes, 0);
        assert_eq!(resources.write_bytes, 136);

        // The same XDR also yields the authoritative total resource fee.
        let fee = parse_transaction_data_resource_fee(&Some(live_transaction_data.to_string()))
            .expect("parse should succeed")
            .expect("fee should be present");
        assert_eq!(fee, 15_427);
    }

    #[test]
    fn test_parse_transaction_data_resources_synthetic_multiple_footprint_entries() {
        let key1 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([1u8; 32]),
            )),
        });
        let key2 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([2u8; 32]),
            )),
        });
        let key3 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([3u8; 32]),
            )),
        });

        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: vec![key1].try_into().unwrap(),
                    read_write: vec![key2, key3].try_into().unwrap(),
                },
                instructions: 750_000,
                disk_read_bytes: 512,
                write_bytes: 1024,
            },
            resource_fee: 50_000,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let resources = parse_transaction_data_resources(&Some(b64))
            .expect("parse should succeed")
            .expect("resources should be present");

        assert_eq!(resources.cpu_insns, 750_000);
        assert_eq!(resources.read_entries, 1);
        assert_eq!(resources.write_entries, 2);
        assert_eq!(resources.read_bytes, 512);
        assert_eq!(resources.write_bytes, 1024);
    }

    #[test]
    fn test_parse_transaction_data_resources_empty_footprint() {
        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: stellar_xdr::VecM::default(),
                    read_write: stellar_xdr::VecM::default(),
                },
                instructions: 0,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 0,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let resources = parse_transaction_data_resources(&Some(b64))
            .expect("parse should succeed")
            .expect("resources should be present");

        assert_eq!(resources.cpu_insns, 0);
        assert_eq!(resources.read_entries, 0);
        assert_eq!(resources.write_entries, 0);
        assert_eq!(resources.read_bytes, 0);
        assert_eq!(resources.write_bytes, 0);
    }

    #[test]
    fn test_parse_transaction_data_resources_absent() {
        assert_eq!(parse_transaction_data_resources(&None).unwrap(), None);
    }

    #[test]
    fn test_parse_transaction_data_resources_invalid() {
        assert!(parse_transaction_data_resources(&Some("not-base64!".to_string())).is_err());
    }
}
