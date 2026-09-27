//! Helpers for encoding/decoding Soroban XDR types using `stellar_xdr`.

use stellar_xdr::ReadXdr;
use stellar_xdr::WriteXdr;

use crate::config_snapshot::model::ConfigSnapshot;
use crate::error::{AppError, AppResult};
use crate::wasm::parser::FunctionInfo;

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
#[must_use]
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

/// Validates `--arg` values against the contract-spec types for the function
/// being simulated, before any RPC call is made.
///
/// When the WASM carries a `contractspecv0` and a matching function is found
/// with typed parameters, each `--arg` value must match the declared type
/// (e.g. `abc` is rejected for `i64`). Mismatches fail fast so a bad argument
/// never reaches the network. Only the target function's declared parameters
/// are checked: extra or missing arguments are a mismatch as well.
///
/// Errors name the parameter, its expected type, and the provided value,
/// e.g. `argument 'step': expected i64 but got 'abc'`.
///
/// * `function_name` - `None` (WASM upload) or a function with no spec params
///   skips validation entirely.
/// * `args` - Raw `--arg KEY=VAL` entries as passed on the CLI.
/// * `functions` - Enumerated functions with spec-derived params.
pub fn validate_args_against_spec(
    function_name: Option<&str>,
    args: &[String],
    functions: &[FunctionInfo],
) -> AppResult<()> {
    let Some(fn_name) = function_name else {
        return Ok(());
    };
    let Some(fn_info) = functions.iter().find(|f| f.name == fn_name) else {
        return Ok(());
    };
    if fn_info.params.is_empty() {
        return Ok(());
    }

    if args.len() != fn_info.params.len() {
        let decl = fn_info
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.type_name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::TypeValidation(format!(
            "function '{fn_name}' expects {} argument(s) ({decl}), but {} were provided",
            fn_info.params.len(),
            args.len()
        )));
    }

    for (arg, param) in args.iter().zip(&fn_info.params) {
        let value = arg.split_once('=').map(|(_, v)| v).unwrap_or(arg);
        if crate::wasm::parser::validate_arg_value(&param.type_def, arg).is_err() {
            return Err(AppError::TypeValidation(format!(
                "argument '{}': expected {} but got '{value}'",
                param.name, param.type_name
            )));
        }
    }
    Ok(())
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
#[must_use]
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

