use std::io::Cursor;
use std::path::Path;

use serde_json::json;
use stellar_xdr::ReadXdr;
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};

/// Size of the fixed WASM module header: the 4-byte magic number
/// (`\0asm`) plus the 4-byte version field. Every byte of a module lives
/// either in this header or in exactly one section, so section sizes that
/// include their own headers add up to the file length.
pub const WASM_MODULE_HEADER_SIZE: usize = 8;

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

    let ModuleMetadata {
        mut functions,
        start_function,
        memories,
        imports,
        exports,
        sections,
    } = enumerate_module_metadata(&bytes)?;
    let (spec_entries, has_spec) = parse_contract_spec_entries(&bytes)?;
    let spec_functions = spec_functions_from_entries(&spec_entries);
    let contract_meta = parse_contract_meta(&bytes)?;

    if !spec_functions.is_empty() {
        for fn_info in &mut functions {
            if let Some(spec_fn) = spec_functions.iter().find(|f| f.name == fn_info.name) {
                fn_info.params = spec_fn.inputs.clone();
                fn_info.param_count = spec_fn.inputs.len() as u32;
                fn_info.returns = spec_fn.outputs.clone();
                fn_info.result_count = spec_fn.outputs.len() as u32;
            }
        }
    }

    trace!(functions = functions.len(), has_spec, "WASM parsed");
    Ok(WasmInfo {
        bytes,
        functions,
        has_spec,
        spec_entries,
        contract_meta,
        start_function,
        memories,
        imports,
        exports,
        sections,
    })
}

/// Basic structural validation of a WASM binary.
pub fn validate_wasm(bytes: &[u8]) -> AppResult<()> {
    wasmparser::validate(bytes)
        .map_err(|e| AppError::WasmValidation(format!("not a valid WebAssembly binary: {e}")))?;
    Ok(())
}

/// Enumerates exported function names from a validated WASM binary.
pub fn enumerate_functions(bytes: &[u8]) -> AppResult<Vec<FunctionInfo>> {
    Ok(enumerate_module_metadata(bytes)?.functions)
}

/// Builds the section size breakdown straight from WASM bytes, validating the
/// module first so malformed input fails with a clear error.
pub fn section_size_breakdown(bytes: &[u8]) -> AppResult<SectionSizeBreakdown> {
    validate_wasm(bytes)?;
    let metadata = enumerate_module_metadata(bytes)?;
    Ok(SectionSizeBreakdown::from_sections(
        &metadata.sections,
        bytes.len(),
    ))
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
    /// Every section found in the binary, with its name and byte size.
    pub sections: Vec<SectionInfo>,
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
    let mut sections = Vec::new();
    let mut imported_function_count = 0usize;
    // End of the previous section's contents, starting just after the
    // 8-byte magic + version header. The gap to the next section's contents
    // is that section's header (id byte + LEB128 size prefix).
    let mut cursor = WASM_MODULE_HEADER_SIZE;

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        if let Some((id, range)) = payload.as_section() {
            let name = match &payload {
                wasmparser::Payload::CustomSection(section) => section.name().to_string(),
                _ => section_id_name(id).to_string(),
            };
            sections.push(SectionInfo {
                id,
                name,
                custom: matches!(&payload, wasmparser::Payload::CustomSection(_)),
                offset: range.start,
                end: range.end,
                size: range.end - range.start,
                header_size: range.start.saturating_sub(cursor),
            });
            cursor = range.end;
        }
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
                        let defined_idx =
                            (export.index as usize).checked_sub(imported_function_count);
                        let (param_count, result_count) = defined_idx
                            .and_then(|idx| func_to_type.get(idx))
                            .and_then(|&type_idx| type_infos.get(type_idx as usize).copied())
                            .unwrap_or((0, 0));
                        functions.push(FunctionInfo {
                            name: export.name.to_string(),
                            param_count,
                            result_count,
                            params: Vec::new(),
                            returns: Vec::new(),
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
                            imported_function_count +=
                                usize::from(is_function_type_ref(&imported.ty));
                            imports.push(ImportInfo {
                                module: imported.module.to_string(),
                                name: imported.name.to_string(),
                                kind: type_ref_kind_name(&imported.ty).to_string(),
                            });
                        }
                        wasmparser::Imports::Compact1 { module, items } => {
                            for item in items {
                                let item = item.map_err(|e| AppError::WasmParse(e.to_string()))?;
                                imported_function_count +=
                                    usize::from(is_function_type_ref(&item.ty));
                                imports.push(ImportInfo {
                                    module: module.to_string(),
                                    name: item.name.to_string(),
                                    kind: type_ref_kind_name(&item.ty).to_string(),
                                });
                            }
                        }
                        wasmparser::Imports::Compact2 { module, ty, names } => {
                            imported_function_count +=
                                names.count() as usize * usize::from(is_function_type_ref(&ty));
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
        sections,
    })
}

