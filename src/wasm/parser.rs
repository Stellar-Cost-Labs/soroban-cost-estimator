use std::io::Cursor;
use std::path::Path;

use stellar_xdr::{ReadXdr, ScVal};
use tracing::{debug, trace};

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
    debug!(path = %path.display(), "loading WASM file");
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::FileNotFound(path.display().to_string())
        } else {
            AppError::Io(e)
        }
    })?;
    debug!(bytes = bytes.len(), "WASM bytes read");

    validate_wasm(&bytes)?;
    debug!("WASM validated");

    let metadata = enumerate_module_metadata(&bytes)?;
    let (spec_functions, has_spec) = parse_contract_spec(&bytes).unwrap_or_default();
    let contract_meta = parse_contract_meta(&bytes).unwrap_or_default();

    let mut functions = metadata.functions;
    if !spec_functions.is_empty() {
        for fn_info in &mut functions {
            if let Some((_, params)) = spec_functions.iter().find(|(n, _)| n == &fn_info.name) {
                fn_info.params = params.clone();
                fn_info.param_count = params.len() as u32;
            }
        }
    }

    trace!(functions = functions.len(), has_spec, "WASM parsed");
    Ok(WasmInfo {
        bytes,
        functions,
        has_spec,
        contract_meta,
        start_function: metadata.start_function,
        memories: metadata.memories,
        imports: metadata.imports,
        exports: metadata.exports,
    })
}

/// Basic structural validation of a WASM binary.
pub fn validate_wasm(bytes: &[u8]) -> AppResult<()> {
    wasmparser::validate(bytes).map_err(|e| AppError::WasmValidation(e.to_string()))?;
    Ok(())
}

/// Enumerates exported function names from a validated WASM binary.
pub fn enumerate_functions(bytes: &[u8]) -> AppResult<Vec<FunctionInfo>> {
    Ok(enumerate_module_metadata(bytes)?.functions)
}

/// Metadata captured while walking a WASM module.
#[derive(Debug, Clone)]
pub struct ModuleMetadata {
    /// Names and signatures of exported functions.
    pub functions: Vec<FunctionInfo>,
    /// Index of the module's start function, if one is declared.
    pub start_function: Option<u32>,
    /// Linear memories declared by the module, with their limits.
    pub memories: Vec<MemoryInfo>,
    /// Imports declared by the module (`module::name` → kind).
    pub imports: Vec<ImportInfo>,
    /// Exports declared by the module, including non-function exports.
    pub exports: Vec<ExportInfo>,
}

/// Enumerates exported functions and captures module entry-point metadata:
/// the start function, memory limits, and the import/export structure.
///
/// This is the "diagnostic" walk — it records everything `estimate-all`
/// needs to describe a module, not just the typed function list. Function
/// signatures are reconstructed by following the type/function/export index
/// spaces, the same as `enumerate_functions`.
#[allow(clippy::too_many_lines)]
pub fn enumerate_module_metadata(bytes: &[u8]) -> AppResult<ModuleMetadata> {
    let mut functions = Vec::new();
    // Map from function index -> type index
    let mut func_to_type: Vec<u32> = Vec::new();
    // Map from type index -> (param_count, result_count)
    let mut type_infos: Vec<(u32, u32)> = Vec::new();
    let mut start_function = None;
    let mut memories = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();

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
                    if export.kind == wasmparser::ExternalKind::Func {
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
                    exports.push(ExportInfo {
                        name: export.name.to_string(),
                        kind: external_kind_name(export.kind).to_string(),
                        index: export.index,
                    });
                }
            }
            wasmparser::Payload::MemorySection(section) => {
                for memory in section {
                    let memory = memory.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    memories.push(MemoryInfo {
                        initial_pages: memory.initial,
                        maximum_pages: memory.maximum,
                        memory64: memory.memory64,
                    });
                }
            }
            wasmparser::Payload::StartSection { func, .. } => start_function = Some(func),
            wasmparser::Payload::ImportSection(section) => {
                for group in section {
                    let group = group.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    match group {
                        wasmparser::Imports::Single(_, imported) => {
                            imports.push(ImportInfo {
                                module: imported.module.to_string(),
                                name: imported.name.to_string(),
                                kind: type_ref_kind_name(&imported.ty).to_string(),
                            });
                        }
                        wasmparser::Imports::Compact1 { module, items } => {
                            for item in items {
                                let item = item.map_err(|e| AppError::WasmParse(e.to_string()))?;
                                imports.push(ImportInfo {
                                    module: module.to_string(),
                                    name: item.name.to_string(),
                                    kind: type_ref_kind_name(&item.ty).to_string(),
                                });
                            }
                        }
                        wasmparser::Imports::Compact2 { module, ty, names } => {
                            for name in names {
                                let name = name.map_err(|e| AppError::WasmParse(e.to_string()))?;
                                imports.push(ImportInfo {
                                    module: module.to_string(),
                                    name: name.to_string(),
                                    kind: type_ref_kind_name(&ty).to_string(),
                                });
                            }
                        }
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

    Ok(ModuleMetadata {
        functions,
        start_function,
        memories,
        imports,
        exports,
    })
}

/// Human-readable name for an `ExternalKind`.
#[must_use]
pub fn external_kind_name(kind: wasmparser::ExternalKind) -> &'static str {
    match kind {
        wasmparser::ExternalKind::Func => "function",
        wasmparser::ExternalKind::Table => "table",
        wasmparser::ExternalKind::Memory => "memory",
        wasmparser::ExternalKind::Global => "global",
        wasmparser::ExternalKind::Tag => "tag",
        wasmparser::ExternalKind::FuncExact => "function (exact type)",
    }
}

/// Human-readable name for a `TypeRef` (import object kind).
#[must_use]
pub fn type_ref_kind_name(ty: &wasmparser::TypeRef) -> &'static str {
    match ty {
        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_) => "function",
        wasmparser::TypeRef::Table(_) => "table",
        wasmparser::TypeRef::Memory(_) => "memory",
        wasmparser::TypeRef::Global(_) => "global",
        wasmparser::TypeRef::Tag(_) => "tag",
    }
}

