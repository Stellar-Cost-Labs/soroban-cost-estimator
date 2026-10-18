//! `soroban-cost-estimator` — Estimate Soroban smart contract resource costs and track
//! Stellar network pricing configuration changes over time.
//!
//! This library powers the `soroban-cost-estimator` CLI and can be embedded directly into
//! Rust tools, CI pipelines, and analytics services to:
//!
//! - Parse and inspect compiled Soroban `.wasm` binaries ([`wasm`]).
//! - Simulate contract invocations and query network ledger entries via JSON-RPC ([`rpc`]).
//! - Calculate deterministic, integer-only stroop fees from simulation footprints and
//!   on-chain `CONFIG_SETTING` parameters ([`report`]).
//! - Capture, persist, diff, and audit historical network resource-pricing snapshots
//!   ([`config_snapshot`]).
//!
//! # Examples
//!
//! ## Computing Soroban Resource Fees Offline
//!
//! ```rust
//! use soroban_cost_estimator::report::fee_calc::{
//!     compute_fee_breakdown, FeeRates, DEFAULT_PRECISION,
//! };
//!
//! let rates = FeeRates {
//!     fee_per_10k_insns: 25,
//!     fee_per_read_entry: 6_250,
//!     fee_per_write_entry: 10_000,
//!     fee_per_read_1kb: 1_786,
//!     fee_per_1kb: 16_235,
//! };
//!
//! let breakdown = compute_fee_breakdown(
//!     85_000,     // total_resource_fee (stroops)
//!     2_500_000,  // cpu_insns
//!     3,          // read_entries
//!     1,          // write_entries
//!     2_048,      // read_bytes
//!     600,        // tx_size (bytes)
//!     rates,
//!     DEFAULT_PRECISION,
//! );
//!
//! assert_eq!(breakdown.base_fee_stroops, 100);
//! assert_eq!(breakdown.total_stroops, 85_100);
//! assert!(breakdown.refundable_stroops >= 0);
//! ```
//!
//! ## Diffing Network Configuration Snapshots
//!
//! ```rust
//! use soroban_cost_estimator::config_snapshot::diff::diff_snapshots;
//! use soroban_cost_estimator::config_snapshot::model::{ConfigSnapshot, ContractComputeV0};
//!
//! let old = ConfigSnapshot {
//!     network: "testnet".to_string(),
//!     timestamp: "2026-01-01T00:00:00Z".to_string(),
//!     ledger: 100,
//!     protocol_version: Some(21),
//!     contract_compute: Some(ContractComputeV0 {
//!         ledger_max_instructions: 100_000_000,
//!         tx_max_instructions: 100_000_000,
//!         fee_rate_per_instructions_increment: 25,
//!         tx_memory_limit: 41_943_040,
//!     }),
//!     contract_ledger_cost: None,
//!     contract_historical_data: None,
//!     contract_events: None,
//!     contract_bandwidth: None,
//!     state_archival: None,
//! };
//!
//! let mut new = old.clone();
//! new.ledger = 200;
//! if let Some(ref mut compute) = new.contract_compute {
//!     compute.fee_rate_per_instructions_increment = 50;
//! }
//!
//! let diff = diff_snapshots(&old, &new);
//! assert!(diff.has_pricing_changes());
//! ```

#![warn(missing_docs)]
#![allow(rustdoc::private_intra_doc_links)]

pub mod cache;
/// Command-line interface definitions and argument parsers.
#[allow(missing_docs)]
pub mod cli;
pub mod config_snapshot;
/// Structured error types and result aliases used across the crate.
#[allow(missing_docs)]
pub mod error;
/// Interactive terminal prompt utilities for selecting contracts and arguments.
pub mod interactive;
/// Cross-platform application directory and storage path resolution.
pub mod paths;
pub mod report;
pub mod rpc;
pub mod wasm;
/// Helper utilities for constructing and decoding Stellar XDR types.
pub mod xdr_helper;
