//! Network resource-pricing configuration snapshot management, persistence, and diffing.
//!
//! Soroban resource fees depend on six on-chain `CONFIG_SETTING` ledger entries. This module
//! provides data models ([`model`]), disk persistence ([`store`]), field-by-field diffing
//! ([`diff`]), and historical timeline reconstruction ([`history`]).
//!
//! # Examples
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
//! let mut new = old.clone();
//! if let Some(ref mut compute) = new.contract_compute {
//!     compute.fee_rate_per_instructions_increment = 50;
//! }
//!
//! let diff = diff_snapshots(&old, &new);
//! assert!(diff.has_pricing_changes());
//! ```

/// Snapshot comparison and CI drift exit-code resolution.
#[allow(missing_docs)]
pub mod diff;
/// Chronological change log construction across stored snapshots.
#[allow(missing_docs)]
pub mod history;
/// Data structures representing Soroban on-chain `CONFIG_SETTING` parameters.
#[allow(missing_docs)]
pub mod model;
/// Snapshot file storage, pruning, validation, and bundle export/import.
#[allow(missing_docs)]
pub mod store;
