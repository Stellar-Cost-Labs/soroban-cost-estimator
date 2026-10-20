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
pub fn decode_config_entry_xdr(
    xdr_b64: &str,
    verbose: bool,
) -> AppResult<stellar_xdr::ConfigSettingEntry> {
    if verbose {
        eprintln!(
            "[XDR] Decoding config entry from base64 ({} chars)",
            xdr_b64.len()
        );
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, xdr_b64)
        .map_err(|e| AppError::XdrDecode(format!("invalid base64: {e}")))?;

    if verbose {
        eprintln!("[XDR] -> Decoded to {} bytes", bytes.len());
    }

    let entry_data = stellar_xdr::LedgerEntryData::from_xdr(&bytes, stellar_xdr::Limits::none())
        .map_err(|e| AppError::XdrDecode(format!("ledger entry data: {e}")))?;

    if verbose {
        eprintln!(
            "[XDR] -> Parsed LedgerEntryData variant: {}",
            entry_data.name()
        );
    }

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
        protocol_version: None,
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
                |e: stellar_xdr::Error| {
                    AppError::TxConstruction(format!("invalid function symbol '{fn_name}': {e}"))
                },
            )?;

            let contract_id = stellar_xdr::ContractId(stellar_xdr::Hash(id_bytes));
            let sc_address = stellar_xdr::ScAddress::Contract(contract_id);

            let args_m: VecM<stellar_xdr::ScVal> = args
                .to_vec()
                .try_into()
                .map_err(|e| AppError::TxConstruction(format!("invalid argument list: {e}")))?;

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
                    AppError::TxConstruction(format!("invalid WASM upload payload: {e}"))
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
                AppError::TxConstruction(format!("invalid operation list: {e}"))
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
        .map_err(|e| AppError::XdrEncode(format!("transaction envelope: {e}")))?;

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
        crate::wasm::parser::validate_arg_value(&param.type_def, arg)?;
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

// ─────────────────────────────────────────────────────────────────────────
// Typed invocation arguments from a JSON file (#486)
// ─────────────────────────────────────────────────────────────────────────

/// One typed invocation argument parsed from a `--args-file` JSON document.
///
/// Unlike `--arg KEY=VALUE` strings — which are type-inferred at simulation
/// time — a `TypedArg` carries an explicitly declared Soroban type and its
/// XDR value, already converted and structurally validated. Validation of
/// parameter names and types against the contract spec happens separately in
/// [`validate_typed_args_against_spec`], once the WASM spec is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedArg {
    /// Optional parameter name from the JSON document; validated against the
    /// contract spec when one is available.
    pub name: Option<String>,
    /// The canonical type name as written in the JSON document (`"i64"`,
    /// `"symbol"`, `"vec"`, …).
    pub type_name: String,
    /// The converted value.
    pub value: stellar_xdr::ScVal,
}

/// The `type` names accepted in a `--args-file` document (issue #486).
///
/// The format is deliberately explicit and small: every value names its
/// Soroban type, nothing is inferred, and types whose XDR shape is ambiguous
/// in JSON (options, results, tuples, UDTs, 256-bit integers) are rejected
/// with a clear error until a later version defines them. `--arg KEY=VALUE`
/// remains available for those.
pub const SUPPORTED_ARG_FILE_TYPES: &[&str] = &[
    "bool",
    "u32",
    "i32",
    "u64",
    "i64",
    "u128",
    "i128",
    "timepoint",
    "duration",
    "string",
    "symbol",
    "address",
    "bytes",
    "vec",
    "map",
];

/// Serde model of the whole `--args-file` document.
///
/// Unknown fields are rejected everywhere so a typo (`"typo": "i64"`) is an
/// error rather than a silently ignored declaration.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ArgsFileDocument {
    args: Vec<ArgsFileArg>,
}

/// Serde model of one top-level argument entry.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ArgsFileArg {
    /// Optional parameter name (top-level entries only).
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    type_name: String,
    value: serde_json::Value,
}

