//! Helpers for encoding/decoding Soroban XDR types using `stellar_xdr`.

use stellar_xdr::ReadXdr;
use stellar_xdr::WriteXdr;

use crate::config_snapshot::model::ConfigSnapshot;
use crate::error::{AppError, AppResult};

/// Decode a base64-encoded XDR `LedgerEntryData` and extract a typed `ConfigSettingEntry`.
///
/// The Soroban RPC `getLedgerEntries` returns the entry data as a `LedgerEntryData`
/// XDR (not the full `LedgerEntry` which includes `lastModifiedLedgerSeq` and `ext`
/// fields that are returned as separate JSON fields).
pub fn decode_config_entry_xdr(xdr_b64: &str) -> AppResult<stellar_xdr::ConfigSettingEntry> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, xdr_b64)
        .map_err(|e| AppError::XdrDecode(format!("base64 decode: {e}")))?;

    let entry_data = stellar_xdr::LedgerEntryData::from_xdr(&bytes, stellar_xdr::Limits::none())
        .map_err(|e| AppError::XdrDecode(format!("LedgerEntryData from_xdr: {e}")))?;

    match entry_data {
        stellar_xdr::LedgerEntryData::ConfigSetting(config_entry) => Ok(config_entry),
        other => Err(AppError::XdrDecode(format!(
            "expected ConfigSetting entry, got {}",
            other.name()
        ))),
    }
}

/// Initialize a snapshot with the network, timestamp, and ledger set; all
/// config entries start empty.
pub fn begin_snapshot(network: &str, ledger: u32) -> ConfigSnapshot {
    use chrono::Utc;
    ConfigSnapshot {
        network: network.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        ledger,
        contract_compute: None,
        contract_ledger_cost: None,
        contract_historical_data: None,
        contract_events: None,
        contract_bandwidth: None,
        state_archival: None,
    }
}

/// Apply one decoded `ConfigSettingEntry` into a snapshot.
pub fn apply_config_entry(snapshot: &mut ConfigSnapshot, entry: stellar_xdr::ConfigSettingEntry) {
    use crate::config_snapshot::model::*;
    match entry {
        stellar_xdr::ConfigSettingEntry::ContractComputeV0(s) => {
            snapshot.contract_compute = Some(ContractComputeV0 {
                ledger_max_instructions: s.ledger_max_instructions,
                tx_max_instructions: s.tx_max_instructions,
                fee_rate_per_instructions_increment: s.fee_rate_per_instructions_increment,
                tx_memory_limit: s.tx_memory_limit,
            });
        }
        stellar_xdr::ConfigSettingEntry::ContractLedgerCostV0(s) => {
            snapshot.contract_ledger_cost = Some(ContractLedgerCostV0 {
                ledger_max_disk_read_entries: s.ledger_max_disk_read_entries,
                ledger_max_disk_read_bytes: s.ledger_max_disk_read_bytes,
                ledger_max_write_ledger_entries: s.ledger_max_write_ledger_entries,
                ledger_max_write_bytes: s.ledger_max_write_bytes,
                tx_max_disk_read_entries: s.tx_max_disk_read_entries,
                tx_max_disk_read_bytes: s.tx_max_disk_read_bytes,
                tx_max_write_ledger_entries: s.tx_max_write_ledger_entries,
                tx_max_write_bytes: s.tx_max_write_bytes,
                fee_disk_read_ledger_entry: s.fee_disk_read_ledger_entry,
                fee_write_ledger_entry: s.fee_write_ledger_entry,
                fee_disk_read1_kb: s.fee_disk_read1_kb,
                soroban_state_target_size_bytes: s.soroban_state_target_size_bytes,
                rent_fee1_kb_soroban_state_size_low: s.rent_fee1_kb_soroban_state_size_low,
                rent_fee1_kb_soroban_state_size_high: s.rent_fee1_kb_soroban_state_size_high,
                soroban_state_rent_fee_growth_factor: s.soroban_state_rent_fee_growth_factor,
            });
        }
        stellar_xdr::ConfigSettingEntry::ContractHistoricalDataV0(s) => {
            snapshot.contract_historical_data = Some(ContractHistoricalDataV0 {
                fee_historical1_kb: s.fee_historical1_kb,
            });
        }
        stellar_xdr::ConfigSettingEntry::ContractEventsV0(s) => {
            snapshot.contract_events = Some(ContractEventsV0 {
                tx_max_contract_events_size_bytes: s.tx_max_contract_events_size_bytes,
                fee_contract_events1_kb: s.fee_contract_events1_kb,
            });
        }
        stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(s) => {
            snapshot.contract_bandwidth = Some(ContractBandwidthV0 {
                ledger_max_txs_size_bytes: s.ledger_max_txs_size_bytes,
                tx_max_size_bytes: s.tx_max_size_bytes,
                fee_tx_size1_kb: s.fee_tx_size1_kb,
            });
        }
        stellar_xdr::ConfigSettingEntry::StateArchival(s) => {
            snapshot.state_archival = Some(StateArchivalV0 {
                max_entry_ttl: s.max_entry_ttl,
                min_temporary_ttl: s.min_temporary_ttl,
                min_persistent_ttl: s.min_persistent_ttl,
                persistent_rent_rate_denominator: s.persistent_rent_rate_denominator,
                temp_rent_rate_denominator: s.temp_rent_rate_denominator,
                max_entries_to_archive: s.max_entries_to_archive,
                live_soroban_state_size_window_sample_size: s
                    .live_soroban_state_size_window_sample_size,
                live_soroban_state_size_window_sample_period: s
                    .live_soroban_state_size_window_sample_period,
                eviction_scan_size: s.eviction_scan_size,
                starting_eviction_scan_level: s.starting_eviction_scan_level,
            });
        }
        _ => {}
    }
}

