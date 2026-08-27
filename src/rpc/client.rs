use serde_json::Value;
use tracing::{debug, trace};

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

/// Resolves a network name to its WebSocket RPC endpoint (`wss://…/ws`).
///
/// Derives the WebSocket URL from the HTTP endpoint returned by
/// [`resolve_endpoint`] by swapping the scheme (`https` → `wss`, `http` →
/// `ws`) and appending the `/ws` path used by Stellar RPC for streaming
/// subscriptions. Custom URLs override network resolution and must already
/// be in WebSocket form.
///
/// # Network calls
/// None — pure string transformation of the resolved endpoint.
pub fn resolve_ws_endpoint(network: &str, custom_url: Option<&str>) -> AppResult<String> {
    if let Some(url) = custom_url {
        debug!(url, "using custom WebSocket RPC endpoint");
        return Ok(url.to_string());
    }

    let http_endpoint = resolve_endpoint(network, None)?;
    let ws_endpoint = match http_endpoint.strip_prefix("https://") {
        Some(host) => format!("wss://{host}/ws"),
        None => match http_endpoint.strip_prefix("http://") {
            Some(host) => format!("ws://{host}/ws"),
            None => return Err(AppError::UnknownNetwork(network.to_string())),
        },
    };
    debug!(network, ws_endpoint, "resolved WebSocket RPC endpoint");
    Ok(ws_endpoint)
}

/// A minimal JSON-RPC 2.0 client for Soroban RPC endpoints.
#[derive(Debug, Clone)]
pub struct RpcClient {
    url: String,
    client: reqwest::Client,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL.
    pub fn new(url: &str) -> Self {
        debug!(url, "creating RPC client");
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send a JSON-RPC request and deserialize the response.
    ///
    /// # Network calls
    /// Makes an HTTP POST to the configured RPC endpoint.
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
        let response = self.client.post(&self.url).json(&body).send().await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_ws_endpoint_derives_well_known_urls() {
        assert_eq!(
            resolve_ws_endpoint("testnet", None).unwrap(),
            "wss://soroban-testnet.stellar.org/ws"
        );
        assert_eq!(
            resolve_ws_endpoint("mainnet", None).unwrap(),
            "wss://soroban.stellar.org/ws"
        );
        assert_eq!(
            resolve_ws_endpoint("futurenet", None).unwrap(),
            "wss://rpc-futurenet.stellar.org/ws"
        );
    }

    #[test]
    fn test_resolve_ws_endpoint_unknown_network_errors() {
        assert!(matches!(
            resolve_ws_endpoint("nosuchnet", None),
            Err(AppError::UnknownNetwork(_))
        ));
    }

    #[test]
    fn test_resolve_ws_endpoint_custom_url_passthrough() {
        assert_eq!(
            resolve_ws_endpoint("testnet", Some("ws://localhost:8000/ws")).unwrap(),
            "ws://localhost:8000/ws"
        );
    }
}
