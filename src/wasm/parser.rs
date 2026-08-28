use std::io::Cursor;
use std::path::Path;

use stellar_xdr::ReadXdr;
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
