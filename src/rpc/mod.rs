//! JSON-RPC and WebSocket clients for Stellar Soroban RPC endpoints.
//!
//! This module provides:
//! - [`client`]: HTTP JSON-RPC client with custom header and timeout support.
//! - [`config`]: Batch fetching and XDR key encoding for on-chain `CONFIG_SETTING` entries.
//! - [`retry`]: Reusable exponential-backoff retry helper for transient network failures.
//! - [`simulate`]: `simulateTransaction` request builder and footprint/resource parser.
//! - [`ws`]: WebSocket client for real-time ledger and event subscriptions.
//!
//! # Examples
//!
//! ```rust
//! use soroban_cost_estimator::rpc::client::resolve_endpoint;
//! use soroban_cost_estimator::rpc::config::ConfigSettingId;
//!
//! let url = resolve_endpoint("testnet", None).expect("resolve testnet endpoint");
//! assert_eq!(url, "https://soroban-testnet.stellar.org");
//!
//! let key_b64 = ConfigSettingId::ContractComputeV0
//!     .ledger_key_b64()
//!     .expect("encode ledger key");
//! assert!(!key_b64.is_empty());
//! ```

/// Thin HTTP JSON-RPC client for Stellar Soroban RPC endpoints.
pub mod client;
/// Ledger entry queries for Soroban `CONFIG_SETTING` network parameters.
#[allow(missing_docs)]
pub mod config;
/// Generic exponential backoff retry helper for async RPC operations.
pub mod retry;
/// Transaction simulation (`simulateTransaction`) request and response handling.
pub mod simulate;
/// WebSocket JSON-RPC client for real-time RPC subscriptions.
#[allow(missing_docs)]
pub mod ws;