/// Coerces one `--arg` value to the `ScVal` dictated by the contract spec.
///
/// Unlike [`parse_arg_scval`] (heuristic inference), the declared
/// `ScSpecTypeDef` is authoritative here: a value that cannot represent the
/// type fails with a [`AppError::TypeValidation`] naming the parameter, the
/// expected type, and the provided value, e.g.
/// `argument 'step': expected i64 but got 'abc'`.
///
/// Supported primitives: `bool`, `i32`, `u32`, `i64`, `u64` (plus
/// `timepoint`/`duration` as `u64`), `i128`, `u128`, `i256`, `u256`,
/// `bytes`, `bytes_n`, `string`, `symbol`, `address` (strkey `G…`/`C…` or
/// 64-hex contract id) and `muxed_address`. Compound or user-defined types
/// (`option`, `result`, `vec`, `map`, `tuple`, `udt`, `val`, `void`, `error`)
/// are out of scope and fall back to heuristic inference.
///
/// * `param_name` - Spec parameter name, used only for error context.
/// * `value` - Bare argument value (already stripped of any `key=` prefix).
/// * `type_def` - Declared spec type for this parameter.
pub fn parse_arg_with_spec(
    param_name: &str,
    value: &str,
    type_def: &stellar_xdr::ScSpecTypeDef,
) -> AppResult<stellar_xdr::ScVal> {
    use stellar_xdr::ScSpecTypeDef as T;
    let expected = crate::wasm::parser::spec_type_name(type_def);
    let mismatch = |detail: &str| {
        AppError::TypeValidation(format!(
            "argument '{param_name}': expected {expected} but got '{value}' ({detail})"
        ))
    };

    match type_def {
        T::Bool => match value {
            "true" => Ok(stellar_xdr::ScVal::Bool(true)),
            "false" => Ok(stellar_xdr::ScVal::Bool(false)),
            _ => Err(mismatch("expected \"true\" or \"false\"")),
        },
        T::U32 => value
            .parse::<u32>()
            .map(stellar_xdr::ScVal::U32)
            .map_err(|_| mismatch("invalid u32 integer")),
        T::I32 => value
            .parse::<i32>()
            .map(stellar_xdr::ScVal::I32)
            .map_err(|_| mismatch("invalid i32 integer")),
        T::U64 => value
            .parse::<u64>()
            .map(stellar_xdr::ScVal::U64)
            .map_err(|_| mismatch("invalid u64 integer")),
        T::I64 => value
            .parse::<i64>()
            .map(stellar_xdr::ScVal::I64)
            .map_err(|_| mismatch("invalid i64 integer")),
        T::Timepoint => value.parse::<u64>().map_or_else(
            |_| Err(mismatch("invalid timepoint integer")),
            |n| Ok(stellar_xdr::ScVal::Timepoint(stellar_xdr::TimePoint(n))),
        ),
        T::Duration => value.parse::<u64>().map_or_else(
            |_| Err(mismatch("invalid duration integer")),
            |n| Ok(stellar_xdr::ScVal::Duration(stellar_xdr::Duration(n))),
        ),
        T::U128 => value.parse::<u128>().map_or_else(
            |_| Err(mismatch("invalid u128 integer")),
            |n| Ok(stellar_xdr::ScVal::from(n)),
        ),
        T::I128 => value.parse::<i128>().map_or_else(
            |_| Err(mismatch("invalid i128 integer")),
            |n| Ok(stellar_xdr::ScVal::from(n)),
        ),
        T::U256 => parse_u256_parts(value)
            .map(stellar_xdr::ScVal::U256)
            .map_err(|_| mismatch("invalid u256 integer")),
        T::I256 => parse_i256_parts(value)
            .map(stellar_xdr::ScVal::I256)
            .map_err(|_| mismatch("invalid i256 integer")),
        T::Bytes => {
            let raw = decode_hex_bytes(value).map_err(|_| mismatch("invalid hex bytes"))?;
            let bytes_m: stellar_xdr::BytesM = raw
                .try_into()
                .map_err(|_| mismatch("bytes value too long"))?;
            Ok(stellar_xdr::ScVal::Bytes(stellar_xdr::ScBytes(bytes_m)))
        }
        T::BytesN(spec) => {
            let raw = decode_hex_bytes(value).map_err(|_| mismatch("invalid hex bytes"))?;
            let want = usize::try_from(spec.n).map_err(|_| mismatch("invalid bytes_n length"))?;
            if raw.len() != want {
                return Err(mismatch("wrong byte length"));
            }
            let bytes_m: stellar_xdr::BytesM = raw
                .try_into()
                .map_err(|_| mismatch("bytes value too long"))?;
            Ok(stellar_xdr::ScVal::Bytes(stellar_xdr::ScBytes(bytes_m)))
        }
        T::String => {
            let string_m: stellar_xdr::StringM = value
                .as_bytes()
                .to_vec()
                .try_into()
                .map_err(|_| mismatch("string value too long"))?;
            Ok(stellar_xdr::ScVal::String(stellar_xdr::ScString(string_m)))
        }
        T::Symbol => {
            if !crate::wasm::parser::is_valid_symbol(value) {
                return Err(mismatch("invalid symbol"));
            }
            let symbol = stellar_xdr::ScSymbol::try_from(value.as_bytes().to_vec())
                .map_err(|_| mismatch("invalid symbol"))?;
            Ok(stellar_xdr::ScVal::Symbol(symbol))
        }
        T::Address | T::MuxedAddress => {
            let addr = parse_sc_address(value).map_err(|_| mismatch("invalid address"))?;
            Ok(stellar_xdr::ScVal::Address(addr))
        }
        // Compound / user-defined types are out of scope: fall back to
        // heuristic inference so existing behaviour is preserved.
        T::Val
        | T::Void
        | T::Error
        | T::Option(_)
        | T::Result(_)
        | T::Vec(_)
        | T::Map(_)
        | T::Tuple(_)
        | T::Udt(_) => Ok(parse_arg_scval(value)),
    }
}