/// Decoded spec function entries: (function name, typed parameter list).
pub type SpecFunctions = Vec<(String, Vec<ParamInfo>)>;

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
pub fn parse_contract_spec(bytes: &[u8]) -> AppResult<(SpecFunctions, bool)> {
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
                    stellar_xdr::Limited::new(&mut cursor, stellar_xdr::Limits::unlimited());
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
                            type_def: input.type_.clone(),
                        })
                        .collect();
                    spec_functions.push((name, params));
                }
            }
        }
    }

    Ok((spec_functions, has_spec))
}

/// Metadata parsed from the Soroban `contractmetaV0` custom section.
///
/// Contract developers attach this section (typically via the SDK's
/// `contractmetadata`/`contractmeta` macros) to carry human-readable
/// information about the contract: a name, a version, and a description,
/// plus arbitrary extra key/value pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractMeta {
    /// Contract name, when the section carries a `name` key.
    pub name: Option<String>,
    /// Contract version, when the section carries a `version` key.
    pub version: Option<String>,
    /// Contract description, when the section carries a `description`
    /// (or `desc`) key.
    pub description: Option<String>,
    /// Every key/value pair found in the section, in section order —
    /// including the recognized keys above and any custom ones.
    pub entries: Vec<(String, String)>,
}

impl ContractMeta {
    /// True when the WASM carried no decodable `contractmetaV0` entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Parses the Soroban contract metadata (`contractmetaV0` custom section).
///
/// Returns the parsed name/version/description plus the full ordered list of
/// key/value pairs. The section is optional — a WASM without one yields an
/// empty `ContractMeta`, never an error.
///
/// Like `contractspecv0`, the section payload is **not** a count-prefixed
/// vector: it is a concatenation of raw `ScMetaEntry` XDR union values, each
/// starting with its 4-byte union discriminant (`00 00 00 00` = `ScMetaV0`)
/// followed by the `{ key, val }` struct. Entries are decoded one at a time
/// from a cursor; a truncated or malformed trailing entry stops the loop
/// without discarding the entries already decoded.
pub fn parse_contract_meta(bytes: &[u8]) -> AppResult<ContractMeta> {
    let mut meta = ContractMeta::default();

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        if let wasmparser::Payload::CustomSection(section) = payload {
            if section.name() != "contractmetav0" {
                continue;
            }

            let data = section.data();
            let mut cursor = Cursor::new(data);
            while (cursor.position() as usize) < data.len() {
                let mut limited =
                    stellar_xdr::Limited::new(&mut cursor, stellar_xdr::Limits::none());
                // Break (not `?`) on a decode error, matching
                // `parse_contract_spec`: entries already decoded are kept and
                // a malformed tail degrades gracefully to what we have.
                let Ok(entry) = stellar_xdr::ScMetaEntry::read_xdr(&mut limited) else {
                    break;
                };
                let stellar_xdr::ScMetaEntry::ScMetaV0(v) = entry;
                let key = String::from_utf8_lossy(v.key.as_slice()).to_string();
                let val = String::from_utf8_lossy(v.val.as_slice()).to_string();
                match key.as_str() {
                    "name" => meta.name = Some(val.clone()),
                    "version" => meta.version = Some(val.clone()),
                    "description" | "desc" => meta.description = Some(val.clone()),
                    _ => {}
                }
                meta.entries.push((key, val));
            }
        }
    }

