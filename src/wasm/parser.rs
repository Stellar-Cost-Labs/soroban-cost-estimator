use std::io::Cursor;
use std::path::Path;

use stellar_xdr::ReadXdr;

use crate::error::{AppError, AppResult};

/// Loads a compiled Soroban contract `.wasm` file from disk.
///
/// Reads the file bytes, performs basic structural validation via
/// `wasmparser`, enumerates exported functions, and — when the WASM carries
/// a Soroban contract spec (`contractspecv0` custom section) — decodes the
/// typed parameter information from it.
///
/// # Network calls
/// None — pure file I/O + parsing.
pub fn load_wasm(path: &Path) -> AppResult<WasmInfo> {
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(path.display().to_string())
        } else {
            AppError::Io(e)
        }
    })?;

    validate_wasm(&bytes)?;
    let functions = enumerate_functions(&bytes)?;
    // The spec is advisory: if it cannot be decoded (e.g. an oversized or
    // malformed `contractspecv0` section), degrade to bare WASM exports
    // rather than failing the whole load.
    let (spec_functions, has_spec) = parse_contract_spec(&bytes).unwrap_or_default();

    // Enrich the export-derived function list with spec-derived typed params.
    let mut functions = functions;
    if !spec_functions.is_empty() {
        for fn_info in &mut functions {
            if let Some((_, params)) = spec_functions.iter().find(|(n, _)| n == &fn_info.name) {
                fn_info.params = params.clone();
                // The WASM type section counts the injected `env` pointer as
                // a parameter (`increment(env, step)` reports 2), but the
                // spec knows the real user-facing arity. Prefer the spec so
                // `param_count` drives --arg/--fn decisions correctly.
                fn_info.param_count = params.len() as u32;
            }
        }
    }

    Ok(WasmInfo {
        bytes,
        functions,
        has_spec,
    })
}

/// Basic structural validation of a WASM binary.
fn validate_wasm(bytes: &[u8]) -> AppResult<()> {
    wasmparser::validate(bytes).map_err(|e| AppError::WasmValidation(e.to_string()))?;
    Ok(())
}

