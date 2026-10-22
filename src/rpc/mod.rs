/// Thin JSON-RPC client for Stellar Soroban RPC endpoints.
pub mod client;
/// Circuit breaker for persistently unhealthy RPC endpoints (issue #505).
pub mod circuit_breaker;
pub mod config;
pub mod retry;
pub mod simulate;
/// WebSocket JSON-RPC client for real-time RPC subscriptions.
pub mod ws;
