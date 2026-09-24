use std::io::Cursor;
use std::path::Path;

use stellar_xdr::ReadXdr;
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};

/// Maximum initial linear memory, in WASM pages, expected for Soroban
/// contracts.
///
/// Contracts declaring more than this warn in `--verbose` / `--wasm-info`
/// output because excess initial memory drives up memory fees and
/// initialization costs.
pub const SOROBAN_MAX_MEMORY_PAGES: u64 = 16;

/// Size of one WASM linear-memory page in bytes (64 KiB).
pub const WASM_PAGE_SIZE_BYTES: u64 = 65_536;

/// Import module used by Soroban contracts for host functions (`env._` imports).
pub const HOST_IMPORT_MODULE: &str = "env";

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

    let functions = enumerate_functions(&bytes)?;
    let (spec_functions, has_spec) = parse_contract_spec(&bytes).unwrap_or_default();

    let mut functions = functions;
    if !spec_functions.is_empty() {
        for fn_info in &mut functions {
            if let Some((_, params)) = spec_functions.iter().find(|(n, _)| n == &fn_info.name) {
                fn_info.params = params.clone();
                fn_info.param_count = params.len() as u32;
            }
        }
    }

    trace!(functions = functions.len(), has_spec, "WASM parsed");
    let structure = parse_structure(&bytes)?;
    Ok(WasmInfo {
        bytes,
        functions,
        has_spec,
        structure,
    })
}

/// Basic structural validation of a WASM binary.
pub fn validate_wasm(bytes: &[u8]) -> AppResult<()> {
    wasmparser::validate(bytes).map_err(|e| AppError::WasmValidation(e.to_string()))?;
    Ok(())
}

/// Enumerates exported function names from a validated WASM binary.
pub fn enumerate_functions(bytes: &[u8]) -> AppResult<Vec<FunctionInfo>> {
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
    /// Structural summary: memories, host imports, start function, tables.
    pub structure: WasmStructureSummary,
}

/// Linear-memory limits declared in the WASM memory section, in pages.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemoryLimits {
    /// Initial linear-memory size in WASM pages (64 KiB each).
    pub initial_pages: u64,
    /// Optional maximum linear-memory size in WASM pages.
    pub maximum_pages: Option<u64>,
}

/// A host function imported from the `env` module (e.g. storage, crypto,
/// context functions).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportedHostFunction {
    /// Import module, always `"env"` for host functions.
    pub module: String,
    /// Imported function name (e.g. `"_" suffixed host dispatch names).
    pub name: String,
}

/// Table limits declared in the WASM table section, in elements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableSummary {
    /// Initial table size in elements.
    pub initial: u64,
    /// Optional maximum table size in elements.
    pub maximum: Option<u64>,
}

/// Structural summary of a WASM binary: entry points and memory layout.
///
/// Built by [`parse_structure`] via `wasmparser::Parser`, traversing
/// `Payload::MemorySection`, `Payload::ImportSection`, `Payload::ExportSection`,
/// `Payload::StartSection`, and `Payload::TableSection`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WasmStructureSummary {
    /// Memory limits from the memory section (usually zero or one entry).
    pub memories: Vec<MemoryLimits>,
    /// Host functions imported from the `env` module.
    pub imported_host_functions: Vec<ImportedHostFunction>,
    /// Total number of imported functions (any module).
    pub imported_function_count: u32,
    /// Total number of imports of any kind.
    pub total_import_count: u32,
    /// Names of exported functions (contract entry points).
    pub exported_functions: Vec<String>,
    /// Start function index, if the module declares one.
    pub start_function: Option<u32>,
    /// Table limits from the table section.
    pub tables: Vec<TableSummary>,
}

impl WasmStructureSummary {
    /// Initial memory pages of the first declared memory, if any.
    #[must_use]
    pub fn initial_memory_pages(&self) -> Option<u64> {
        self.memories.first().map(|m| m.initial_pages)
    }

    /// Maximum memory pages of the first declared memory, if any.
    #[must_use]
    pub fn maximum_memory_pages(&self) -> Option<u64> {
        self.memories.first().and_then(|m| m.maximum_pages)
    }

    /// True when any declared memory exceeds [`SOROBAN_MAX_MEMORY_PAGES`].
    #[must_use]
    pub fn initial_memory_exceeds_limit(&self) -> bool {
        self.memories
            .iter()
            .any(|m| m.initial_pages > SOROBAN_MAX_MEMORY_PAGES)
    }

    /// Human-readable warnings (e.g. excess initial memory).
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for memory in &self.memories {
            if memory.initial_pages > SOROBAN_MAX_MEMORY_PAGES {
                out.push(format!(
                    "initial memory ({} pages) exceeds standard Soroban limit of {} pages",
                    memory.initial_pages, SOROBAN_MAX_MEMORY_PAGES
                ));
            }
        }
        out
    }
}