/// Construct a minimal `TransactionEnvelope` for simulating a contract
/// operation via `simulateTransaction`.
///
/// Returns the **raw XDR bytes** rather than the base64 encoding: the
/// bandwidth fee is computed from the transaction size in XDR bytes, and
/// base64 inflates the byte count by ~33%, which would overcharge the
/// bandwidth fee. Callers base64-encode for the RPC if needed.
///
/// * `wasm_bytes` - Contract WASM, used only for the upload host function.
/// * `contract_id` - Contract ID (64-hex or `C…` strkey) to invoke. Required
///   when `function_name` is `Some` — `simulateTransaction` loads the
///   contract instance from the ledger, so a zeroed ID can never simulate.
/// * `function_name` - Function to invoke. `None` builds an upload operation.
/// * `args` - Positional `ScVal` arguments (already parsed from `--arg`).
pub fn build_simulation_tx_envelope(
    wasm_bytes: &[u8],
    contract_id: Option<&str>,
    function_name: Option<&str>,
    args: &[stellar_xdr::ScVal],
) -> AppResult<Vec<u8>> {
    use std::convert::TryInto;
    use stellar_xdr::VecM;

    let source = stellar_xdr::MuxedAccount::Ed25519(stellar_xdr::Uint256([0u8; 32]));

    let host_function = match function_name {
        Some(fn_name) => {
            let id_hex = contract_id.ok_or_else(|| {
                AppError::TxConstruction(
                    "contract id required for function invocation (pass --id <64-hex>)".to_string(),
                )
            })?;
            let id_bytes = parse_contract_id(id_hex)?;

            let fn_name_bytes: Vec<u8> = fn_name.as_bytes().to_vec();
            let sc_symbol = stellar_xdr::ScSymbol::try_from(fn_name_bytes).map_err(
                |e: stellar_xdr::Error| AppError::TxConstruction(format!("ScSymbol: {e}")),
            )?;

            let contract_id = stellar_xdr::ContractId(stellar_xdr::Hash(id_bytes));
            let sc_address = stellar_xdr::ScAddress::Contract(contract_id);

            let args_m: VecM<stellar_xdr::ScVal> = args
                .to_vec()
                .try_into()
                .map_err(|e| AppError::TxConstruction(format!("ScVal args: {e}")))?;

            stellar_xdr::HostFunction::InvokeContract(stellar_xdr::InvokeContractArgs {
                contract_address: sc_address,
                function_name: sc_symbol,
                args: args_m,
            })
        }
        None => {
            let wasm_vec: Vec<u8> = wasm_bytes.to_vec();
            let bytes_m: stellar_xdr::BytesM =
                wasm_vec.try_into().map_err(|e: stellar_xdr::Error| {
                    AppError::TxConstruction(format!("BytesM: {e}"))
                })?;
            stellar_xdr::HostFunction::UploadContractWasm(bytes_m)
        }
    };

    let invoke_op = stellar_xdr::InvokeHostFunctionOp {
        host_function,
        auth: VecM::<stellar_xdr::SorobanAuthorizationEntry>::default(),
    };

    let operation = stellar_xdr::Operation {
        source_account: None,
        body: stellar_xdr::OperationBody::InvokeHostFunction(invoke_op),
    };

    let operations: VecM<stellar_xdr::Operation, 100> =
        vec![operation]
            .try_into()
            .map_err(|e: stellar_xdr::Error| {
                AppError::TxConstruction(format!("VecM operations: {e}"))
            })?;

    let tx = stellar_xdr::Transaction {
        source_account: source,
        fee: 0,
        seq_num: stellar_xdr::SequenceNumber(0),
        cond: stellar_xdr::Preconditions::None,
        memo: stellar_xdr::Memo::None,
        operations,
        ext: stellar_xdr::TransactionExt::V0,
    };

    let tx_env = stellar_xdr::TransactionEnvelope::Tx(stellar_xdr::TransactionV1Envelope {
        tx,
        signatures: VecM::<stellar_xdr::DecoratedSignature, 20>::default(),
    });

    let xdr_bytes = tx_env
        .to_xdr(stellar_xdr::Limits::none())
        .map_err(|e| AppError::TxConstruction(format!("XDR encode: {e}")))?;

    Ok(xdr_bytes)
}