/// Human-readable name for a WebAssembly section id.
#[must_use]
pub fn section_id_name(id: u8) -> &'static str {
    match id {
        0 => "custom",
        1 => "type",
        2 => "import",
        3 => "function",
        4 => "table",
        5 => "memory",
        6 => "global",
        7 => "export",
        8 => "start",
        9 => "element",
        10 => "code",
        11 => "data",
        12 => "data count",
        13 => "tag",
        _ => "unknown",
    }
}

fn is_function_type_ref(ty: &wasmparser::TypeRef) -> bool {
    matches!(
        ty,
        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
    )
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

/// A typed value (function return, or any other spec type) with both its
/// human-readable name and the raw spec type definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// Human-readable Soroban type, e.g. `i64`, `symbol`, `string`.
    pub type_name: String,
    /// The raw spec type definition, used for value validation.
    pub type_def: stellar_xdr::ScSpecTypeDef,
}

/// One function entry decoded from a `contractspecv0` custom section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFunctionInfo {
    /// Exported contract function name.
    pub name: String,
    /// Doc string attached to the function, when present.
    pub doc: String,
    /// Typed input parameters.
    pub inputs: Vec<ParamInfo>,
    /// Typed return values (a contract function returns at most one value).
    pub outputs: Vec<TypeInfo>,
}

/// Decodes the Soroban contract spec (`contractspecv0` custom section) into
/// the function entries that carry typed parameters.
pub fn parse_contract_spec(bytes: &[u8]) -> AppResult<(SpecFunctions, bool)> {
    let (entries, has_spec) = parse_contract_spec_entries(bytes)?;
    let functions = spec_functions_from_entries(&entries)
        .into_iter()
        .map(|f| (f.name, f.inputs))
        .collect();
    Ok((functions, has_spec))
}

/// Decodes the `contractspecv0` custom section into function entries with
/// typed parameters *and* return types.
pub fn parse_contract_spec_functions(bytes: &[u8]) -> AppResult<(Vec<SpecFunctionInfo>, bool)> {
    let (entries, has_spec) = parse_contract_spec_entries(bytes)?;
    Ok((spec_functions_from_entries(&entries), has_spec))
}

/// Decodes every `ScSpecEntry` from the `contractspecv0` custom section.
///
/// The section payload is **not** a count-prefixed `VecM<ScSpecEntry>`: it is
/// a concatenation of raw `ScSpecEntry` XDR values, each starting with its
/// 4-byte union discriminant (e.g. `00 00 00 00` = FunctionV0). Entries are
/// therefore decoded one at a time from a cursor until the section payload is
/// exhausted. All entry kinds are preserved (functions, UDT structs, unions,
/// enums, error enums, and events) so callers can render the full spec.
pub fn parse_contract_spec_entries(
    bytes: &[u8],
) -> AppResult<(Vec<stellar_xdr::ScSpecEntry>, bool)> {
    let mut entries = Vec::new();
    let mut has_spec = false;

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        let wasmparser::Payload::CustomSection(section) = payload else {
            continue;
        };
        if section.name() != "contractspecv0" {
            continue;
        }
        has_spec = true;

        let data = section.data();
        let mut cursor = Cursor::new(data);
        while (cursor.position() as usize) < data.len() {
            let mut limited = stellar_xdr::Limited::new(&mut cursor, stellar_xdr::Limits::none());
            let entry = stellar_xdr::ScSpecEntry::read_xdr(&mut limited).map_err(|e| {
                AppError::WasmParse(format!(
                    "failed to decode contractspecv0 entry {}: {e}",
                    entries.len()
                ))
            })?;
            entries.push(entry);
        }
    }

    Ok((entries, has_spec))
}

