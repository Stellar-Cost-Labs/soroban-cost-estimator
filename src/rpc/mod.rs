/// Thin JSON-RPC client for Stellar Soroban RPC endpoints.
pub mod client;
/// Typed accessors for the network configuration (`getLedgerEntries`).
pub mod config;
/// Retry policy and exponential backoff for transient RPC failures.
pub mod retry;
/// `simulateTransaction` request building and response parsing.
pub mod simulate;
/// WebSocket JSON-RPC client for real-time RPC subscriptions.
pub mod ws;
