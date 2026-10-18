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
/// Internal prefix used to carry a pre-loaded `--args-file` JSON payload
/// through the CLI argument slice pipeline.
pub const JSON_ARGS_MARKER_PREFIX: &str = "__soroban_json_args__:";

/// Supported explicit `"type"` tags in a `--args-file` JSON argument document.
pub const SUPPORTED_JSON_ARG_TYPES: &[&str] = &[
    "bool", "u32", "i32", "u64", "i64", "string", "symbol", "bytes", "address", "void", "vec",
    "map",
];

/// Validates `--arg` or `--args-file` values against the contract-spec types
/// for the function being simulated, before any RPC call is made.
///
/// When the WASM carries a `contractspecv0` and a matching function is found
/// with typed parameters, each `--arg` value must match the declared type
/// (e.g. `abc` is rejected for `i64`). Mismatches fail fast so a bad argument
/// never reaches the network. Only the target function's declared parameters
/// are checked: extra or missing arguments are a mismatch as well.
///
/// * `function_name` - `None` (WASM upload) or a function with no spec params
///   skips spec validation for `--arg` strings; JSON argument files still
///   validate their syntax and typed values.
/// * `args` - Raw `--arg KEY=VAL` entries (or a `JSON_ARGS_MARKER_PREFIX`
///   payload loaded from `--args-file`).
/// * `functions` - Enumerated functions with spec-derived params.
pub fn validate_args_against_spec(
    function_name: Option<&str>,
    args: &[String],
    functions: &[FunctionInfo],
) -> AppResult<()> {
    if let Some(json_payload) = extract_json_args_marker(args) {
        let _ = parse_json_invocation_args(json_payload, function_name, functions)?;
        return Ok(());
    }

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
        let decl = format_expected_params(fn_info);
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

fn extract_json_args_marker(args: &[String]) -> Option<&str> {
    if args.len() == 1 {
        args[0].strip_prefix(JSON_ARGS_MARKER_PREFIX)
    } else {
        None
    }
}

fn format_expected_params(fn_info: &FunctionInfo) -> String {
    fn_info
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.type_name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reads a JSON invocation arguments file from disk, validates its JSON syntax
/// and typed values upfront, and wraps the content in [`JSON_ARGS_MARKER_PREFIX`]
/// so it can travel through the CLI argument slice pipeline.
pub fn load_json_args_marker(path: &std::path::Path) -> AppResult<String> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(path.display().to_string())
        } else {
            AppError::Io(e)
        }
    })?;
    // Perform an upfront syntax & type check without contract spec so malformed
    // files, floating-point numbers, and unsupported types fail immediately.
    let _ = parse_json_invocation_args(&content, None, &[])?;
    Ok(format!("{JSON_ARGS_MARKER_PREFIX}{content}"))
}

/// Loads and validates typed invocation arguments from a JSON file against the
/// target function's contract spec.
pub fn load_json_invocation_args(
    path: &std::path::Path,
    function_name: Option<&str>,
    functions: &[FunctionInfo],
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(path.display().to_string())
        } else {
            AppError::Io(e)
        }
    })?;
    parse_json_invocation_args(&content, function_name, functions)
}

/// Resolves either CLI `--arg` entries or a `--args-file` JSON payload into
/// ordered `(label, ScVal)` pairs, validating against the contract spec when
/// present.
pub fn resolve_invocation_args(
    function_name: Option<&str>,
    args: &[String],
    functions: &[FunctionInfo],
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    if let Some(json_payload) = extract_json_args_marker(args) {
        return parse_json_invocation_args(json_payload, function_name, functions);
    }
    let sc_vals: Vec<stellar_xdr::ScVal> = args.iter().map(|a| parse_arg_scval(a)).collect();
    validate_args_against_spec(function_name, args, functions)?;
    Ok(args.iter().cloned().zip(sc_vals).collect())
}

/// Convenience wrapper around [` parse_json_invocation_args`] that returns only
/// the ordered [`stellar_xdr::ScVal`] vector.
pub fn parse_json_args_scvals(
    content: &str,
    function_name: Option<&str>,
    functions: &[FunctionInfo],
) -> AppResult<Vec<stellar_xdr::ScVal>> {
    Ok(
        parse_json_invocation_args(content, function_name, functions)?
            .into_iter()
            .map(|(_, val)| val)
            .collect(),
    )
}