/// Serde model of a nested value (a `vec` element or a `map` key/value):
/// identical to [`ArgsFileArg`] minus the `name` field, which is only
/// meaningful for a function parameter.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NestedArg {
    #[serde(rename = "type")]
    type_name: String,
    value: serde_json::Value,
}

/// Serde model of one `map` entry: an explicit `{"key": …, "value": …}`
/// pair of nested typed values.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ArgsFileMapEntry {
    key: NestedArg,
    value: NestedArg,
}

/// Read and parse a `--args-file` JSON document into typed arguments.
///
/// File I/O problems surface as [`AppError::FileNotFound`] / [`AppError::Io`]
/// (matching [`crate::wasm::parser::load_wasm`]); malformed JSON, unknown
/// fields, unsupported types, and structurally invalid values surface as
/// [`AppError::ArgsFile`] with the failing document position. Every value is
/// converted to XDR here, so nothing downstream re-interprets JSON.
///
/// # Network calls
/// None — pure file I/O + parsing.
pub fn parse_args_file(path: &std::path::Path) -> AppResult<Vec<TypedArg>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(path.display().to_string())
        } else {
            AppError::Io(e)
        }
    })?;
    let doc: ArgsFileDocument = serde_json::from_str(&content)
        .map_err(|e| AppError::ArgsFile(format!("{}: {e}", path.display())))?;
    doc.args
        .into_iter()
        .enumerate()
        .map(|(i, arg)| {
            let value = json_to_scval(&arg.type_name, &arg.value, &format!("args[{i}]"))?;
            Ok(TypedArg {
                name: arg.name,
                type_name: arg.type_name,
                value,
            })
        })
        .collect()
}

/// Canonical, deterministic string forms of typed arguments, used only as
/// estimate-cache key material.
///
/// Each entry is the declared type name plus the value's XDR bytes (hex), so
/// distinct values — including strings full of shell metacharacters — never
/// collide and never need escaping.
///
/// # Errors
/// Only if the value cannot be XDR-encoded (should not happen for values that
/// passed [`parse_args_file`]).
pub fn typed_cache_args(args: &[TypedArg]) -> AppResult<Vec<String>> {
    args.iter()
        .map(|arg| {
            let bytes = arg
                .value
                .to_xdr(stellar_xdr::Limits::none())
                .map_err(|e| {
                    AppError::XdrEncode(format!(
                        "cache key for '{}' argument: {e}",
                        arg.type_name
                    ))
                })?;
            Ok(format!("{}:{}", arg.type_name, hex::encode(bytes)))
        })
        .collect()
}

/// Build the error for a `type` name the v1 format does not support.
fn unsupported_type(at: &str, type_name: &str) -> AppError {
    AppError::ArgsFile(format!(
        "{at}: unsupported type '{type_name}' (supported types: {})",
        SUPPORTED_ARG_FILE_TYPES.join(", ")
    ))
}

/// Shape error for a value that does not match its declared type.
fn wrong_shape(at: &str, type_name: &str, found: &serde_json::Value, wanted: &str) -> AppError {
    let kind = match found {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    };
    AppError::ArgsFile(format!(
        "{at}: type '{type_name}' expects {wanted}, found {kind}"
    ))
}

/// Parse a JSON integer token without ever routing through floating point.
///
/// `serde_json` stores integer tokens as `u64`/`i64` and floats as `f64`, so
/// asking for the integer view rejects `5.0`, `1e3`, and any token that
/// overflowed 64 bits (which serde_json keeps as a lossy float) with a clear
/// message instead of silently truncating.
fn json_integer_u64(
    value: &serde_json::Value,
    at: &str,
    type_name: &str,
) -> AppResult<u64> {
    value.as_u64().ok_or_else(|| {
        wrong_shape(
            at,
            type_name,
            value,
            "a non-negative integer written as a JSON number (no floats, no strings)",
        )
    })
}