/// Parses every `--arg` for a function call, coercing each value to its
/// spec-declared type when a contract spec is available.
///
/// Positional mapping is used: `args[i]` is coerced to `params[i]`. Each
/// entry may be `key=value` (key informational, ignored) or a bare value.
/// When no spec applies (`function_name` is `None`, the function is unknown,
/// or it declares no typed params) every entry falls back to heuristic
/// [`parse_arg_scval`] inference.
///
/// Arity mismatches fail with a `TypeValidation` error naming the function,
/// its declared signature, and the provided count — before any RPC traffic.
pub fn parse_args_with_spec(
    function_name: Option<&str>,
    args: &[String],
    functions: &[FunctionInfo],
) -> AppResult<Vec<stellar_xdr::ScVal>> {
    let Some(fn_name) = function_name else {
        return Ok(args.iter().map(|a| parse_arg_scval(a)).collect());
    };
    let Some(fn_info) = functions.iter().find(|f| f.name == fn_name) else {
        return Ok(args.iter().map(|a| parse_arg_scval(a)).collect());
    };
    if fn_info.params.is_empty() {
        return Ok(args.iter().map(|a| parse_arg_scval(a)).collect());
    }

    if args.len() != fn_info.params.len() {
        let decl = fn_info
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.type_name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::TypeValidation(format!(
            "function '{fn_name}' expects {} argument(s) ({decl}), but {} were provided",
            fn_info.params.len(),
            args.len()
        )));
    }

    args.iter()
        .zip(&fn_info.params)
        .map(|(arg, param)| {
            let value = arg.split_once('=').map(|(_, v)| v).unwrap_or(arg);
            parse_arg_with_spec(&param.name, value, &param.type_def)
        })
        .collect()
}

/// Decodes a `0x`-prefixed or bare hex string into raw bytes.
fn decode_hex_bytes(value: &str) -> Result<Vec<u8>, AppError> {
    let stripped = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    hex::decode(stripped).map_err(|e| AppError::TypeValidation(format!("invalid hex: {e}")))
}

/// Parses a strkey (`G…`/`C…`/…) or 64-hex contract id into an `ScAddress`.
fn parse_sc_address(value: &str) -> AppResult<stellar_xdr::ScAddress> {
    if let Ok(addr) = value.parse::<stellar_xdr::ScAddress>() {
        return Ok(addr);
    }
    if let Ok(decoded) = hex::decode(value) {
        if let Ok(bytes) = <[u8; 32]>::try_from(decoded) {
            let contract = stellar_xdr::ContractId(stellar_xdr::Hash(bytes));
            return Ok(stellar_xdr::ScAddress::Contract(contract));
        }
    }
    Err(AppError::TypeValidation(format!(
        "invalid address '{value}'"
    )))
}

/// Parses a `u256` decimal or `0x` hex string into `UInt256Parts`.
fn parse_u256_parts(value: &str) -> Result<stellar_xdr::UInt256Parts, AppError> {
    let limbs = parse_u256_limbs(value)?;
    Ok(stellar_xdr::UInt256Parts {
        hi_hi: limbs[3],
        hi_lo: limbs[2],
        lo_hi: limbs[1],
        lo_lo: limbs[0],
    })
}

/// Parses an `i256` decimal or `0x` hex string into `Int256Parts`.
///
/// Range is the signed 256-bit range `-2^255..=2^255-1`.
fn parse_i256_parts(value: &str) -> Result<stellar_xdr::Int256Parts, AppError> {
    let (negative, magnitude) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    if magnitude.is_empty() {
        return Err(AppError::TypeValidation("empty i256 value".to_string()));
    }
    let limbs = parse_u256_limbs(magnitude)?;
    // 2^255 as limbs (LE): only the top bit of limbs[3] set.
    let is_zero = limbs.iter().all(|&l| l == 0);
    if negative {
        if is_zero {
            return Ok(stellar_xdr::Int256Parts {
                hi_hi: 0,
                hi_lo: 0,
                lo_hi: 0,
                lo_lo: 0,
            });
        }
        let over = limbs[3] > 0x8000_0000_0000_0000
            || (limbs[3] == 0x8000_0000_0000_0000 && limbs[..3].iter().any(|&l| l != 0));
        if over {
            return Err(AppError::TypeValidation(
                "i256 value out of range".to_string(),
            ));
        }
        // Two's complement negate: !limbs + 1.
        let mut neg = [
            limbs[0] ^ u64::MAX,
            limbs[1] ^ u64::MAX,
            limbs[2] ^ u64::MAX,
            limbs[3] ^ u64::MAX,
        ];
        let mut carry: u128 = 1;
        for limb in &mut neg {
            let sum = u128::from(*limb) + carry;
            *limb = sum as u64;
            carry = sum >> 64;
        }
        Ok(stellar_xdr::Int256Parts {
            hi_hi: neg[3] as i64,
            hi_lo: neg[2],
            lo_hi: neg[1],
            lo_lo: neg[0],
        })
    } else {
        if limbs[3] >= 0x8000_0000_0000_0000 {
            return Err(AppError::TypeValidation(
                "i256 value out of range".to_string(),
            ));
        }
        #[allow(clippy::cast_possible_wrap)]
        let hi_hi = limbs[3] as i64;
        Ok(stellar_xdr::Int256Parts {
            hi_hi,
            hi_lo: limbs[2],
            lo_hi: limbs[1],
            lo_lo: limbs[0],
        })
    }
}