/// Parse a contract ID into a 32-byte array.
///
/// Accepts either a 64-hex-character contract ID or a `C…` strkey contract
/// ID (the format `stellar contract deploy` prints, e.g.
/// `CATFGUK…`).
pub fn parse_contract_id(id: &str) -> AppResult<[u8; 32]> {
    // Try hex first (64 hex chars = 32 bytes).
    if let Ok(decoded) = hex::decode(id) {
        if let Ok(bytes) = <[u8; 32]>::try_from(decoded) {
            return Ok(bytes);
        }
    }

    // Fall back to a strkey contract ID (`C…`, SEP-23) — the format the
    // Stellar CLI prints after `contract deploy`.
    let contract_id = id.parse::<stellar_xdr::ContractId>().map_err(|e| {
        AppError::TxConstruction(format!(
            "invalid contract id (expected 64 hex chars or a C… strkey): {e}"
        ))
    })?;
    Ok(contract_id.0.0)
}

/// Parse a single `--arg` value into an `ScVal` using type inference.
///
/// Accepts `key=value` (the key is informational and ignored) or a bare
/// `value`. Inference order: `true`/`false` → `Bool`, integer → `I64`,
/// non-negative integer that overflows `i64` → `U64`, anything else →
/// `String`. The inferred type drives the simulation's serialization size,
/// which is what the fee math actually depends on.
pub fn parse_arg_scval(arg: &str) -> stellar_xdr::ScVal {
    let value = arg.split_once('=').map(|(_, v)| v).unwrap_or(arg);

    match value {
        "true" => return stellar_xdr::ScVal::Bool(true),
        "false" => return stellar_xdr::ScVal::Bool(false),
        _ => {}
    }

    if let Ok(n) = value.parse::<i64>() {
        return stellar_xdr::ScVal::I64(n);
    }
    if let Ok(n) = value.parse::<u64>() {
        return stellar_xdr::ScVal::U64(n);
    }

    // Fall back to a string; an empty string is always a valid ScString.
    let sc_string: stellar_xdr::ScString =
        stellar_xdr::StringM::try_from(value.as_bytes().to_vec())
            .map(stellar_xdr::ScString::from)
            .unwrap_or_default();
    stellar_xdr::ScVal::String(sc_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        ConfigSettingContractBandwidthV0, ConfigSettingContractComputeV0,
        ConfigSettingContractEventsV0, ConfigSettingContractHistoricalDataV0,
        ConfigSettingContractLedgerCostV0, StateArchivalSettings,
    };

    #[test]
    fn test_begin_snapshot_defaults() {
        let snap = begin_snapshot("testnet", 42);
        assert_eq!(snap.network, "testnet");
        assert_eq!(snap.ledger, 42);
        assert!(snap.contract_compute.is_none());
        assert!(snap.contract_ledger_cost.is_none());
        assert!(snap.contract_historical_data.is_none());
        assert!(snap.contract_events.is_none());
        assert!(snap.contract_bandwidth.is_none());
        assert!(snap.state_archival.is_none());
        assert!(!snap.timestamp.is_empty());
    }

    #[test]
    fn test_apply_contract_compute() {
        let mut snap = begin_snapshot("test", 0);
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractComputeV0(ConfigSettingContractComputeV0 {
                ledger_max_instructions: 580_000_000,
                tx_max_instructions: 400_000_000,
                fee_rate_per_instructions_increment: 7,
                tx_memory_limit: 41_943_040,
            }),
        );

        let compute = snap.contract_compute.expect("compute should be set");
        assert_eq!(compute.ledger_max_instructions, 580_000_000);
        assert_eq!(compute.tx_max_instructions, 400_000_000);
        assert_eq!(compute.fee_rate_per_instructions_increment, 7);
        assert_eq!(compute.tx_memory_limit, 41_943_040);
    }

    #[test]
    fn test_apply_contract_bandwidth() {
        let mut snap = begin_snapshot("test", 0);
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(
                ConfigSettingContractBandwidthV0 {
                    ledger_max_txs_size_bytes: 266_240,
                    tx_max_size_bytes: 132_096,
                    fee_tx_size1_kb: 406,
                },
            ),
        );

        let bw = snap.contract_bandwidth.expect("bandwidth should be set");
        assert_eq!(bw.ledger_max_txs_size_bytes, 266_240);
        assert_eq!(bw.fee_tx_size1_kb, 406);
    }

    #[test]
    fn test_parse_contract_id_hex() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let bytes = parse_contract_id(hex_id).expect("hex id should parse");
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[31], 0xef);
    }

    #[test]
    fn test_parse_contract_id_strkey() {
        // A real contract ID as printed by `stellar contract deploy` (SEP-23 `C…`).
        let strkey = "CATFGUK47IOTMS4IQQDHUY3UEEJF5EUHDP3M4UYQW4G6IVSTF57JTGOK";
        let bytes = parse_contract_id(strkey).expect("strkey id should parse");
        assert_eq!(bytes.len(), 32);
        // Decoding must round-trip back to the same strkey.
        let contract_id = stellar_xdr::ContractId(stellar_xdr::Hash(bytes));
        assert_eq!(contract_id.to_string(), strkey);
    }

    #[test]
    fn test_parse_contract_id_invalid() {
        assert!(parse_contract_id("not-an-id").is_err());
        assert!(
            parse_contract_id("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_err()
        );
    }

    #[test]
    fn test_apply_all_six_config_types() {
        let mut snap = begin_snapshot("test", 0);

        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractComputeV0(ConfigSettingContractComputeV0 {
                ledger_max_instructions: 1,
                tx_max_instructions: 2,
                fee_rate_per_instructions_increment: 3,
                tx_memory_limit: 4,
            }),
        );
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractLedgerCostV0(
                ConfigSettingContractLedgerCostV0 {
                    ledger_max_disk_read_entries: 10,
                    ledger_max_disk_read_bytes: 11,
                    ledger_max_write_ledger_entries: 12,
                    ledger_max_write_bytes: 13,
                    tx_max_disk_read_entries: 14,
                    tx_max_disk_read_bytes: 15,
                    tx_max_write_ledger_entries: 16,
                    tx_max_write_bytes: 17,
                    fee_disk_read_ledger_entry: 18,
                    fee_write_ledger_entry: 19,
                    fee_disk_read1_kb: 20,
                    soroban_state_target_size_bytes: 21,
                    rent_fee1_kb_soroban_state_size_low: 22,
                    rent_fee1_kb_soroban_state_size_high: 23,
                    soroban_state_rent_fee_growth_factor: 24,
                },
            ),
        );
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractHistoricalDataV0(
                ConfigSettingContractHistoricalDataV0 {
                    fee_historical1_kb: 30,
                },
            ),
        );
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractEventsV0(ConfigSettingContractEventsV0 {
                tx_max_contract_events_size_bytes: 40,
                fee_contract_events1_kb: 41,
            }),
        );
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::ContractBandwidthV0(
                ConfigSettingContractBandwidthV0 {
                    ledger_max_txs_size_bytes: 50,
                    tx_max_size_bytes: 51,
                    fee_tx_size1_kb: 52,
                },
            ),
        );
        apply_config_entry(
            &mut snap,
            stellar_xdr::ConfigSettingEntry::StateArchival(StateArchivalSettings {
                max_entry_ttl: 60,
                min_temporary_ttl: 61,
                min_persistent_ttl: 62,
                persistent_rent_rate_denominator: 63,
                temp_rent_rate_denominator: 64,
                max_entries_to_archive: 65,
                live_soroban_state_size_window_sample_size: 66,
                live_soroban_state_size_window_sample_period: 67,
                eviction_scan_size: 68,
                starting_eviction_scan_level: 69,
            }),
        );

        assert!(snap.contract_compute.is_some());
        assert!(snap.contract_ledger_cost.is_some());
        assert!(snap.contract_historical_data.is_some());
        assert!(snap.contract_events.is_some());
        assert!(snap.contract_bandwidth.is_some());
        assert!(snap.state_archival.is_some());
    }
}
