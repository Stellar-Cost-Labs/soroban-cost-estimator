/// Thin JSON-RPC client for Stellar Soroban RPC endpoints.
pub mod client;
pub mod config;
pub mod retry;
pub mod simulate;
/// WebSocket JSON-RPC client for real-time RPC subscriptions.
pub mod ws;

pub use retry::{DEFAULT_MAX_RETRIES, with_retry, with_retry_backoff};
