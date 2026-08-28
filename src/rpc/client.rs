use serde_json::Value;
use tracing::{debug, trace, warn};

use crate::error::{AppError, AppResult};

/// Resolves a network name to its well-known Soroban RPC endpoint.
///
/// # Network calls
/// None — returns hardcoded well-known URLs. Custom URLs override network resolution.
pub fn resolve_endpoint(network: &str, custom_url: Option<&str>) -> AppResult<String> {
    if let Some(url) = custom_url {
        debug!(url, "using custom RPC endpoint");
        return Ok(url.to_string());
    }

    let endpoint = match network {
        "testnet" => Ok("https://soroban-testnet.stellar.org".to_string()),
        "mainnet" => Ok("https://soroban.stellar.org".to_string()),
        "futurenet" => Ok("https://rpc-futurenet.stellar.org".to_string()),
        other => Err(AppError::UnknownNetwork(other.to_string())),
    };

    if let Ok(ref url) = endpoint {
        debug!(network, url, "resolved RPC endpoint");
    }
    endpoint
}

/// A minimal JSON-RPC 2.0 client for Soroban RPC endpoints.
#[derive(Debug, Clone)]
pub struct RpcClient {
    url: String,
    fallback_url: Option<String>,
    client: reqwest::Client,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL.
    pub fn new(url: &str) -> Self {
        debug!(url, "creating RPC client");
        Self {
            url: url.to_string(),
            fallback_url: None,
            client: reqwest::Client::new(),
        }
    }

    /// Create a new RPC client with a fallback URL for failover.
    pub fn with_fallback(url: &str, fallback_url: Option<&str>) -> Self {
        debug!(url, ?fallback_url, "creating RPC client with fallback");
        Self {
            url: url.to_string(),
            fallback_url: fallback_url.map(String::from),
            client: reqwest::Client::new(),
        }
    }

    /// Send a JSON-RPC request and deserialize the response.
    ///
    /// Tries the primary endpoint first. If the primary fails with a
    /// network-level error (connection refused, timeout, DNS failure, etc.)
    /// and a fallback URL is configured, retries against the fallback.
    ///
    /// RPC-level errors (e.g. bad method, invalid params) are **not** retried
    /// against the fallback — they would fail there too.
    ///
    /// # Network calls
    /// Makes an HTTP POST to the configured RPC endpoint (and optionally the fallback).
    pub async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> AppResult<T> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        trace!(method, "sending RPC request");
        match self.post_and_deserialize(method, &body, &self.url).await {
            Ok(result) => Ok(result),
            Err(e) if self.is_network_error(&e) => {
                if let Some(ref fallback) = self.fallback_url {
                    warn!(
                        method,
                        primary = %self.url,
                        fallback = %fallback,
                        error = %e,
                        "primary RPC endpoint failed — trying fallback"
                    );
                    self.post_and_deserialize(method, &body, fallback).await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Check whether an error is a network-level failure (as opposed to an
    /// RPC-level error returned inside a successful HTTP response).
    fn is_network_error(&self, error: &AppError) -> bool {
        match error {
            AppError::Http(e) => {
                // reqwest errors that indicate connectivity problems — these
                // are the cases where a fallback endpoint might succeed.
                e.is_connect() || e.is_timeout() || e.is_request()
            }
            _ => false,
        }
    }

    /// POST to the given URL, parse the JSON-RPC response, and deserialize
    /// the `result` field into `T`.
    async fn post_and_deserialize<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        body: &Value,
        url: &str,
    ) -> AppResult<T> {
        let response = self.client.post(url).json(body).send().await?;

        let status = response.status();
        let response_body: Value = response.json().await?;
        if std::env::var("SCE_DEBUG_RPC").is_ok() {
            debug!(
                method,
                response = %serde_json::to_string(&response_body).unwrap_or_default(),
                "RPC response"
            );
        }

        if let Some(error) = response_body.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            debug!(method, code, message, "RPC error");
            return Err(AppError::Rpc {
                status: code,
                message,
            });
        }

        let result = response_body.get("result").ok_or_else(|| AppError::Rpc {
            status: status.as_u16() as i64,
            message: "response missing 'result' field".to_string(),
        })?;

        trace!(method, "RPC call succeeded");
        serde_json::from_value(result.clone())
            .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
    }
}
