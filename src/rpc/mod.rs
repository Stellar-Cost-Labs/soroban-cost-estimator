/// Thin JSON-RPC client for Stellar Soroban RPC endpoints.
pub mod client;
/// Circuit breaker for persistently unhealthy RPC endpoints (issue #505).
pub mod circuit_breaker;
/// Typed accessors for the network configuration (`getLedgerEntries`).
pub mod config;
/// Retry policy and exponential backoff for transient RPC failures.
pub mod retry;
/// `simulateTransaction` request building and response parsing.
pub mod simulate;
/// WebSocket JSON-RPC client for real-time RPC subscriptions.
pub mod ws;