/// Parses a JSON document containing typed and/or named invocation arguments,
/// validates parameter names and types against the contract spec (when
/// available), and returns ordered `(label, ScVal)` pairs.
///
/// # Supported JSON structures
/// 1. Positional or named array:
///    - `[{"type": "i64", "value": 5}, {"type": "symbol", "value": "tag"}]`
///    - `[{"name": "step", "type": "i64", "value": 5}]`
/// 2. Named parameter map (top-level object or `{"args": { ... }}`):
///    - `{"step": {"type": "i64", "value": 5}, "label": {"type": "symbol", "value": "tag"}}`
#[allow(clippy::too_many_lines)]
pub fn parse_json_invocation_args(
    content: &str,
    function_name: Option<&str>,
    functions: &[FunctionInfo],
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    let root: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| AppError::TypeValidation(format!("invalid JSON in arguments file: {e}")))?;

    let fn_info = function_name.and_then(|name| functions.iter().find(|f| f.name == name));
    let has_spec_params = fn_info.is_some_and(|f| !f.params.is_empty());

    let effective_root = match &root {
        serde_json::Value::Object(map)
            if map.len() == 1
                && map.contains_key("args")
                && !fn_info.is_some_and(|f| f.params.iter().any(|p| p.name == "args")) =>
        {
            &map["args"]
        }
        other => other,
    };

    match effective_root {
        serde_json::Value::Array(items) => {
            parse_json_arg_array(items, function_name, fn_info, has_spec_params)
        }
        serde_json::Value::Object(map) => {
            parse_json_arg_object(map, function_name, fn_info, has_spec_params)
        }
        _ => Err(AppError::TypeValidation(
            "JSON arguments file must contain a top-level array or parameter object".to_string(),
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_json_arg_array(
    items: &[serde_json::Value],
    function_name: Option<&str>,
    fn_info: Option<&FunctionInfo>,
    has_spec_params: bool,
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    let any_named = items
        .iter()
        .any(|item| item.as_object().is_some_and(|obj| obj.contains_key("name")));
    let all_named = items
        .iter()
        .all(|item| item.as_object().is_some_and(|obj| obj.contains_key("name")));

    if any_named && !all_named {
        return Err(AppError::TypeValidation(
            "cannot mix named and positional arguments in JSON array".to_string(),
        ));
    }

    if any_named && !items.is_empty() {
        let mut named_entries: Vec<(String, serde_json::Value)> = Vec::with_capacity(items.len());
        let mut seen = std::collections::BTreeSet::new();
        for item in items {
            let Some(obj) = item.as_object() else {
                return Err(AppError::TypeValidation(
                    "named argument entry must be a JSON object".to_string(),
                ));
            };
            let Some(name_val) = obj.get("name").and_then(serde_json::Value::as_str) else {
                return Err(AppError::TypeValidation(
                    "argument 'name' field must be a non-empty string".to_string(),
                ));
            };
            if name_val.trim().is_empty() {
                return Err(AppError::TypeValidation(
                    "argument 'name' field must be a non-empty string".to_string(),
                ));
            }
            if !seen.insert(name_val.to_string()) {
                return Err(AppError::TypeValidation(format!(
                    "duplicate parameter '{name_val}' in JSON arguments"
                )));
            }
            let mut stripped = obj.clone();
            stripped.remove("name");
            let arg_val = if stripped.len() == 1 && stripped.contains_key("value") {
                stripped["value"].clone()
            } else {
                serde_json::Value::Object(stripped)
            };
            named_entries.push((name_val.to_string(), arg_val));
        }
        return resolve_named_entries(&named_entries, function_name, fn_info, has_spec_params);
    }

    if has_spec_params {
        if let Some(info) = fn_info {
            if items.len() != info.params.len() {
                let decl = format_expected_params(info);
                let fn_label = function_name.unwrap_or(&info.name);
                return Err(AppError::TypeValidation(format!(
                    "function '{fn_label}' expects {} argument(s) ({decl}), but {} were provided",
                    info.params.len(),
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for (item, param) in items.iter().zip(&info.params) {
                let sc_val = parse_typed_json_scval(item, Some(&param.type_def)).map_err(|e| {
                    AppError::TypeValidation(format!("parameter '{}': {e}", param.name))
                })?;
                out.push((
                    format!("{}={}", param.name, compact_json_label(item)),
                    sc_val,
                ));
            }
            return Ok(out);
        }
    }

    let mut out = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let sc_val = parse_typed_json_scval(item, None)
            .map_err(|e| AppError::TypeValidation(format!("argument[{idx}]: {e}")))?;
        out.push((compact_json_label(item), sc_val));
    }
    Ok(out)
}

fn parse_json_arg_object(
    map: &serde_json::Map<String, serde_json::Value>,
    function_name: Option<&str>,
    fn_info: Option<&FunctionInfo>,
    has_spec_params: bool,
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    // Check if the user wrote a single named entry `{"name": "x", "type": "...", "value": ...}`
    // at the top level instead of wrapping it in an array.
    if map.contains_key("name")
        && map.contains_key("type")
        && !fn_info.is_some_and(|f| f.params.iter().any(|p| p.name == "name"))
    {
        return parse_json_arg_array(
            &[serde_json::Value::Object(map.clone())],
            function_name,
            fn_info,
            has_spec_params,
        );
    }

    let entries: Vec<(String, serde_json::Value)> =
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    resolve_named_entries(&entries, function_name, fn_info, has_spec_params)
}

fn resolve_named_entries(
    entries: &[(String, serde_json::Value)],
    function_name: Option<&str>,
    fn_info: Option<&FunctionInfo>,
    has_spec_params: bool,
) -> AppResult<Vec<(String, stellar_xdr::ScVal)>> {
    if has_spec_params {
        if let Some(info) = fn_info {
            let fn_label = function_name.unwrap_or(&info.name);
            let expected_names: Vec<&str> = info.params.iter().map(|p| p.name.as_str()).collect();
            for (name, _) in entries {
                if !expected_names.contains(&name.as_str()) {
                    return Err(AppError::TypeValidation(format!(
                        "unknown parameter '{name}' for function '{fn_label}': expected [{}]",
                        expected_names.join(", ")
                    )));
                }
            }
            if entries.len() != info.params.len() {
                let decl = format_expected_params(info);
                return Err(AppError::TypeValidation(format!(
                    "function '{fn_label}' expects {} argument(s) ({decl}), but {} were provided",
                    info.params.len(),
                    entries.len()
                )));
            }
            let mut out = Vec::with_capacity(info.params.len());
            for param in &info.params {
                let Some((_, raw_val)) = entries.iter().find(|(k, _)| k == &param.name) else {
                    return Err(AppError::TypeValidation(format!(
                        "missing required parameter '{}' ({}) for function '{fn_label}'",
                        param.name, param.type_name
                    )));
                };
                let sc_val =
                    parse_typed_json_scval(raw_val, Some(&param.type_def)).map_err(|e| {
                        AppError::TypeValidation(format!("parameter '{}': {e}", param.name))
                    })?;
                out.push((
                    format!("{}={}", param.name, compact_json_label(raw_val)),
                    sc_val,
                ));
            }
            return Ok(out);
        }
    }

    let mut out = Vec::with_capacity(entries.len());
    for (name, raw_val) in entries {
        if name.trim().is_empty() {
            return Err(AppError::TypeValidation(
                "parameter name in JSON arguments cannot be empty".to_string(),
            ));
        }
        let sc_val = parse_typed_json_scval(raw_val, None)
            .map_err(|e| AppError::TypeValidation(format!("parameter '{name}': {e}")))?;
        out.push((format!("{name}={}", compact_json_label(raw_val)), sc_val));
    }
    Ok(out)
}

fn compact_json_label(value: &serde_json::Value) -> String {
    if let Some(obj) = value.as_object() {
        if let Some(val) = obj.get("value") {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            return val.to_string();
        }
    }
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    value.to_string()
}

/// Parses a single JSON argument value into a [`stellar_xdr::ScVal`], enforcing
/// explicit `"type"` tags (`bool`, `u32`, `i32`, `u64`, `i64`, `string`,
/// `symbol`, `bytes`, `address`, `void`, `vec`, `map`), rejecting
/// floating-point numbers, and checking compatibility with `expected_spec` when
/// provided.
#[allow(clippy::too_many_lines)]
pub fn parse_typed_json_scval(
    value: &serde_json::Value,
    expected_spec: Option<&stellar_xdr::ScSpecTypeDef>,
) -> AppResult<stellar_xdr::ScVal> {
    if let serde_json::Value::Object(obj) = value {
        if let Some(type_val) = obj.get("type") {
            let Some(type_name) = type_val.as_str() else {
                return Err(AppError::TypeValidation(
                    "JSON argument 'type' field must be a string".to_string(),
                ));
            };
            for key in obj.keys() {
                if key != "type" && key != "value" {
                    return Err(AppError::TypeValidation(format!(
                        "unexpected field '{key}' in typed JSON argument (expected only 'type' and 'value')"
                    )));
                }
            }
            validate_explicit_type_name(type_name, expected_spec)?;
            let inner_val = obj.get("value").unwrap_or(&serde_json::Value::Null);
            return parse_value_for_type_name(type_name, inner_val, expected_spec);
        }
    }

    // Reject any raw floating-point number before doing spec-guided or
    // untagged inference.
    if let serde_json::Value::Number(n) = value {
        if !n.is_i64() && !n.is_u64() {
            return Err(AppError::TypeValidation(format!(
                "floating-point number '{n}' is not supported for Soroban arguments"
            )));
        }
    }

    if let Some(spec) = expected_spec {
        let Some(type_name) = spec_to_json_type_name(spec) else {
            let spec_name = crate::wasm::parser::spec_type_name(spec);
            return Err(AppError::TypeValidation(format!(
                "unsupported contract-spec parameter type '{spec_name}': supported types are {}",
                SUPPORTED_JSON_ARG_TYPES.join(", ")
            )));
        };
        return parse_value_for_type_name(type_name, value, Some(spec));
    }

    match value {
        serde_json::Value::Null => Ok(stellar_xdr::ScVal::Void),
        serde_json::Value::Bool(b) => Ok(stellar_xdr::ScVal::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(stellar_xdr::ScVal::I64(i))
            } else if let Some(u) = n.as_u64() {
                Ok(stellar_xdr::ScVal::U64(u))
            } else {
                Err(AppError::TypeValidation(format!(
                    "floating-point number '{n}' is not supported for Soroban arguments"
                )))
            }
        }
        serde_json::Value::String(_) => parse_value_for_type_name("string", value, None),
        serde_json::Value::Array(_) => parse_value_for_type_name("vec", value, None),
        serde_json::Value::Object(_) => Err(AppError::TypeValidation(
            "JSON argument object must include a 'type' field (e.g. {\"type\": \"i64\", \"value\": 42})"
                .to_string(),
        )),
    }
}

fn spec_to_json_type_name(spec: &stellar_xdr::ScSpecTypeDef) -> Option<&'static str> {
    use stellar_xdr::ScSpecTypeDef as T;
    match spec {
        T::Bool => Some("bool"),
        T::Void => Some("void"),
        T::U32 => Some("u32"),
        T::I32 => Some("i32"),
        T::U64 | T::Timepoint | T::Duration => Some("u64"),
        T::I64 => Some("i64"),
        T::String => Some("string"),
        T::Symbol => Some("symbol"),
        T::Bytes | T::BytesN(_) => Some("bytes"),
        T::Address => Some("address"),
        T::Vec(_) => Some("vec"),
        T::Map(_) => Some("map"),
        T::Val => Some("val"),
        _ => None,
    }
}

fn validate_explicit_type_name(
    type_name: &str,
    expected_spec: Option<&stellar_xdr::ScSpecTypeDef>,
) -> AppResult<()> {
    let normalized = if type_name == "bytes_n" {
        "bytes"
    } else {
        type_name
    };
    if !SUPPORTED_JSON_ARG_TYPES.contains(&normalized) {
        return Err(AppError::TypeValidation(format!(
            "unsupported JSON argument type '{type_name}': supported types are {}",
            SUPPORTED_JSON_ARG_TYPES.join(", ")
        )));
    }

    if let Some(spec) = expected_spec {
        if matches!(spec, stellar_xdr::ScSpecTypeDef::Val) {
            return Ok(());
        }
        let Some(expected_type) = spec_to_json_type_name(spec) else {
            let spec_name = crate::wasm::parser::spec_type_name(spec);
            return Err(AppError::TypeValidation(format!(
                "contract-spec parameter type '{spec_name}' is not supported by JSON arguments"
            )));
        };
        if normalized != expected_type {
            let spec_name = crate::wasm::parser::spec_type_name(spec);
            return Err(AppError::TypeValidation(format!(
                "type mismatch: contract spec expects '{spec_name}', but JSON argument has type '{type_name}'"
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn parse_value_for_type_name(
    type_name: &str,
    value: &serde_json::Value,
    expected_spec: Option<&stellar_xdr::ScSpecTypeDef>,
) -> AppResult<stellar_xdr::ScVal> {
    let normalized = if type_name == "bytes_n" {
        "bytes"
    } else {
        type_name
    };
    match normalized {
        "bool" => value
            .as_bool()
            .map(stellar_xdr::ScVal::Bool)
            .ok_or_else(|| {
                AppError::TypeValidation(format!("expected boolean for 'bool', got {value}"))
            }),
        "u32" => parse_json_u32(value).map(stellar_xdr::ScVal::U32),
        "i32" => parse_json_i32(value).map(stellar_xdr::ScVal::I32),
        "u64" => parse_json_u64(value).map(stellar_xdr::ScVal::U64),
        "i64" => parse_json_i64(value).map(stellar_xdr::ScVal::I64),
        "string" => {
            let Some(s) = value.as_str() else {
                return Err(AppError::TypeValidation(format!(
                    "expected string for 'string', got {value}"
                )));
            };
            let sc_str = stellar_xdr::StringM::try_from(s.as_bytes().to_vec())
                .map(stellar_xdr::ScString::from)
                .map_err(|e| AppError::TypeValidation(format!("invalid Soroban string: {e}")))?;
            Ok(stellar_xdr::ScVal::String(sc_str))
        }
        "symbol" => {
            let Some(s) = value.as_str() else {
                return Err(AppError::TypeValidation(format!(
                    "expected string for 'symbol', got {value}"
                )));
            };
            if !crate::wasm::parser::is_valid_symbol(s) {
                return Err(AppError::TypeValidation(format!(
                    "invalid Soroban symbol '{s}': must be 1..=32 ASCII alphanumeric or '_' characters"
                )));
            }
            let sym = stellar_xdr::ScSymbol::try_from(s.as_bytes().to_vec())
                .map_err(|e| AppError::TypeValidation(format!("invalid Soroban symbol: {e}")))?;
            Ok(stellar_xdr::ScVal::Symbol(sym))
        }
        "bytes" => parse_json_bytes(value, expected_spec),
        "address" => {
            let Some(s) = value.as_str() else {
                return Err(AppError::TypeValidation(format!(
                    "expected string for 'address', got {value}"
                )));
            };
            parse_sc_address(s).map(stellar_xdr::ScVal::Address)
        }
        "void" => {
            if !value.is_null() {
                return Err(AppError::TypeValidation(format!(
                    "expected null for 'void', got {value}"
                )));
            }
            Ok(stellar_xdr::ScVal::Void)
        }
        "vec" => {
            let Some(items) = value.as_array() else {
                return Err(AppError::TypeValidation(format!(
                    "expected JSON array for 'vec', got {value}"
                )));
            };
            let elem_spec = match expected_spec {
                Some(stellar_xdr::ScSpecTypeDef::Vec(v)) => Some(v.element_type.as_ref()),
                _ => None,
            };
            let mut parsed = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                let sc_val = parse_typed_json_scval(item, elem_spec)
                    .map_err(|e| AppError::TypeValidation(format!("vec[{idx}]: {e}")))?;
                parsed.push(sc_val);
            }
            let vec_m: stellar_xdr::VecM<stellar_xdr::ScVal> = parsed
                .try_into()
                .map_err(|e| AppError::TypeValidation(format!("invalid Soroban vec: {e}")))?;
            Ok(stellar_xdr::ScVal::Vec(Some(stellar_xdr::ScVec(vec_m))))
        }
        "map" => parse_json_map(value, expected_spec),
        "val" => parse_typed_json_scval(value, None),
        other => Err(AppError::TypeValidation(format!(
            "unsupported JSON argument type '{other}': supported types are {}",
            SUPPORTED_JSON_ARG_TYPES.join(", ")
        ))),
    }
}

fn reject_float_value(value: &serde_json::Value, type_label: &str) -> AppResult<()> {
    match value {
        serde_json::Value::Number(n) if !n.is_i64() && !n.is_u64() => {
            Err(AppError::TypeValidation(format!(
                "floating-point number '{n}' is not allowed for '{type_label}'"
            )))
        }
        serde_json::Value::String(s) if s.contains('.') || s.contains('e') || s.contains('E') => {
            Err(AppError::TypeValidation(format!(
                "floating-point number '{s}' is not allowed for '{type_label}'"
            )))
        }
        _ => Ok(()),
    }
}

fn parse_json_u32(value: &serde_json::Value) -> AppResult<u32> {
    reject_float_value(value, "u32")?;
    match value {
        serde_json::Value::Number(n) => {
            n.as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| {
                    AppError::TypeValidation(format!("value '{n}' is out of range for 'u32'"))
                })
        }
        serde_json::Value::String(s) => s.parse::<u32>().map_err(|_| {
            AppError::TypeValidation(format!("value '{s}' cannot be parsed as 'u32'"))
        }),
        other => Err(AppError::TypeValidation(format!(
            "expected integer for 'u32', got {other}"
        ))),
    }
}

fn parse_json_i32(value: &serde_json::Value) -> AppResult<i32> {
    reject_float_value(value, "i32")?;
    match value {
        serde_json::Value::Number(n) => {
            n.as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| {
                    AppError::TypeValidation(format!("value '{n}' is out of range for 'i32'"))
                })
        }
        serde_json::Value::String(s) => s.parse::<i32>().map_err(|_| {
            AppError::TypeValidation(format!("value '{s}' cannot be parsed as 'i32'"))
        }),
        other => Err(AppError::TypeValidation(format!(
            "expected integer for 'i32', got {other}"
        ))),
    }
}

fn parse_json_u64(value: &serde_json::Value) -> AppResult<u64> {
    reject_float_value(value, "u64")?;
    match value {
        serde_json::Value::Number(n) => n.as_u64().ok_or_else(|| {
            AppError::TypeValidation(format!("value '{n}' is out of range for 'u64'"))
        }),
        serde_json::Value::String(s) => s.parse::<u64>().map_err(|_| {
            AppError::TypeValidation(format!("value '{s}' cannot be parsed as 'u64'"))
        }),
        other => Err(AppError::TypeValidation(format!(
            "expected integer for 'u64', got {other}"
        ))),
    }
}

fn parse_json_i64(value: &serde_json::Value) -> AppResult<i64> {
    reject_float_value(value, "i64")?;
    match value {
        serde_json::Value::Number(n) => n.as_i64().ok_or_else(|| {
            AppError::TypeValidation(format!("value '{n}' is out of range for 'i64'"))
        }),
        serde_json::Value::String(s) => s.parse::<i64>().map_err(|_| {
            AppError::TypeValidation(format!("value '{s}' cannot be parsed as 'i64'"))
        }),
        other => Err(AppError::TypeValidation(format!(
            "expected integer for 'i64', got {other}"
        ))),
    }
}

fn parse_json_bytes(
    value: &serde_json::Value,
    expected_spec: Option<&stellar_xdr::ScSpecTypeDef>,
) -> AppResult<stellar_xdr::ScVal> {
    let raw_bytes: Vec<u8> = match value {
        serde_json::Value::String(s) => {
            let stripped = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            if !stripped.is_empty() && !crate::wasm::parser::is_valid_hex(s) {
                return Err(AppError::TypeValidation(format!(
                    "invalid hex string '{s}' for 'bytes' (expected even-length hex)"
                )));
            }
            hex::decode(stripped).map_err(|e| {
                AppError::TypeValidation(format!("invalid hex string '{s}' for 'bytes': {e}"))
            })?
        }
        serde_json::Value::Array(items) => {
            let mut buf = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                reject_float_value(item, "bytes")?;
                let Some(byte) = item.as_u64().and_then(|v| u8::try_from(v).ok()) else {
                    return Err(AppError::TypeValidation(format!(
                        "bytes[{idx}]: expected byte integer 0..=255, got {item}"
                    )));
                };
                buf.push(byte);
            }
            buf
        }
        other => {
            return Err(AppError::TypeValidation(format!(
                "expected hex string or byte array for 'bytes', got {other}"
            )));
        }
    };

    if let Some(stellar_xdr::ScSpecTypeDef::BytesN(spec)) = expected_spec {
        let expected_len = usize::try_from(spec.n).unwrap_or(0);
        if raw_bytes.len() != expected_len {
            return Err(AppError::TypeValidation(format!(
                "expected {expected_len} bytes for 'bytes_n({expected_len})', got {} bytes",
                raw_bytes.len()
            )));
        }
    }

    let bytes_m: stellar_xdr::BytesM = raw_bytes
        .try_into()
        .map_err(|e| AppError::TypeValidation(format!("invalid Soroban bytes: {e}")))?;
    Ok(stellar_xdr::ScVal::Bytes(stellar_xdr::ScBytes(bytes_m)))
}

/// Parses a Stellar contract (`C…` or 64-hex) or account (`G…`) address into
/// a [`stellar_xdr::ScAddress`].
pub fn parse_sc_address(value: &str) -> AppResult<stellar_xdr::ScAddress> {
    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(decoded) = hex::decode(value) {
            if let Ok(bytes) = <[u8; 32]>::try_from(decoded) {
                return Ok(stellar_xdr::ScAddress::Contract(stellar_xdr::ContractId(
                    stellar_xdr::Hash(bytes),
                )));
            }
        }
    }
    value.parse::<stellar_xdr::ScAddress>().map_err(|e| {
        AppError::TypeValidation(format!(
            "invalid Soroban address '{value}' (expected C… contract strkey, G… account strkey, or 64-hex contract id): {e}"
        ))
    })
}

fn parse_json_map(
    value: &serde_json::Value,
    expected_spec: Option<&stellar_xdr::ScSpecTypeDef>,
) -> AppResult<stellar_xdr::ScVal> {
    let (key_spec, val_spec) = match expected_spec {
        Some(stellar_xdr::ScSpecTypeDef::Map(m)) => {
            (Some(m.key_type.as_ref()), Some(m.value_type.as_ref()))
        }
        _ => (None, None),
    };

    let mut entries: Vec<stellar_xdr::ScMapEntry> = Vec::new();
    match value {
        serde_json::Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                let Some(obj) = item.as_object() else {
                    return Err(AppError::TypeValidation(format!(
                        "map[{idx}]: expected object with 'key' and 'val'/'value' fields"
                    )));
                };
                let Some(raw_key) = obj.get("key") else {
                    return Err(AppError::TypeValidation(format!(
                        "map[{idx}]: missing 'key' field"
                    )));
                };
                let Some(raw_val) = obj.get("val").or_else(|| obj.get("value")) else {
                    return Err(AppError::TypeValidation(format!(
                        "map[{idx}]: missing 'val' or 'value' field"
                    )));
                };
                let key = parse_typed_json_scval(raw_key, key_spec)
                    .map_err(|e| AppError::TypeValidation(format!("map[{idx}].key: {e}")))?;
                let val = parse_typed_json_scval(raw_val, val_spec)
                    .map_err(|e| AppError::TypeValidation(format!("map[{idx}].val: {e}")))?;
                entries.push(stellar_xdr::ScMapEntry { key, val });
            }
        }
        serde_json::Value::Object(obj) => {
            for (k_str, raw_val) in obj {
                let k_json = serde_json::Value::String(k_str.clone());
                let key = parse_typed_json_scval(&k_json, key_spec)
                    .map_err(|e| AppError::TypeValidation(format!("map key '{k_str}': {e}")))?;
                let val = parse_typed_json_scval(raw_val, val_spec)
                    .map_err(|e| AppError::TypeValidation(format!("map['{k_str}']: {e}")))?;
                entries.push(stellar_xdr::ScMapEntry { key, val });
            }
        }
        other => {
            return Err(AppError::TypeValidation(format!(
                "expected JSON array of {{key, val}} entries or JSON object for 'map', got {other}"
            )));
        }
    }

    let map_m: stellar_xdr::VecM<stellar_xdr::ScMapEntry> = entries
        .try_into()
        .map_err(|e| AppError::TypeValidation(format!("invalid Soroban map: {e}")))?;
    Ok(stellar_xdr::ScVal::Map(Some(stellar_xdr::ScMap(map_m))))
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