/// Integer view of a signed JSON token (`u64` view first so positive values
/// above `i64::MAX` are rejected as out of range rather than wrapped).
fn json_integer_i64(value: &serde_json::Value, at: &str, type_name: &str) -> AppResult<i64> {
    value.as_i64().ok_or_else(|| {
        wrong_shape(
            at,
            type_name,
            value,
            "an integer written as a JSON number (no floats, no strings)",
        )
    })
}

/// Decimal-string integer for the 128-bit types.
///
/// JSON numbers above 64 bits are kept by `serde_json` as lossy floats, so
/// `u128`/`i128` values must be written as decimal strings to preserve every
/// digit exactly.
fn decimal_string_integer<T: std::str::FromStr>(
    value: &serde_json::Value,
    at: &str,
    type_name: &str,
) -> AppResult<T> {
    let text = value.as_str().ok_or_else(|| {
        wrong_shape(
            at,
            type_name,
            value,
            "a decimal string (JSON numbers above 64 bits lose precision)",
        )
    })?;
    text.parse::<T>()
        .map_err(|_| AppError::ArgsFile(format!("{at}: '{text}' is not a valid {type_name}")))
}

/// Convert one `{"type": …, "value": …}` pair into an `ScVal`, following
/// the explicit type declaration. Nothing is inferred; every mismatch is an
/// error that names the document position (`args[2].value[0]`).
fn json_to_scval(
    type_name: &str,
    value: &serde_json::Value,
    at: &str,
) -> AppResult<stellar_xdr::ScVal> {
    match type_name {
        "bool" => value
            .as_bool()
            .map(stellar_xdr::ScVal::Bool)
            .ok_or_else(|| wrong_shape(at, type_name, value, "a JSON boolean (true or false)")),
        "u32" => {
            let n = json_integer_u64(value, at, type_name)?;
            u32::try_from(n).map(stellar_xdr::ScVal::U32).map_err(|_| {
                AppError::ArgsFile(format!(
                    "{at}: {n} is out of range for 'u32' (0..={})",
                    u32::MAX
                ))
            })
        }
        "i32" => {
            let n = json_integer_i64(value, at, type_name)?;
            i32::try_from(n).map(stellar_xdr::ScVal::I32).map_err(|_| {
                AppError::ArgsFile(format!(
                    "{at}: {n} is out of range for 'i32' ({}..={})",
                    i32::MIN,
                    i32::MAX
                ))
            })
        }
        "u64" => json_integer_u64(value, at, type_name).map(stellar_xdr::ScVal::U64),
        "i64" => json_integer_i64(value, at, type_name).map(stellar_xdr::ScVal::I64),
        "timepoint" => json_integer_u64(value, at, type_name)
            .map(|n| stellar_xdr::ScVal::Timepoint(stellar_xdr::TimePoint(n))),
        "duration" => json_integer_u64(value, at, type_name)
            .map(|n| stellar_xdr::ScVal::Duration(stellar_xdr::Duration(n))),
        "u128" => {
            let n = decimal_string_integer::<u128>(value, at, type_name)?;
            Ok(stellar_xdr::ScVal::U128(stellar_xdr::UInt128Parts {
                hi: n >> 64,
                lo: n as u64,
            }))
        }
        "i128" => {
            let n = decimal_string_integer::<i128>(value, at, type_name)?;
            Ok(stellar_xdr::ScVal::I128(stellar_xdr::Int128Parts {
                hi: (n >> 64) as i64,
                lo: n as u64,
            }))
        }
        "string" => {
            let s = value
                .as_str()
                .ok_or_else(|| wrong_shape(at, type_name, value, "a JSON string"))?;
            stellar_xdr::StringM::try_from(s.as_bytes().to_vec())
                .map(stellar_xdr::ScString::from)
                .map(stellar_xdr::ScVal::String)
                .map_err(|e| AppError::ArgsFile(format!("{at}: string is too long: {e}")))
        }
        "symbol" => {
            let s = value
                .as_str()
                .ok_or_else(|| wrong_shape(at, type_name, value, "a JSON string"))?;
            if !crate::wasm::parser::is_valid_symbol(s) {
                return Err(AppError::ArgsFile(format!(
                    "{at}: '{s}' is not a valid symbol (1..=32 characters of [A-Za-z0-9_])"
                )));
            }
            stellar_xdr::StringM::try_from(s.as_bytes().to_vec())
                .map(stellar_xdr::ScSymbol::from)
                .map(stellar_xdr::ScVal::Symbol)
                .map_err(|e| AppError::ArgsFile(format!("{at}: invalid symbol: {e}")))
        }
        "address" => {
            let s = value
                .as_str()
                .ok_or_else(|| wrong_shape(at, type_name, value, "a JSON string"))?;
            s.parse::<stellar_xdr::ScAddress>()
                .map(stellar_xdr::ScVal::Address)
                .map_err(|e| AppError::ArgsFile(format!("{at}: invalid address '{s}': {e}")))
        }
        "bytes" => {
            let s = value
                .as_str()
                .ok_or_else(|| wrong_shape(at, type_name, value, "a hex string"))?;
            let hex_str = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            let raw = hex::decode(hex_str).map_err(|e| {
                AppError::ArgsFile(format!("{at}: '{s}' is not valid hex: {e}"))
            })?;
            stellar_xdr::BytesM::try_from(raw)
                .map(stellar_xdr::ScBytes::from)
                .map(stellar_xdr::ScVal::Bytes)
                .map_err(|e| AppError::ArgsFile(format!("{at}: bytes value rejected: {e}")))
        }
        "vec" => {
            let items = value
                .as_array()
                .ok_or_else(|| wrong_shape(at, type_name, value, "a JSON array"))?;
            let mut sc_vals = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let nested: NestedArg = serde_json::from_value(item.clone()).map_err(|e| {
                    AppError::ArgsFile(format!(
                        "{at}[{i}]: vec elements must be objects of the form \
                         {{\"type\": …, \"value\": …}}: {e}"
                    ))
                })?;
                sc_vals.push(json_to_scval(
                    &nested.type_name,
                    &nested.value,
                    &format!("{at}[{i}]"),
                )?);
            }
            stellar_xdr::VecM::<stellar_xdr::ScVal>::try_from(sc_vals)
                .map(stellar_xdr::ScVec::from)
                .map(Some)
                .map(stellar_xdr::ScVal::Vec)
                .map_err(|e| AppError::ArgsFile(format!("{at}: vec is too large: {e}")))
        }
        "map" => {
            let items = value
                .as_array()
                .ok_or_else(|| {
                    wrong_shape(
                        at,
                        type_name,
                        value,
                        "a JSON array of {\"key\": …, \"value\": …} entries",
                    )
                })?;
            let mut entries = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let entry: ArgsFileMapEntry = serde_json::from_value(item.clone()).map_err(|e| {
                    AppError::ArgsFile(format!(
                        "{at}[{i}]: map entries must be objects of the form \
                         {{\"key\": {{\"type\": …, \"value\": …}}, \
                          \"value\": {{\"type\": …, \"value\": …}}}}: {e}"
                    ))
                })?;
                let key = json_to_scval(
                    &entry.key.type_name,
                    &entry.key.value,
                    &format!("{at}[{i}].key"),
                )?;
                let val = json_to_scval(
                    &entry.value.type_name,
                    &entry.value.value,
                    &format!("{at}[{i}].value"),
                )?;
                entries.push(stellar_xdr::ScMapEntry { key, val });
            }
            // Soroban maps are sorted by key; sort here (and reject
            // duplicates) so the encoded XDR is valid regardless of the
            // order the document lists its entries in.
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            if entries.windows(2).any(|pair| pair[0].key == pair[1].key) {
                return Err(AppError::ArgsFile(format!(
                    "{at}: map keys must be unique (duplicate key after sorting)"
                )));
            }
            stellar_xdr::VecM::<stellar_xdr::ScMapEntry>::try_from(entries)
                .map(stellar_xdr::ScMap::from)
                .map(Some)
                .map(stellar_xdr::ScVal::Map)
                .map_err(|e| AppError::ArgsFile(format!("{at}: map is too large: {e}")))
        }
        other => Err(unsupported_type(at, other)),
    }
}

