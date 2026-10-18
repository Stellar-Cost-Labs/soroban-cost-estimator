//! WASM inspection CLI subcommands and argument definitions.

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Subcommand, Debug)]
pub enum WasmAction {
    /// Print size, SHA-256, exported signatures, embedded metadata, and the
    /// section size summary for a WASM file. Fully offline: no RPC calls.
    Info {
        /// Path to the compiled Soroban contract `.wasm` file.
        wasm: PathBuf,

        /// Output the full parsed spec/metadata as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Arguments for the top-level `wasm-info` subcommand.
#[derive(Args, Clone, Debug)]
pub struct WasmInfoArgs {
    /// Path to the compiled Soroban contract `.wasm` file.
    #[arg(long, short)]
    pub wasm: String,

    /// Output the full parsed spec/metadata as JSON.
    #[arg(long)]
    pub json: bool,
}