    Ok(meta)
}

/// Formats the parsed contract metadata for display.
///
/// Prints the recognized fields (name/version/description) followed by any
/// additional custom key/value pairs, so no metadata is hidden. WASMs without
/// a section produce a single "absent" line.
#[must_use]
pub fn format_contract_meta(meta: &ContractMeta) -> String {
    if meta.entries.is_empty() {
        return "Contract meta: absent".to_string();
    }

    let mut lines = vec!["Contract meta: present".to_string()];
    if let Some(name) = &meta.name {
        lines.push(format!("  name: {name}"));
    }
    if let Some(version) = &meta.version {
        lines.push(format!("  version: {version}"));
    }
    if let Some(description) = &meta.description {
        lines.push(format!("  description: {description}"));
    }
    for (key, val) in &meta.entries {
        if !matches!(key.as_str(), "name" | "version" | "description" | "desc") {
            lines.push(format!("  {key}: {val}"));
        }
    }
    lines.join("\n")
}

/// Human-readable name for a `ScSpecTypeDef`.
#[must_use]
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
    /// The raw spec type definition, used for `--arg` value validation.
    pub type_def: stellar_xdr::ScSpecTypeDef,
}

/// Validates a single `--arg` `key=value` pair against a contract-spec type.
///
/// When a contract spec is present the declared type is authoritative, so a
/// value that cannot represent that type (e.g. `abc` for `i64`) is rejected
/// before any RPC simulation is attempted. Bare values and `key=value` forms
/// are both accepted; the key is informational and ignored.
///
/// "Out of scope" types (custom user-defined types, val/void/vec/map/tuple,
/// options, results) cannot be validated without a bespoke parser and are
/// accepted as-is.
pub fn validate_arg_value(type_def: &stellar_xdr::ScSpecTypeDef, arg: &str) -> AppResult<()> {
    let value = arg.split_once('=').map(|(_, v)| v).unwrap_or(arg);
    let expected = spec_type_name(type_def);

    let ok = match type_def {
        stellar_xdr::ScSpecTypeDef::Bool => value == "true" || value == "false",
        stellar_xdr::ScSpecTypeDef::U32 => value.parse::<u32>().is_ok(),
        stellar_xdr::ScSpecTypeDef::I32 => value.parse::<i32>().is_ok(),
        stellar_xdr::ScSpecTypeDef::U64
        | stellar_xdr::ScSpecTypeDef::Timepoint
        | stellar_xdr::ScSpecTypeDef::Duration => value.parse::<u64>().is_ok(),
        stellar_xdr::ScSpecTypeDef::I64 => value.parse::<i64>().is_ok(),
        stellar_xdr::ScSpecTypeDef::U128 => value.parse::<u128>().is_ok(),
        stellar_xdr::ScSpecTypeDef::I128 => value.parse::<i128>().is_ok(),
        stellar_xdr::ScSpecTypeDef::U256 | stellar_xdr::ScSpecTypeDef::I256 => {
            is_wide_integer(value)
        }
        stellar_xdr::ScSpecTypeDef::Symbol => is_valid_symbol(value),
        stellar_xdr::ScSpecTypeDef::Bytes => is_valid_hex(value),
        stellar_xdr::ScSpecTypeDef::BytesN(spec) => {
            let hex = value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
                .unwrap_or(value);
            is_valid_hex(value) && Some(hex.len() / 2) == usize::try_from(spec.n).ok()
        }
        stellar_xdr::ScSpecTypeDef::Address => is_valid_address(value),
        // Bare strings, and types that carry no validator: bespoke parsers
        // are out of scope.
        stellar_xdr::ScSpecTypeDef::String
        | stellar_xdr::ScSpecTypeDef::Val
        | stellar_xdr::ScSpecTypeDef::Void
        | stellar_xdr::ScSpecTypeDef::Error
        | stellar_xdr::ScSpecTypeDef::MuxedAddress
        | stellar_xdr::ScSpecTypeDef::Option(_)
        | stellar_xdr::ScSpecTypeDef::Result(_)
        | stellar_xdr::ScSpecTypeDef::Vec(_)
        | stellar_xdr::ScSpecTypeDef::Map(_)
        | stellar_xdr::ScSpecTypeDef::Tuple(_)
        | stellar_xdr::ScSpecTypeDef::Udt(_) => true,
    };

    if !ok {
        return Err(AppError::TypeValidation(format!(
            "arg '{arg}' cannot be used as '{expected}'"
        )));
    }
    Ok(())
}