/// True when `value` can be an argument for the contract-spec type
/// `type_def`.
///
/// `ScSpecTypeDef::Val` (the untyped escape hatch) accepts anything. Types
/// the v1 args-file format cannot express (`option`, `result`, `tuple`,
/// `udt`, `void`, `error`, `muxed_address`, 256-bit integers) match nothing,
/// so passing them via `--args-file` fails loudly instead of silently
/// simulating the wrong value; `--arg KEY=VALUE` remains the escape hatch.
/// `timepoint`/`duration` accept both the XDR-native `ScVal` variants and
/// plain `U64` values, mirroring how SDKs encode wall-clock values.
fn scval_matches_spec(type_def: &stellar_xdr::ScSpecTypeDef, value: &stellar_xdr::ScVal) -> bool {
    use stellar_xdr::{ScSpecTypeDef, ScVal};
    match type_def {
        ScSpecTypeDef::Val => true,
        ScSpecTypeDef::Bool => matches!(value, ScVal::Bool(_)),
        ScSpecTypeDef::U32 => matches!(value, ScVal::U32(_)),
        ScSpecTypeDef::I32 => matches!(value, ScVal::I32(_)),
        ScSpecTypeDef::U64 => matches!(value, ScVal::U64(_)),
        ScSpecTypeDef::I64 => matches!(value, ScVal::I64(_)),
        ScSpecTypeDef::Timepoint => matches!(value, ScVal::Timepoint(_) | ScVal::U64(_)),
        ScSpecTypeDef::Duration => matches!(value, ScVal::Duration(_) | ScVal::U64(_)),
        ScSpecTypeDef::U128 => matches!(value, ScVal::U128(_)),
        ScSpecTypeDef::I128 => matches!(value, ScVal::I128(_)),
        ScSpecTypeDef::Bytes => matches!(value, ScVal::Bytes(_)),
        ScSpecTypeDef::BytesN(spec) => {
            matches!(value, ScVal::Bytes(b) if b.len() == usize::try_from(spec.n).unwrap_or(usize::MAX))
        }
        ScSpecTypeDef::String => matches!(value, ScVal::String(_)),
        ScSpecTypeDef::Symbol => matches!(value, ScVal::Symbol(_)),
        ScSpecTypeDef::Address => matches!(value, ScVal::Address(_)),
        ScSpecTypeDef::Vec(spec) => match value {
            ScVal::Vec(Some(vec)) => vec
                .0
                .iter()
                .all(|item| scval_matches_spec(&spec.element_type, item)),
            _ => false,
        },
        ScSpecTypeDef::Map(spec) => match value {
            ScVal::Map(Some(map)) => map.0.iter().all(|entry| {
                scval_matches_spec(&spec.key_type, &entry.key)
                    && scval_matches_spec(&spec.value_type, &entry.val)
            }),
            _ => false,
        },
        ScSpecTypeDef::Void
        | ScSpecTypeDef::Error
        | ScSpecTypeDef::U256
        | ScSpecTypeDef::I256
        | ScSpecTypeDef::MuxedAddress
        | ScSpecTypeDef::Option(_)
        | ScSpecTypeDef::Result(_)
        | ScSpecTypeDef::Tuple(_)
        | ScSpecTypeDef::Udt(_) => false,
    }
}

