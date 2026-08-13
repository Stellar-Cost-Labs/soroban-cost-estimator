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
pub mod report;
pub mod rpc;
pub mod wasm;
pub mod xdr_helper;
