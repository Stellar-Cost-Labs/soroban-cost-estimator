use serde_json::Value;

use crate::error::{AppError, AppResult};

/// Resolves a network name to its well-known Soroban RPC endpoint.
///
/// # Network calls
/// None — returns hardcoded well-known URLs. Custom URLs override network resolution.
pub fn resolve_endpoint(network: &str, custom_url: Option<&str>) -> AppResult<String> {
    if let Some(url) = custom_url {
        return Ok(url.to_string());
    }

    match network {
        "testnet" => Ok("https://soroban-testnet.stellar.org".to_string()),
        "mainnet" => Ok("https://soroban.stellar.org".to_string()),
        "futurenet" => Ok("https://rpc-futurenet.stellar.org".to_string()),
        other => Err(AppError::UnknownNetwork(other.to_string())),
    }
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

        let response = self.client.post(&self.url).json(&body).send().await?;

        let status = response.status();
        let response_body: Value = response.json().await?;
        if std::env::var("SCE_DEBUG_RPC").is_ok() {
            eprintln!(
                "[rpc-debug] {method}: {}",
                serde_json::to_string(&response_body).unwrap_or_default()
            );
        }

        // Check for a JSON-RPC error object
        if let Some(error) = response_body.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            return Err(AppError::Rpc {
                status: code,
                message,
            });
        }

        // Extract the `result` field
        let result = response_body.get("result").ok_or_else(|| AppError::Rpc {
            status: status.as_u16() as i64,
            message: "response missing 'result' field".to_string(),
        })?;

        serde_json::from_value(result.clone())
            .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
    }
}