fn spec_functions_from_entries(entries: &[stellar_xdr::ScSpecEntry]) -> Vec<SpecFunctionInfo> {
    entries
        .iter()
        .filter_map(|entry| {
            let stellar_xdr::ScSpecEntry::FunctionV0(f) = entry else {
                return None;
            };
            Some(SpecFunctionInfo {
                name: String::from_utf8_lossy(f.name.as_slice()).to_string(),
                doc: String::from_utf8_lossy(f.doc.as_slice()).to_string(),
                inputs: f
                    .inputs
                    .iter()
                    .map(|input| ParamInfo {
                        name: String::from_utf8_lossy(input.name.as_slice()).to_string(),
                        type_name: spec_type_name(&input.type_).to_string(),
                        type_def: input.type_.clone(),
                    })
                    .collect(),
                outputs: f
                    .outputs
                    .iter()
                    .map(|output| TypeInfo {
                        type_name: spec_type_name(output).to_string(),
                        type_def: output.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
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
    /// Soroban SDK version the contract was built with, when the section
    /// carries one of the SDK-version keys.
    pub sdk_version: Option<String>,
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

    /// Looks up a metadata key by name, preserving the original casing.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

const SDK_VERSION_KEYS: [&str; 5] = [
    "rssdkver",
    "sdkver",
    "sdk_version",
    "soroban_sdk_version",
    "soroban-sdk-version",
];

/// Parses the Soroban contract metadata (`contractmetaV0` custom section).
///
/// Returns the parsed name/version/description/SDK version plus the full
/// ordered list of key/value pairs. The section is optional — a WASM without
/// one yields an empty `ContractMeta`, never an error.
///
/// Like `contractspecv0`, the section payload is **not** a count-prefixed
/// vector: it is a concatenation of raw `ScMetaEntry` XDR union values, each
/// starting with its 4-byte union discriminant (`00 00 00 00` = `ScMetaV0`)
/// followed by the `{ key, val }` struct. Entries are decoded one at a time
/// from a cursor until the payload is exhausted.
pub fn parse_contract_meta(bytes: &[u8]) -> AppResult<ContractMeta> {
    let mut meta = ContractMeta::default();

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        let wasmparser::Payload::CustomSection(section) = payload else {
            continue;
        };
        if section.name() != "contractmetav0" {
            continue;
        }

        let data = section.data();
        let mut cursor = Cursor::new(data);
        while (cursor.position() as usize) < data.len() {
            let mut limited = stellar_xdr::Limited::new(&mut cursor, stellar_xdr::Limits::none());
            let entry = stellar_xdr::ScMetaEntry::read_xdr(&mut limited).map_err(|e| {
                AppError::WasmParse(format!(
                    "failed to decode contractmetav0 entry {}: {e}",
                    meta.entries.len()
                ))
            })?;
            let stellar_xdr::ScMetaEntry::ScMetaV0(v) = entry;
            let key = String::from_utf8_lossy(v.key.as_slice()).to_string();
            let val = String::from_utf8_lossy(v.val.as_slice()).to_string();
            match key.as_str() {
                "name" => meta.name = Some(val.clone()),
                "version" => meta.version = Some(val.clone()),
                "description" | "desc" => meta.description = Some(val.clone()),
                _ if SDK_VERSION_KEYS.contains(&key.as_str()) => {
                    meta.sdk_version = Some(val.clone());
                }
                _ => {}
            }
            meta.entries.push((key, val));
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

/// Information about a WebAssembly section found in the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionInfo {
    /// Raw section id from the binary (`0` = custom).
    pub id: u8,
    /// Canonical section name, or the custom section's own name.
    pub name: String,
    /// Whether this is a custom section (id `0`).
    pub custom: bool,
    /// Byte offset of the section contents in the binary.
    pub offset: usize,
    /// Byte offset one past the end of the section contents.
    pub end: usize,
    /// Size of the section contents, in bytes.
    pub size: usize,
    /// Bytes taken by the section header itself: the section id byte plus its
    /// LEB128-encoded size prefix. `header_size + size` is the section's
    /// full footprint in the binary.
    pub header_size: usize,
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
    /// Typed return values from the contract spec, if the WASM has one.
    pub returns: Vec<TypeInfo>,
}

/// Renders a spec type definition as JSON, preserving the full type tree
/// (nested `option`/`result`/`vec`/`map`/`tuple`/`bytes_n`/`udt` types keep
/// their structure instead of collapsing to a flat name).
#[must_use]
pub fn spec_type_json(type_def: &stellar_xdr::ScSpecTypeDef) -> serde_json::Value {
    use stellar_xdr::ScSpecTypeDef as Type;
    match type_def {
        Type::Option(inner) => json!({
            "type": "option",
            "inner": spec_type_json(&inner.value_type),
        }),
        Type::Result(inner) => json!({
            "type": "result",
            "ok": spec_type_json(&inner.ok_type),
            "error": spec_type_json(&inner.error_type),
        }),
        Type::Vec(inner) => json!({
            "type": "vec",
            "element": spec_type_json(&inner.element_type),
        }),
        Type::Map(inner) => json!({
            "type": "map",
            "key": spec_type_json(&inner.key_type),
            "value": spec_type_json(&inner.value_type),
        }),
        Type::Tuple(inner) => json!({
            "type": "tuple",
            "elements": inner.value_types.iter().map(spec_type_json).collect::<Vec<_>>(),
        }),
        Type::BytesN(inner) => json!({
            "type": "bytes_n",
            "length": inner.n,
        }),
        Type::Udt(inner) => json!({
            "type": "udt",
            "name": String::from_utf8_lossy(inner.name.as_slice()),
        }),
        other => json!({ "type": spec_type_name(other) }),
    }
}

/// Renders a single `contractspecv0` entry as JSON, covering every entry kind
/// the contract spec can carry (functions, UDTs, enums, error enums, events).
#[must_use]
pub fn spec_entry_json(entry: &stellar_xdr::ScSpecEntry) -> serde_json::Value {
    use stellar_xdr::ScSpecEntry as Entry;
    match entry {
        Entry::FunctionV0(f) => json!({
            "kind": "function",
            "name": String::from_utf8_lossy(f.name.as_slice()),
            "doc": String::from_utf8_lossy(f.doc.as_slice()),
            "inputs": f.inputs.iter().map(|input| json!({
                "name": String::from_utf8_lossy(input.name.as_slice()),
                "doc": String::from_utf8_lossy(input.doc.as_slice()),
                "type": spec_type_name(&input.type_),
                "type_def": spec_type_json(&input.type_),
            })).collect::<Vec<_>>(),
            "outputs": f.outputs.iter().map(|output| json!({
                "type": spec_type_name(output),
                "type_def": spec_type_json(output),
            })).collect::<Vec<_>>(),
        }),
        Entry::UdtStructV0(udt) => json!({
            "kind": "udt_struct",
            "name": String::from_utf8_lossy(udt.name.as_slice()),
            "lib": String::from_utf8_lossy(udt.lib.as_slice()),
            "doc": String::from_utf8_lossy(udt.doc.as_slice()),
            "fields": udt.fields.iter().map(|field| json!({
                "name": String::from_utf8_lossy(field.name.as_slice()),
                "doc": String::from_utf8_lossy(field.doc.as_slice()),
                "type": spec_type_name(&field.type_),
                "type_def": spec_type_json(&field.type_),
            })).collect::<Vec<_>>(),
        }),
        Entry::UdtUnionV0(udt) => json!({
            "kind": "udt_union",
            "name": String::from_utf8_lossy(udt.name.as_slice()),
            "lib": String::from_utf8_lossy(udt.lib.as_slice()),
            "doc": String::from_utf8_lossy(udt.doc.as_slice()),
            "cases": udt.cases.iter().map(udt_union_case_json).collect::<Vec<_>>(),
        }),
        Entry::UdtEnumV0(udt) => json!({
            "kind": "udt_enum",
            "name": String::from_utf8_lossy(udt.name.as_slice()),
            "lib": String::from_utf8_lossy(udt.lib.as_slice()),
            "doc": String::from_utf8_lossy(udt.doc.as_slice()),
            "cases": udt.cases.iter().map(|case| json!({
                "name": String::from_utf8_lossy(case.name.as_slice()),
                "doc": String::from_utf8_lossy(case.doc.as_slice()),
                "value": case.value,
            })).collect::<Vec<_>>(),
        }),
        Entry::UdtErrorEnumV0(udt) => json!({
            "kind": "udt_error_enum",
            "name": String::from_utf8_lossy(udt.name.as_slice()),
            "lib": String::from_utf8_lossy(udt.lib.as_slice()),
            "doc": String::from_utf8_lossy(udt.doc.as_slice()),
            "cases": udt.cases.iter().map(|case| json!({
                "name": String::from_utf8_lossy(case.name.as_slice()),
                "doc": String::from_utf8_lossy(case.doc.as_slice()),
                "value": case.value,
            })).collect::<Vec<_>>(),
        }),
        Entry::EventV0(event) => json!({
            "kind": "event",
            "name": String::from_utf8_lossy(event.name.as_slice()),
            "lib": String::from_utf8_lossy(event.lib.as_slice()),
            "doc": String::from_utf8_lossy(event.doc.as_slice()),
            "prefix_topics": event.prefix_topics.iter()
                .map(|topic| String::from_utf8_lossy(topic.as_slice()))
                .collect::<Vec<_>>(),
            "data_format": match event.data_format {
                stellar_xdr::ScSpecEventDataFormat::SingleValue => "single_value",
                stellar_xdr::ScSpecEventDataFormat::Vec => "vec",
                stellar_xdr::ScSpecEventDataFormat::Map => "map",
            },
            "params": event.params.iter().map(|param| json!({
                "name": String::from_utf8_lossy(param.name.as_slice()),
                "doc": String::from_utf8_lossy(param.doc.as_slice()),
                "type": spec_type_name(&param.type_),
                "type_def": spec_type_json(&param.type_),
                "location": match param.location {
                    stellar_xdr::ScSpecEventParamLocationV0::Data => "data",
                    stellar_xdr::ScSpecEventParamLocationV0::TopicList => "topic",
                },
            })).collect::<Vec<_>>(),
        }),
    }
}

fn udt_union_case_json(case: &stellar_xdr::ScSpecUdtUnionCaseV0) -> serde_json::Value {
    use stellar_xdr::ScSpecUdtUnionCaseV0 as Case;
    match case {
        Case::VoidV0(case) => json!({
            "kind": "void",
            "name": String::from_utf8_lossy(case.name.as_slice()),
            "doc": String::from_utf8_lossy(case.doc.as_slice()),
        }),
        Case::TupleV0(case) => json!({
            "kind": "tuple",
            "name": String::from_utf8_lossy(case.name.as_slice()),
            "doc": String::from_utf8_lossy(case.doc.as_slice()),
            "types": case.type_.iter()
                .map(spec_type_json)
                .collect::<Vec<_>>(),
        }),
    }
}

/// Formats a function with its spec-derived signature, e.g.
/// `increment(step: i64) -> i64`.
#[must_use]
pub fn format_function(fn_info: &FunctionInfo) -> String {
    let params = fn_info
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut signature = format!("{}({params})", fn_info.name);
    if !fn_info.returns.is_empty() {
        let returns = fn_info
            .returns
            .iter()
            .map(|r| r.type_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        signature = format!("{signature} -> {returns}");
    }
    signature
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
    /// Every entry decoded from the `contractspecv0` custom section, in
    /// section order (functions, UDTs, enums, error enums, and events).
    pub spec_entries: Vec<stellar_xdr::ScSpecEntry>,
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
    /// Every section in the binary, with its name and byte size.
    pub sections: Vec<SectionInfo>,
}

impl WasmInfo {
    /// Size accounting for the whole file: the module header plus one entry
    /// per section, each with its share of the file.
    #[must_use]
    pub fn section_breakdown(&self) -> SectionSizeBreakdown {
        SectionSizeBreakdown::from_sections(&self.sections, self.bytes.len())
    }
}

/// Byte accounting for one part of a WASM file: either a section (with its
/// header) or the fixed module header.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SectionSize {
    /// Section name (`code`, `data`, `contractspecv0`, …), or
    /// [`MODULE_HEADER_LABEL`] for the fixed module header.
    pub name: String,
    /// Raw section id (`0` = custom), or `None` for the module header.
    pub id: Option<u8>,
    /// Byte offset of the section contents, or `None` for the module header.
    pub offset: Option<usize>,
    /// Byte offset one past the end of the section contents.
    pub end: Option<usize>,
    /// Size of the section contents, in bytes (`0` for the module header).
    pub size: usize,
    /// Bytes taken by the section header, in bytes.
    pub header_size: usize,
    /// Full footprint: `header_size + size`.
    pub total_size: usize,
    /// Share of the whole file, in percent (`0.0`–`100.0`).
    pub percent: f64,
}

/// Label used for the fixed 8-byte module header in size reports.
pub const MODULE_HEADER_LABEL: &str = "module header";

/// Size accounting for a whole WASM file.
///
/// A module is the 8-byte magic + version header followed by sections, so
/// `header_size` plus the sum of every entry's `total_size` equals the file
/// length exactly. Entries are sorted largest first, which is what makes a
/// bloated `contractspecv0` or an oversized data segment obvious.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SectionSizeBreakdown {
    /// Total size of the file, in bytes.
    pub total_size: usize,
    /// Size of the fixed module header, in bytes.
    pub header_size: usize,
    /// Per-section and module-header entries, largest first.
    pub sections: Vec<SectionSize>,
}

impl SectionSizeBreakdown {
    /// Builds a breakdown from the sections collected by
    /// [`enumerate_module_metadata`] and the total file length.
    ///
    /// Sections are ordered by byte offset first, so the header sizes stay
    /// correct no matter what order the caller supplies them in.
    #[must_use]
    pub fn from_sections(sections: &[SectionInfo], total_size: usize) -> Self {
        let mut ordered: Vec<&SectionInfo> = sections.iter().collect();
        ordered.sort_by_key(|s| s.offset);
        let sections: Vec<SectionInfo> = ordered.into_iter().cloned().collect();
        let mut cursor = WASM_MODULE_HEADER_SIZE;
        let mut entries: Vec<SectionSize> = Vec::with_capacity(sections.len() + 1);
        entries.push(SectionSize {
            name: MODULE_HEADER_LABEL.to_string(),
            id: None,
            offset: None,
            end: None,
            size: 0,
            header_size: WASM_MODULE_HEADER_SIZE,
            total_size: WASM_MODULE_HEADER_SIZE,
            percent: percent_of(WASM_MODULE_HEADER_SIZE, total_size),
        });
        for section in &sections {
            let header_size = section
                .offset
                .saturating_sub(cursor)
                .max(section.header_size);
            cursor = section.end;
            entries.push(SectionSize {
                name: section.name.clone(),
                id: Some(section.id),
                offset: Some(section.offset),
                end: Some(section.end),
                size: section.size,
                header_size,
                total_size: header_size + section.size,
                percent: percent_of(header_size + section.size, total_size),
            });
        }
        // Any bytes after the last section (never present in a validated
        // module, but possible in a hand-assembled binary) are reported
        // explicitly so the breakdown always adds up to the file length.
        if let Some(last) = sections.last() {
            if total_size > last.end {
                let trailing = total_size - last.end;
                entries.push(SectionSize {
                    name: "trailing bytes".to_string(),
                    id: None,
                    offset: Some(last.end),
                    end: Some(total_size),
                    size: trailing,
                    header_size: 0,
                    total_size: trailing,
                    percent: percent_of(trailing, total_size),
                });
            }
        }
        entries.sort_by(|a, b| {
            b.total_size
                .cmp(&a.total_size)
                .then_with(|| a.name.cmp(&b.name))
        });
        Self {
            total_size,
            header_size: WASM_MODULE_HEADER_SIZE,
            sections: entries,
        }
    }

    /// Total bytes taken by sections, excluding the module header.
    #[must_use]
    pub fn section_bytes(&self) -> usize {
        self.sections
            .iter()
            .filter(|s| s.id.is_some())
            .map(|s| s.total_size)
            .sum()
    }

    /// Bytes accounted for by the breakdown; equal to `total_size` whenever
    /// the module parses cleanly.
    #[must_use]
    pub fn accounted_bytes(&self) -> usize {
        self.sections.iter().map(|s| s.total_size).sum()
    }

    /// Name-to-bytes map of the whole breakdown, including the module header.
    ///
    /// The values sum to `total_size`, so a caller can reconcile the map
    /// against the file length. Repeated section names (two `name` custom
    /// sections, for instance) are suffixed with `#2`, `#3`, … so no bytes
    /// are lost.
    #[must_use]
    pub fn size_map(&self) -> std::collections::BTreeMap<String, usize> {
        let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        let mut map = std::collections::BTreeMap::new();
        for entry in &self.sections {
            let mut name = entry.name.clone();
            let count = seen.entry(name.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                name = format!("{}#{count}", entry.name);
            }
            map.insert(name, entry.total_size);
        }
        map
    }
}

/// Computes a share of the file in percent, rounded to two decimals.
fn percent_of(bytes: usize, total_size: usize) -> f64 {
    if total_size == 0 {
        return 0.0;
    }
    (bytes as f64 / total_size as f64 * 10000.0).round() / 100.0
}

impl std::fmt::Display for SectionSizeBreakdown {
    /// Renders an aligned table of the breakdown, largest section first.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.sections.iter().filter(|s| s.id.is_some()).count();
        let mut headline = format!(
            "WASM sections: {count} section(s), {} bytes of section data in a {} byte file \
             ({} byte module header)",
            self.section_bytes(),
            self.total_size,
            self.header_size
        );
        if self.accounted_bytes() != self.total_size {
            let unaccounted = self.total_size.saturating_sub(self.accounted_bytes());
            headline.push_str(&format!(" ({unaccounted} bytes unaccounted)"));
        }
        writeln!(f, "{headline}")?;
        let name_width = self
            .sections
            .iter()
            .map(|s| s.name.len())
            .max()
            .unwrap_or_default();
        let size_width = self
            .sections
            .iter()
            .map(|s| s.total_size.to_string().len())
            .max()
            .unwrap_or_default();
        writeln!(
            f,
            "  {:<name_width$}  {:>size_width$}  share",
            "section", "bytes"
        )?;
        for entry in &self.sections {
            writeln!(
                f,
                "  {:<name_width$}  {:>size_width$}  {:>5.1}%",
                entry.name, entry.total_size, entry.percent
            )?;
        }
        Ok(())
    }
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

/// Formats the section size breakdown of a loaded module as an aligned table
/// with each section's share of the file, largest first.
///
/// Rows include the section header bytes, so the sizes reconcile exactly with
/// the file length. Byte offsets for each section are available from
/// [`WasmInfo::sections`] and in the `wasm info --json` output.
#[must_use]
pub fn format_sections(info: &WasmInfo) -> String {
    info.section_breakdown().to_string().trim_end().to_string()
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