/// Enumerates exported function names from a validated WASM binary.
fn enumerate_functions(bytes: &[u8]) -> AppResult<Vec<FunctionInfo>> {
    let mut functions = Vec::new();
    // Map from function index -> type index
    let mut func_to_type: Vec<u32> = Vec::new();
    // Map from type index -> (param_count, result_count)
    let mut type_infos: Vec<(u32, u32)> = Vec::new();

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        match payload {
            wasmparser::Payload::TypeSection(section) => {
                for rec_group in section {
                    let rec_group = rec_group.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    for ty in rec_group.types() {
                        let func_type = ty.unwrap_func();
                        type_infos.push((
                            func_type.params().len() as u32,
                            func_type.results().len() as u32,
                        ));
                    }
                }
            }
            wasmparser::Payload::FunctionSection(section) => {
                for func in section {
                    let func = func.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    func_to_type.push(func);
                }
            }
            wasmparser::Payload::ExportSection(section) => {
                for export in section {
                    let export = export.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    if matches!(export.kind, wasmparser::ExternalKind::Func) {
                        let idx = export.index as usize;
                        let (param_count, result_count) = func_to_type
                            .get(idx)
                            .and_then(|&type_idx| type_infos.get(type_idx as usize).copied())
                            .unwrap_or((0, 0));
                        functions.push(FunctionInfo {
                            name: export.name.to_string(),
                            param_count,
                            result_count,
                            params: Vec::new(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    if functions.is_empty() {
        return Err(AppError::WasmParse(
            "no exported functions found in WASM binary".to_string(),
        ));
    }

    Ok(functions)
}

/// Decoded spec function entries: (function name, typed parameter list).
type SpecFunctions = Vec<(String, Vec<ParamInfo>)>;

/// Decodes the Soroban contract spec (`contractspecv0` custom section).
///
/// Returns the function entries (name → typed params) and whether the
/// section was present at all. Function entries carry the typed parameter
/// list that the bare WASM export section cannot express.
///
/// The section payload is **not** a count-prefixed `VecM<ScSpecEntry>`: it is
/// a concatenation of raw `ScSpecEntry` XDR values, each starting with its
/// 4-byte union discriminant (e.g. `00 00 00 00` = FunctionV0). We therefore
/// decode entries one at a time from a cursor, stopping when the stream is
/// exhausted.
fn parse_contract_spec(bytes: &[u8]) -> AppResult<(SpecFunctions, bool)> {
    let mut spec_functions = Vec::new();
    let mut has_spec = false;

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        if let wasmparser::Payload::CustomSection(section) = payload {
            if section.name() != "contractspecv0" {
                continue;
            }
            has_spec = true;

            let data = section.data();
            let mut cursor = Cursor::new(data);
            while (cursor.position() as usize) < data.len() {
                let mut limited =
                    stellar_xdr::Limited::new(&mut cursor, stellar_xdr::Limits::none());
                // Break (not `?`) on a decode error: a trailing byte or a
                // truncated final entry should not discard the entries already
                // decoded. If nothing decoded, the caller's `unwrap_or_default`
                // still degrades gracefully to bare WASM exports.
                let Ok(entry) = stellar_xdr::ScSpecEntry::read_xdr(&mut limited) else {
                    break;
                };
                if let stellar_xdr::ScSpecEntry::FunctionV0(f) = entry {
                    let name = String::from_utf8_lossy(f.name.as_slice()).to_string();
                    let params = f
                        .inputs
                        .iter()
                        .map(|input| ParamInfo {
                            name: String::from_utf8_lossy(input.name.as_slice()).to_string(),
                            type_name: spec_type_name(&input.type_).to_string(),
                        })
                        .collect();
                    spec_functions.push((name, params));
                }
            }
        }
    }

    Ok((spec_functions, has_spec))
}

/// Human-readable name for a `ScSpecTypeDef`.
fn spec_type_name(t: &stellar_xdr::ScSpecTypeDef) -> &'static str {
    match t {
        stellar_xdr::ScSpecTypeDef::Val => "val",
        stellar_xdr::ScSpecTypeDef::Bool => "bool",
        stellar_xdr::ScSpecTypeDef::Void => "void",
        stellar_xdr::ScSpecTypeDef::Error => "error",
        stellar_xdr::ScSpecTypeDef::U32 => "u32",
        stellar_xdr::ScSpecTypeDef::I32 => "i32",
        stellar_xdr::ScSpecTypeDef::U64 => "u64",
        stellar_xdr::ScSpecTypeDef::I64 => "i64",
        stellar_xdr::ScSpecTypeDef::Timepoint => "timepoint",
        stellar_xdr::ScSpecTypeDef::Duration => "duration",
        stellar_xdr::ScSpecTypeDef::U128 => "u128",
        stellar_xdr::ScSpecTypeDef::I128 => "i128",
        stellar_xdr::ScSpecTypeDef::U256 => "u256",
        stellar_xdr::ScSpecTypeDef::I256 => "i256",
        stellar_xdr::ScSpecTypeDef::Bytes => "bytes",
        stellar_xdr::ScSpecTypeDef::String => "string",
        stellar_xdr::ScSpecTypeDef::Symbol => "symbol",
        stellar_xdr::ScSpecTypeDef::Address => "address",
        stellar_xdr::ScSpecTypeDef::MuxedAddress => "muxed_address",
        stellar_xdr::ScSpecTypeDef::Option(_) => "option",
        stellar_xdr::ScSpecTypeDef::Result(_) => "result",
        stellar_xdr::ScSpecTypeDef::Vec(_) => "vec",
        stellar_xdr::ScSpecTypeDef::Map(_) => "map",
        stellar_xdr::ScSpecTypeDef::Tuple(_) => "tuple",
        stellar_xdr::ScSpecTypeDef::BytesN(_) => "bytes_n",
        stellar_xdr::ScSpecTypeDef::Udt(_) => "udt",
    }
}

/// Information about a typed parameter from the contract spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamInfo {
    /// Parameter name (from the contract spec).
    pub name: String,
    /// Human-readable Soroban type, e.g. `I64`, `Symbol`, `String`.
    pub type_name: String,
}

/// Information about an exported function.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Name of the exported function.
    pub name: String,
    /// Number of parameters this function takes.
    pub param_count: u32,
    /// Number of return values.
    pub result_count: u32,
    /// Typed parameters from the contract spec, if the WASM has one.
    pub params: Vec<ParamInfo>,
}

/// Formats a function with its spec-derived signature, e.g. `increment(x: I64)`.
pub fn format_function(fn_info: &FunctionInfo) -> String {
    if fn_info.params.is_empty() {
        return fn_info.name.clone();
    }
    let params = fn_info
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({params})", fn_info.name)
}

/// Information extracted from a WASM file.
#[derive(Debug, Clone)]
pub struct WasmInfo {
    /// Raw WASM bytes.
    pub bytes: Vec<u8>,
    /// Names and signatures of exported (public) functions.
    pub functions: Vec<FunctionInfo>,
    /// Whether the WASM carries a Soroban contract spec (`contractspecv0`).
    pub has_spec: bool,
}