/// Parses a `u256` magnitude (decimal or `0x` hex) into little-endian limbs.
///
/// Returns `[lo_lo, lo_hi, hi_lo, hi_hi]`. Integer-only arithmetic throughout.
fn parse_u256_limbs(value: &str) -> Result<[u64; 4], AppError> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return parse_u256_hex_limbs(hex);
    }
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::TypeValidation(format!(
            "invalid u256 integer '{value}'"
        )));
    }
    let mut limbs = [0_u64; 4];
    for digit_byte in value.bytes() {
        let digit = u64::from(digit_byte - b'0');
        // limbs = limbs * 10 + digit, base 2^64 with carry.
        let mut carry: u128 = u128::from(digit);
        for limb in &mut limbs {
            let acc = u128::from(*limb) * 10 + carry;
            *limb = acc as u64;
            carry = acc >> 64;
        }
        if carry != 0 {
            return Err(AppError::TypeValidation(format!(
                "u256 value out of range '{value}'"
            )));
        }
    }
    Ok(limbs)
}

/// Parses bare hex digits (no `0x` prefix) into little-endian `u256` limbs.
fn parse_u256_hex_limbs(hex: &str) -> Result<[u64; 4], AppError> {
    if hex.is_empty() || hex.len() > 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::TypeValidation(format!(
            "invalid u256 hex '{hex}'"
        )));
    }
    let padded = format!("{hex:0>64}");
    let mut limbs_be = [0_u64; 4];
    for (i, chunk) in padded.as_bytes().chunks(16).enumerate() {
        let s = core::str::from_utf8(chunk)
            .map_err(|_| AppError::TypeValidation("invalid u256 hex".to_string()))?;
        limbs_be[i] = u64::from_str_radix(s, 16)
            .map_err(|_| AppError::TypeValidation("invalid u256 hex".to_string()))?;
    }
    // Big-endian [hi_hi, hi_lo, lo_hi, lo_lo] -> little-endian reverse.
    Ok([limbs_be[3], limbs_be[2], limbs_be[1], limbs_be[0]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        ConfigSettingContractBandwidthV0, ConfigSettingContractComputeV0,
        ConfigSettingContractEventsV0, ConfigSettingContractHistoricalDataV0,
        ConfigSettingContractLedgerCostV0, StateArchivalSettings,
    };

    fn increment_function() -> FunctionInfo {
        FunctionInfo {
            name: "increment".to_string(),
            param_count: 2,
            result_count: 1,
            params: vec![
                crate::wasm::parser::ParamInfo {
                    name: "step".to_string(),
                    type_name: "i64".to_string(),
                    type_def: stellar_xdr::ScSpecTypeDef::I64,
                },
                crate::wasm::parser::ParamInfo {
                    name: "label".to_string(),
                    type_name: "symbol".to_string(),
                    type_def: stellar_xdr::ScSpecTypeDef::Symbol,
                },
            ],
        }
    }

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

    #[test]
    fn test_validate_args_against_spec_ok() {
        let functions = vec![increment_function()];
        let args = vec!["step=3".to_string(), "player_1".to_string()];
        let result = validate_args_against_spec(Some("increment"), &args, &functions);
        assert!(result.is_ok(), "expected valid args to pass: {result:?}");
    }

    #[test]
    fn test_validate_args_against_spec_type_mismatch() {
        let functions = vec![increment_function()];
        let args = vec!["step=abc".to_string(), "player_1".to_string()];
        let err = validate_args_against_spec(Some("increment"), &args, &functions)
            .expect_err("abc for i64 must fail");
        assert!(err.to_string().contains("i64"), "got: {err}");
    }

    #[test]
    fn test_validate_args_against_spec_arity_mismatch() {
        let functions = vec![increment_function()];
        let args = vec!["step=3".to_string()];
        let err = validate_args_against_spec(Some("increment"), &args, &functions)
            .expect_err("arity mismatch must fail");
        assert!(
            err.to_string().contains("expects 2 argument(s)"),
            "got: {err}"
        );
    }

    #[test]
    fn test_validate_args_skipped_without_spec() {
        let no_spec = FunctionInfo {
            name: "plain".to_string(),
            param_count: 1,
            result_count: 1,
            params: vec![],
        };
        let result =
            validate_args_against_spec(Some("plain"), &["anything".to_string()], &[no_spec]);
        assert!(result.is_ok(), "no spec params means nothing to validate");
    }

    #[test]
    fn test_validate_args_skipped_for_uploads() {
        let result = validate_args_against_spec(None, &[], &[]);
        assert!(result.is_ok(), "upload path skips validation");
    }

    #[allow(clippy::needless_pass_by_value)]
    fn spec_val(
        param: &str,
        value: &str,
        type_def: stellar_xdr::ScSpecTypeDef,
    ) -> Result<stellar_xdr::ScVal, crate::error::AppError> {
        parse_arg_with_spec(param, value, &type_def)
    }

    #[test]
    fn test_parse_arg_with_spec_bool() {
        assert_eq!(
            spec_val("flag", "true", stellar_xdr::ScSpecTypeDef::Bool).expect("true"),
            stellar_xdr::ScVal::Bool(true)
        );
        assert_eq!(
            spec_val("flag", "false", stellar_xdr::ScSpecTypeDef::Bool).expect("false"),
            stellar_xdr::ScVal::Bool(false)
        );
        let err = spec_val("flag", "yes", stellar_xdr::ScSpecTypeDef::Bool).expect_err("yes");
        let msg = err.to_string();
        assert!(
            msg.contains("flag") && msg.contains("bool") && msg.contains("yes"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_arg_with_spec_ints() {
        assert_eq!(
            spec_val("a", "-12", stellar_xdr::ScSpecTypeDef::I32).expect("i32"),
            stellar_xdr::ScVal::I32(-12)
        );
        assert_eq!(
            spec_val("a", "12", stellar_xdr::ScSpecTypeDef::U32).expect("u32"),
            stellar_xdr::ScVal::U32(12)
        );
        assert_eq!(
            spec_val("a", "-7", stellar_xdr::ScSpecTypeDef::I64).expect("i64"),
            stellar_xdr::ScVal::I64(-7)
        );
        assert_eq!(
            spec_val("a", "7", stellar_xdr::ScSpecTypeDef::U64).expect("u64"),
            stellar_xdr::ScVal::U64(7)
        );
        assert!(spec_val("a", "abc", stellar_xdr::ScSpecTypeDef::I64).is_err());
        assert!(spec_val("a", "-1", stellar_xdr::ScSpecTypeDef::U32).is_err());
        assert!(spec_val("a", "4294967296", stellar_xdr::ScSpecTypeDef::U32).is_err());
    }

    #[test]
    fn test_parse_arg_with_spec_128() {
        let v = spec_val(
            "n",
            "340282366920938463463374607431768211455",
            stellar_xdr::ScSpecTypeDef::U128,
        )
        .expect("u128 max");
        assert_eq!(v, stellar_xdr::ScVal::from(u128::MAX));
        let v = spec_val(
            "n",
            "-170141183460469231731687303715884105728",
            stellar_xdr::ScSpecTypeDef::I128,
        )
        .expect("i128 min");
        assert_eq!(v, stellar_xdr::ScVal::from(i128::MIN));
        assert!(spec_val("n", "abc", stellar_xdr::ScSpecTypeDef::U128).is_err());
        assert!(
            spec_val(
                "n",
                "340282366920938463463374607431768211456",
                stellar_xdr::ScSpecTypeDef::U128
            )
            .is_err()
        );
    }

    #[test]
    fn test_parse_arg_with_spec_timepoint_duration() {
        assert_eq!(
            spec_val("t", "100", stellar_xdr::ScSpecTypeDef::Timepoint).expect("timepoint"),
            stellar_xdr::ScVal::Timepoint(stellar_xdr::TimePoint(100))
        );
        assert_eq!(
            spec_val("d", "60", stellar_xdr::ScSpecTypeDef::Duration).expect("duration"),
            stellar_xdr::ScVal::Duration(stellar_xdr::Duration(60))
        );
        assert!(spec_val("t", "-1", stellar_xdr::ScSpecTypeDef::Timepoint).is_err());
    }

    #[test]
    fn test_parse_arg_with_spec_symbol_string() {
        // A bare numeric-looking value coerces to Symbol when the spec says so.
        let v = spec_val("label", "123", stellar_xdr::ScSpecTypeDef::Symbol).expect("symbol");
        assert!(matches!(v, stellar_xdr::ScVal::Symbol(_)));
        // Same input coerces to String when the spec says string.
        let v = spec_val("name", "123", stellar_xdr::ScSpecTypeDef::String).expect("string");
        assert!(matches!(v, stellar_xdr::ScVal::String(_)));
        let err = spec_val("label", "a b", stellar_xdr::ScSpecTypeDef::Symbol).expect_err("space");
        let msg = err.to_string();
        assert!(
            msg.contains("label") && msg.contains("symbol") && msg.contains("a b"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_arg_with_spec_address() {
        let hex_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let v = spec_val("to", hex_id, stellar_xdr::ScSpecTypeDef::Address).expect("hex addr");
        assert!(matches!(v, stellar_xdr::ScVal::Address(_)));
        let strkey = "CATFGUK47IOTMS4IQQDHUY3UEEJF5EUHDP3M4UYQW4G6IVSTF57JTGOK";
        let v = spec_val("to", strkey, stellar_xdr::ScSpecTypeDef::Address).expect("strkey");
        assert!(matches!(v, stellar_xdr::ScVal::Address(_)));
        let err = spec_val("to", "not-an-address", stellar_xdr::ScSpecTypeDef::Address)
            .expect_err("bad addr");
        let msg = err.to_string();
        assert!(
            msg.contains("to") && msg.contains("address") && msg.contains("not-an-address"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_arg_with_spec_bytes_and_wide() {
        let v = spec_val("b", "0x00ff", stellar_xdr::ScSpecTypeDef::Bytes).expect("bytes");
        assert!(matches!(v, stellar_xdr::ScVal::Bytes(_)));
        let two = stellar_xdr::ScSpecTypeDef::BytesN(stellar_xdr::ScSpecTypeBytesN { n: 2 });
        assert!(spec_val("h", "0x00ff", two.clone()).is_ok());
        assert!(spec_val("h", "0x00", two).is_err());
        let v = spec_val("w", "0x1ff", stellar_xdr::ScSpecTypeDef::U256).expect("u256");
        assert!(matches!(v, stellar_xdr::ScVal::U256(_)));
        assert!(spec_val("w", "abc", stellar_xdr::ScSpecTypeDef::U256).is_err());
        let v = spec_val("w", "-42", stellar_xdr::ScSpecTypeDef::I256).expect("i256");
        assert!(matches!(v, stellar_xdr::ScVal::I256(_)));
    }

    #[test]
    fn test_parse_args_with_spec_coerces_positionally() {
        let functions = vec![increment_function()];
        let args = vec!["step=3".to_string(), "label=123".to_string()];
        let vals = parse_args_with_spec(Some("increment"), &args, &functions).expect("coerce");
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0], stellar_xdr::ScVal::I64(3));
        assert!(matches!(vals[1], stellar_xdr::ScVal::Symbol(_)));
    }

    #[test]
    fn test_parse_args_with_spec_error_names_param() {
        let functions = vec![increment_function()];
        let args = vec!["step=abc".to_string(), "player_1".to_string()];
        let err =
            parse_args_with_spec(Some("increment"), &args, &functions).expect_err("must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("step") && msg.contains("i64") && msg.contains("abc"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_args_with_spec_falls_back_without_spec() {
        let vals = parse_args_with_spec(None, &["42".to_string()], &[]).expect("fallback");
        assert_eq!(vals, vec![stellar_xdr::ScVal::I64(42)]);
        let no_spec = FunctionInfo {
            name: "plain".to_string(),
            param_count: 1,
            result_count: 1,
            params: vec![],
        };
        let vals = parse_args_with_spec(
            Some("plain"),
            &["true".to_string()],
            core::slice::from_ref(&no_spec),
        )
        .expect("fallback");
        assert_eq!(vals, vec![stellar_xdr::ScVal::Bool(true)]);
    }
}
