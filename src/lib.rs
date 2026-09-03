//! soroban-cost-estimator — Estimate Soroban contract costs & track network pricing changes.
//!
//! This crate provides a CLI tool that wraps Stellar's `simulateTransaction` RPC
//! and adds awareness of how the network's resource-pricing configuration changes
//! over time.

// Allow dead code — most modules are wired into the CLI now, but a few helpers
// remain scaffolding for future commands (e.g. cache::load_estimate,
// config_snapshot::store::list_snapshots).
#![allow(dead_code)]

pub mod cache;
pub mod cli;
pub mod config_snapshot;
pub mod error;
pub mod paths;
pub mod report;
pub mod rpc;
pub mod wasm;
pub mod xdr_helper;

/// Resolve the home directory, honoring an explicit `USERPROFILE` (Windows) or
/// `HOME` override before falling back to the platform default.
///
/// Unlike `dirs::home_dir()` alone — which on Windows uses the Known Folder API
/// and ignores environment variables — this lets callers (and integration
/// tests) redirect the data directory on any OS.
pub fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|p| !p.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(dirs::home_dir)
}