/// Validate `--args-file` arguments against the contract spec, returning the
/// values ordered as the function declares its parameters (issue #486).
///
/// Mirrors [`validate_args_against_spec`]: validation is skipped entirely
/// when there is no function name, the function is not in the spec, or the
/// spec declares no parameters — the same leniency `--arg` gets. When the
/// spec applies:
///
/// * arguments may be **positional** (no `name` fields, order = declaration
///   order, count must match) or **named** (every argument has a `name`;
///   matching is by name and the values are reordered to the declaration
///   order); mixing the two is an error;
/// * every declared parameter must be provided exactly once — missing and
///   duplicated names are errors that list what the function expects;
/// * each value must match its parameter's spec type (see
///   [`scval_matches_spec`]), so a mismatch fails before any simulation.
///
/// # Errors
/// [`AppError::TypeValidation`] for every mismatch, with the parameter name
/// and both types in the message.
pub fn validate_typed_args_against_spec(
    function_name: Option<&str>,
    args: &[TypedArg],
    functions: &[FunctionInfo],
) -> AppResult<Vec<stellar_xdr::ScVal>> {
    let values_in_given_order = || args.iter().map(|a| a.value.clone()).collect();

    let Some(fn_name) = function_name else {
        return Ok(values_in_given_order());
    };
    let Some(fn_info) = functions.iter().find(|f| f.name == fn_name) else {
        return Ok(values_in_given_order());
    };
    if fn_info.params.is_empty() {
        return Ok(values_in_given_order());
    }

    let decl = fn_info
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.type_name))
        .collect::<Vec<_>>()
        .join(", ");

    let any_named = args.iter().any(|a| a.name.is_some());
    let all_named = args.iter().all(|a| a.name.is_some());
    if any_named && !all_named {
        return Err(AppError::TypeValidation(
            "cannot mix named and positional arguments in --args-file".to_string(),
        ));
    }

    let ordered: Vec<&TypedArg> = if any_named {
        // Named mode: match every spec parameter to exactly one argument.
        let mut picked: Vec<Option<&TypedArg>> = vec![None; fn_info.params.len()];
        for arg in args {
            let Some(name) = arg.name.as_deref() else {
                return Err(AppError::TypeValidation(
                    "cannot mix named and positional arguments in --args-file".to_string(),
                ));
            };
            match fn_info.params.iter().position(|p| p.name == name) {
                Some(idx) if picked[idx].is_none() => picked[idx] = Some(arg),
                Some(_) => {
                    return Err(AppError::TypeValidation(format!(
                        "duplicate argument '{name}' for function '{fn_name}'"
                    )));
                }
                None => {
                    return Err(AppError::TypeValidation(format!(
                        "unknown parameter '{name}' for function '{fn_name}' \
                         (function expects: {decl})"
                    )));
                }
            }
        }

        let mut result = Vec::with_capacity(picked.len());
        for (param, arg) in fn_info.params.iter().zip(&picked) {
            match arg {
                Some(arg) => result.push(*arg),
                None => {
                    return Err(AppError::TypeValidation(format!(
                        "missing argument '{param}' for function '{fn_name}' \
                         (function expects: {decl})",
                        param = param.name
                    )));
                }
            }
        }
        result
    } else {
        // Positional mode: same arity rule as `--arg`.
        if args.len() != fn_info.params.len() {
            return Err(AppError::TypeValidation(format!(
                "function '{fn_name}' expects {} argument(s) ({decl}), but {} were provided",
                fn_info.params.len(),
                args.len()
            )));
        }
        args.iter().collect()
    };

    let mut values = Vec::with_capacity(ordered.len());
    for (arg, param) in ordered.iter().zip(&fn_info.params) {
        if !scval_matches_spec(&param.type_def, &arg.value) {
            let label = arg.name.as_deref().unwrap_or(&param.name);
            return Err(AppError::TypeValidation(format!(
                "argument '{label}' declared as '{}' does not match contract spec \
                 type '{}' for function '{fn_name}'",
                arg.type_name,
                crate::wasm::parser::spec_type_name(&param.type_def)
            )));
        }
        values.push(arg.value.clone());
    }
    Ok(values)
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
            returns: Vec::new(),
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
            returns: vec![],
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
}
