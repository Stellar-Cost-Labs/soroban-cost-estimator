//! soroban-cost-estimator — Estimate Soroban contract costs & track network pricing changes.
//!
//! This crate provides a CLI tool that wraps Stellar's `simulateTransaction` RPC
//! and adds awareness of how the network's resource-pricing configuration changes
//! over time.
//!
//! # Library usage
//!
//! The binary is a thin wrapper around the modules exposed here, so any tool can
//! reuse the same building blocks:
//!
//! ```
//! use soroban_cost_estimator::rpc::client::resolve_endpoint;
//!
//! // Resolve a well-known network name to its Soroban RPC endpoint.
//! let endpoint = resolve_endpoint("testnet", None)?;
//! assert_eq!(endpoint, "https://soroban-testnet.stellar.org");
//! # Ok::<(), soroban_cost_estimator::error::AppError>(())
//! ```
//!
//! # Modules
//!
//! * [`rpc`] — JSON-RPC transport, retries, simulation, and WebSocket streaming.
//! * [`wasm`] — WASM loading, validation, and exported-function discovery.
//! * [`report`] — cost models, fee calculation, and output formatters.
//! * [`config_snapshot`] — resource-pricing snapshots and drift detection.
//! * [`cache`] — SQLite-backed estimate cache.
//! * [`config`] — `config.toml` parsing and per-network endpoint overrides.
//! * [`cli`] — clap command-line definitions shared with the binary.
//! * [`error`] — the crate-wide [`AppError`](error::AppError) type.
//! * [`paths`] — platform-specific data/cache directory resolution.
//! * [`interactive`] — the interactive estimate session.
//! * [`xdr_helper`] — XDR conversion helpers for the Soroban RPC protocol.

#![warn(missing_docs)]

/// SQLite-backed cache of previous cost estimates.
pub mod cache;
/// Command-line interface definitions (clap derive structs and enums).
pub mod cli;
/// User configuration file parsing and per-network RPC endpoint overrides.
pub mod config;
/// Network resource-pricing snapshots, storage, history, and drift detection.
pub mod config_snapshot;
/// Unified error type and result alias used across the crate.
pub mod error;
/// Interactive estimate session (prompt-driven contract inspection).
pub mod interactive;
/// Platform-specific directory resolution for data and cache files.
pub mod paths;
/// Cost models, fee calculation, and report formatting.
pub mod report;
/// JSON-RPC client, retries, simulation, and WebSocket support.
pub mod rpc;
/// WASM loading, validation, and function enumeration.
pub mod wasm;
/// XDR conversion helpers for Soroban RPC request and response types.
pub mod xdr_helper;
