//! Cost report generation, deterministic stroop fee calculation, and output formatting.
//!
//! This module contains:
//! - [`cost_report`]: Data structures for single-function and batch estimation reports,
//!   threshold checks, and optimization warnings.
//! - [`diff`]: Report-to-report comparison for detecting resource regressions in CI.
//! - [`fee_calc`]: Pure integer arithmetic functions that compute Soroban resource fees
//!   from simulation metrics and network `CONFIG_SETTING` snapshots.
//! - [`formatter`]: Pluggable [`formatter::ReportFormatter`] implementations for Table,
//!   JSON, CSV, Markdown, HTML, and Flamegraph outputs.
//!
//! # Examples
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
//! let fees = compute_fee_breakdown(50_000, 500_000, 1, 1, 512, 400, rates, DEFAULT_PRECISION);
//! assert_eq!(fees.total_stroops, fees.base_fee_stroops + 50_000);
//! ```

/// Cost report data structures, budget violation checks, and rendering helpers.
pub mod cost_report;
pub mod diff;
/// Deterministic integer-only Soroban fee calculation engine.
pub mod fee_calc;
pub mod formatter;
