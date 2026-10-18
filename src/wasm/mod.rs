//! WASM binary loading, structural validation, and Soroban contract spec inspection.
//!
//! The [`parser`] submodule decodes compiled `.wasm` contract files, verifies the WebAssembly
//! magic header and version, computes the SHA-256 executable hash, extracts `contractspecv0`
//! custom sections to discover typed function signatures, and detects debug symbols.
//!
//! # Examples
//!
//! ```rust
//! use soroban_cost_estimator::wasm::parser::is_debug_custom_section;
//!
//! assert!(is_debug_custom_section("name"));
//! assert!(is_debug_custom_section(".debug_info"));
//! assert!(!is_debug_custom_section("contractspecv0"));
//! ```

/// WASM file loading, validation, and function enumeration.
#[allow(missing_docs)]
pub mod parser;