/// Parses WASM structural information: memory limits, host imports,
/// exports, start function, and tables.
///
/// Returns a [`WasmStructureSummary`] for display in `--verbose` or
/// `--wasm-info` modes.
pub fn parse_structure(bytes: &[u8]) -> AppResult<WasmStructureSummary> {
    let mut memories: Vec<MemoryLimits> = Vec::new();
    let mut imported_host_functions: Vec<ImportedHostFunction> = Vec::new();
    let mut imported_function_count: u32 = 0;
    let mut total_import_count: u32 = 0;
    let mut exported_functions: Vec<String> = Vec::new();
    let mut start_function: Option<u32> = None;
    let mut tables: Vec<TableSummary> = Vec::new();

    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| AppError::WasmParse(e.to_string()))?;
        match payload {
            wasmparser::Payload::MemorySection(section) => {
                for memory in section {
                    let memory = memory.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    memories.push(MemoryLimits {
                        initial_pages: memory.initial,
                        maximum_pages: memory.maximum,
                    });
                }
            }
            wasmparser::Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    let import = import.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    total_import_count = total_import_count.saturating_add(1);
                    let is_func = matches!(
                        import.ty,
                        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                    );
                    if is_func {
                        imported_function_count = imported_function_count.saturating_add(1);
                        if import.module == HOST_IMPORT_MODULE {
                            imported_host_functions.push(ImportedHostFunction {
                                module: import.module.to_string(),
                                name: import.name.to_string(),
                            });
                        }
                    }
                }
            }
            wasmparser::Payload::ExportSection(section) => {
                for export in section {
                    let export = export.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    if matches!(export.kind, wasmparser::ExternalKind::Func) {
                        exported_functions.push(export.name.to_string());
                    }
                }
            }
            wasmparser::Payload::StartSection { func, .. } => {
                start_function = Some(func);
            }
            wasmparser::Payload::TableSection(section) => {
                for table in section {
                    let table = table.map_err(|e| AppError::WasmParse(e.to_string()))?;
                    tables.push(TableSummary {
                        initial: table.ty.initial,
                        maximum: table.ty.maximum,
                    });
                }
            }
            _ => {}
        }
    }

    Ok(WasmStructureSummary {
        memories,
        imported_host_functions,
        imported_function_count,
        total_import_count,
        exported_functions,
        start_function,
        tables,
    })
}

/// Formats a [`WasmStructureSummary`] as human-readable lines for
/// `--verbose` / `--wasm-info` output, including the memory configuration
/// and any limit warnings. Integer-only rendering; no fee math here.
#[must_use]
pub fn format_structure_summary(summary: &WasmStructureSummary) -> String {
    let mut out = String::new();
    out.push_str("WASM structure:\n");

    if summary.memories.is_empty() {
        out.push_str("  Memory: none declared\n");
    } else {
        for (i, memory) in summary.memories.iter().enumerate() {
            let max = memory
                .maximum_pages
                .map_or_else(|| "unbounded".to_string(), |m| m.to_string());
            let initial_bytes = memory.initial_pages.saturating_mul(WASM_PAGE_SIZE_BYTES);
            out.push_str(&format!(
                "  Memory[{i}]: initial={} pages ({} bytes), max={} pages\n",
                memory.initial_pages, initial_bytes, max
            ));
        }
    }

    out.push_str(&format!(
        "  Imports: {} function(s) total, {} from `{}` module\n",
        summary.imported_function_count,
        summary.imported_host_functions.len(),
        HOST_IMPORT_MODULE
    ));
    for imported in summary.imported_host_functions.iter().take(20) {
        out.push_str(&format!("    - {}.{}\n", imported.module, imported.name));
    }
    if summary.imported_host_functions.len() > 20 {
        out.push_str(&format!(
            "    ... and {} more\n",
            summary.imported_host_functions.len().saturating_sub(20)
        ));
    }

    out.push_str(&format!(
        "  Exports: {} function(s)\n",
        summary.exported_functions.len()
    ));
    for name in summary.exported_functions.iter().take(20) {
        out.push_str(&format!("    - {name}\n"));
    }
    if summary.exported_functions.len() > 20 {
        out.push_str(&format!(
            "    ... and {} more\n",
            summary.exported_functions.len().saturating_sub(20)
        ));
    }

    match summary.start_function {
        Some(func) => out.push_str(&format!("  Start function: {func}\n")),
        None => out.push_str("  Start function: none\n"),
    }

    if summary.tables.is_empty() {
        out.push_str("  Tables: none\n");
    } else {
        for (i, table) in summary.tables.iter().enumerate() {
            let max = table
                .maximum
                .map_or_else(|| "unbounded".to_string(), |m| m.to_string());
            out.push_str(&format!(
                "  Table[{i}]: initial={} entries, max={max} entries\n",
                table.initial
            ));
        }
    }

    for warning in summary.warnings() {
        out.push_str(&format!("  Warning: {warning}\n"));
    }

    out
}