/// Coerces a single `--arg` `key=value` pair to an `ScVal` using a
/// contract-spec type.
///
/// The spec type is authoritative when it has a text form that maps directly
/// onto an `ScVal` (bool, common fixed-width integers, string, symbol). For
/// those types a value that cannot represent the spec type is rejected early
/// with the parameter's `key=value` text and the expected type name. Spec
/// types that require a bespoke parser (vec, map, udt, ...) return `Ok(None)`
/// so callers can fall back to the legacy type-inference path.
pub fn coerce_arg_scval(
    type_def: &stellar_xdr::ScSpecTypeDef,
    arg: &str,
) -> AppResult<Option<stellar_xdr::ScVal>> {
    let value = arg.split_once('=').map(|(_, v)| v).unwrap_or(arg);
    let expected = spec_type_name(type_def);
    let invalid = || {
        AppError::TypeValidation(format!(
            "arg '{arg}' cannot be used as '{expected}'"
        ))
    };

    let coerced = match type_def {
        stellar_xdr::ScSpecTypeDef::Bool => {
            if value == "true" {
                Some(ScVal::Bool(true))
            } else if value == "false" {
                Some(ScVal::Bool(false))
            } else {
                return Err(invalid());
            }
        }
        stellar_xdr::ScSpecTypeDef::U32 => Some(ScVal::U32(
            value.parse::<u32>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::I32 => Some(ScVal::I32(
            value.parse::<i32>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::U64 => Some(ScVal::U64(
            value.parse::<u64>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::I64 => Some(ScVal::I64(
            value.parse::<i64>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::Timepoint => Some(ScVal::Timepoint(
            value.parse::<u64>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::Duration => Some(ScVal::Duration(
            value.parse::<u64>().map_err(|_| invalid())?,
        )),
        stellar_xdr::ScSpecTypeDef::U128 => {
            let n = value.parse::<u128>().map_err(|_| invalid())?;
            Some(ScVal::U128(stellar_xdr::UInt128Parts {
                hi: (n >> 64) as u64,
                lo: n as u64,
            }))
        }
        stellar_xdr::ScSpecTypeDef::I128 => {
            let n = value.parse::<i128>().map_err(|_| invalid())?;
            Some(ScVal::I128(stellar_xdr::Int128Parts {
                hi: (n >> 64) as i64,
                lo: n as u64,
            }))
        }
        stellar_xdr::ScSpecTypeDef::Symbol => {
            if !is_valid_symbol(value) {
                return Err(invalid());
            }
            Some(ScVal::Symbol(stellar_xdr::ScSymbol(
                value.as_bytes().to_vec(),
            )))
        }
        stellar_xdr::ScSpecTypeDef::String => Some(ScVal::String(stellar_xdr::ScString(
            value.as_bytes().to_vec(),
        ))),
        // Types with no trivial text->ScVal mapping are left to the caller's
        // inference fallback.
        _ => None,
    };

    Ok(coerced)
}

/// True when `value` is a plausible u256/i256 integer: optional `0x` hex or a
/// plain decimal without sign-ambiguity issues. A full 256-bit parse is out of
/// scope, so this is a conservative syntax check.
#[must_use]
pub fn is_wide_integer(value: &str) -> bool {
    let digits = value.strip_prefix("-").unwrap_or(value);
    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
    }
}

/// True when `value` is a valid Soroban symbol: 1..=32 chars of `[A-Za-z0-9_]`.
#[must_use]
pub fn is_valid_symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True when `value` is an even-length, non-empty lowercase hex string (either
/// bare or `0x`-prefixed).
#[must_use]
pub fn is_valid_hex(value: &str) -> bool {
    let stripped = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    !stripped.is_empty()
        && stripped.len() % 2 == 0
        && stripped.chars().all(|c| c.is_ascii_hexdigit())
}

/// True when `value` looks like a Stellar strkey address (`C…` contract or
/// `G…` account; both are 56 chars) or a 64-hex-char contract id.
#[must_use]
pub fn is_valid_address(value: &str) -> bool {
    let c_g = matches!(value.as_bytes().first(), Some(b'C' | b'G')) && value.len() == 56;
    let hex_id = value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit());
    c_g || hex_id
}

/// Information about a linear memory declared by a WASM module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryInfo {
    /// Initial size of this memory, in WASM pages.
    pub initial_pages: u64,
    /// Optional maximum size, in WASM pages (`None` = unbounded).
    pub maximum_pages: Option<u64>,
    /// Whether this is a 64-bit (`i64` indexed) memory.
    pub memory64: bool,
}

/// Information about an import declared by a WASM module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportInfo {
    /// Module name the import is pulled from.
    pub module: String,
    /// Name of the imported item.
    pub name: String,
    /// Human-readable kind, e.g. `function`, `memory`, `global`.
    pub kind: String,
}

/// Information about an export declared by a WASM module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    /// Name of the exported item.
    pub name: String,
    /// Human-readable kind, e.g. `function`, `memory`, `global`.
    pub kind: String,
    /// Index of the exported item in its index space.
    pub index: u32,
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
#[must_use]
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
    /// Contract metadata parsed from the `contractmetaV0` custom section,
    /// when present.
    pub contract_meta: ContractMeta,
    /// Index of the module start function, if one is declared.
    pub start_function: Option<u32>,
    /// Linear memories and their limits.
    pub memories: Vec<MemoryInfo>,
    /// Imports declared by the module (`module::name` → kind).
    pub imports: Vec<ImportInfo>,
    /// Exports declared by the module, including non-function exports.
    pub exports: Vec<ExportInfo>,
}

/// Formats a human-readable diagnostic summary of a loaded module: the start
/// function, memory limits, and the import/export structure.
///
/// List-heavy sections are truncated (at most 10 entries each) to keep the
/// output usable for real contracts.
#[must_use]
pub fn format_module_metadata(info: &WasmInfo) -> String {
    const MAX_LISTED_ENTRIES: usize = 10;

    let mut lines = Vec::new();
    lines.push("WASM module metadata:".to_string());
    match info.start_function {
        Some(idx) => lines.push(format!("- start function: index {idx}")),
        None => lines.push("- start function: none".to_string()),
    }
    if info.memories.is_empty() {
        lines.push("- memories: none".to_string());
    } else {
        for memory in &info.memories {
            let addr = if memory.memory64 { "64-bit" } else { "32-bit" };
            match memory.maximum_pages {
                Some(max) => lines.push(format!(
                    "- memories: {addr}, initial {} pages, max {max} pages",
                    memory.initial_pages
                )),
                None => lines.push(format!(
                    "- memories: {addr}, initial {} pages, unbounded",
                    memory.initial_pages
                )),
            }
        }
    }
    push_entries_generic(
        &mut lines,
        "imports",
        &info.imports,
        MAX_LISTED_ENTRIES,
        |imp| format!("{}::{} ({})", imp.module, imp.name, imp.kind),
    );
    push_entries_generic(
        &mut lines,
        "exports",
        &info.exports,
        MAX_LISTED_ENTRIES,
        |ex| format!("{} ({}) index {}", ex.name, ex.kind, ex.index),
    );
    lines.join("\n")
}

/// Appends a counted, truncated list to `lines`, formatted by `fmt`.
fn push_entries_generic<T>(
    lines: &mut Vec<String>,
    label: &str,
    entries: &[T],
    max_listed: usize,
    fmt: impl Fn(&T) -> String,
) {
    lines.push(format!("- {label}: {} entry(ies)", entries.len()));
    let shown = entries.len().min(max_listed);
    for entry in &entries[..shown] {
        lines.push(format!("  - {}", fmt(entry)));
    }
    if entries.len() > max_listed {
        lines.push(format!("  - ... and {} more", entries.len() - max_listed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{ScSpecTypeDef, ScVal};

    fn coerce(type_def: ScSpecTypeDef, arg: &str) -> Option<ScVal> {
        coerce_arg_scval(&type_def, arg).expect("coercion should be valid")
    }

    #[test]
    fn coerces_i64_args() {
        assert_eq!(coerce(ScSpecTypeDef::I64, "step=5"), Some(ScVal::I64(5)));
    }

    #[test]
    fn coerces_u64_args() {
        assert_eq!(
            coerce(ScSpecTypeDef::U64, "amount=10"),
            Some(ScVal::U64(10))
        );
    }

    #[test]
    fn coerces_bool_args() {
        assert_eq!(
            coerce(ScSpecTypeDef::Bool, "flag=true"),
            Some(ScVal::Bool(true))
        );
    }

    #[test]
    fn coerces_string_args() {
        assert_eq!(
            coerce(ScSpecTypeDef::String, "name=hello"),
            Some(ScVal::String(stellar_xdr::ScString(b"hello".to_vec())))
        );
    }

    #[test]
    fn coerces_symbol_args() {
        assert_eq!(
            coerce(ScSpecTypeDef::Symbol, "symbol=hello_world"),
            Some(ScVal::Symbol(stellar_xdr::ScSymbol(
                b"hello_world".to_vec()
            )))
        );
    }
}
