use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
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

/// A minimal JSON-RPC 2.0 client for Soroban RPC endpoints.
#[derive(Debug, Clone)]
pub struct RpcClient {
    url: String,
    client: reqwest::Client,
    headers: HeaderMap,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL.
    pub fn new(url: &str) -> Self {
        debug!(url, "creating RPC client");
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers: HeaderMap::new(),
        }
    }

    /// Create a new RPC client with custom HTTP headers applied to every request.
    ///
    /// `extra_headers` is a slice of `"Key: Value"` strings. Malformed entries
    /// are silently skipped with a warning so that a single bad header does not
    /// prevent the entire request from being sent.
    pub fn with_headers(url: &str, extra_headers: &[String]) -> Self {
        debug!(url, header_count = extra_headers.len(), "creating RPC client with custom headers");
        let mut headers = HeaderMap::new();
        for raw in extra_headers {
            match parse_header(raw) {
                Ok((name, value)) => {
                    debug!(name = %name, "adding custom header");
                    headers.insert(name, value);
                }
                Err(e) => {
                    eprintln!("Warning: skipping malformed header '{raw}': {e}");
                }
            }
        }
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers,
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

        debug!(method, params = %params, "sending RPC request");
        let response = self.client
            .post(&self.url)
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_body: Value = response.json().await?;

        if let Some(error) = response_body.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            debug!(method, code, message, "RPC error response");
            return Err(AppError::Rpc {
                status: code,
                message,
            });
        }

        let result = response_body.get("result").ok_or_else(|| AppError::Rpc {
            status: status.as_u16() as i64,
            message: "response missing 'result' field".to_string(),
        })?;

        debug!(
            method,
            status = %status,
            result = %serde_json::to_string(result).unwrap_or_default(),
            "RPC response received"
        );
        trace!(method, "RPC call succeeded");
        serde_json::from_value(result.clone())
            .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
    }
}

/// Parse a `"Key: Value"` string into an HTTP header name and value.
///
/// Returns an error if the format is invalid (missing colon, empty key,
/// or non-ASCII characters in the header name).
fn parse_header(raw: &str) -> Result<(HeaderName, HeaderValue), String> {
    let colon_pos = raw.find(':').ok_or("missing ':' separator")?;
    let name_str = raw[..colon_pos].trim();
    let value_str = raw[colon_pos + 1..].trim();

    if name_str.is_empty() {
        return Err("empty header name".to_string());
    }

    let name = HeaderName::from_bytes(name_str.as_bytes())
        .map_err(|e| format!("invalid header name: {e}"))?;
    let value = HeaderValue::from_str(value_str)
        .map_err(|e| format!("invalid header value: {e}"))?;

    Ok((name, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_valid() {
        let (name, value) = parse_header("X-API-Key: secret123").unwrap();
        assert_eq!(name.as_str(), "x-api-key");
        assert_eq!(value.to_str().unwrap(), "secret123");
    }

    #[test]
    fn test_parse_header_with_spaces() {
        let (name, value) = parse_header(" Authorization : Bearer tok ").unwrap();
        assert_eq!(name.as_str(), "authorization");
        assert_eq!(value.to_str().unwrap(), "Bearer tok");
    }

    #[test]
    fn test_parse_header_missing_colon() {
        assert!(parse_header("NoColon").is_err());
    }

    #[test]
    fn test_parse_header_empty_name() {
        assert!(parse_header(": value").is_err());
    }

    #[test]
    fn test_parse_header_empty_value() {
        let (name, value) = parse_header("X-Custom:").unwrap();
        assert_eq!(name.as_str(), "x-custom");
        assert_eq!(value.to_str().unwrap(), "");
    }

    #[test]
    fn test_parse_header_value_with_colons() {
        let (name, value) = parse_header("X-Auth: token:with:colons").unwrap();
        assert_eq!(name.as_str(), "x-auth");
        assert_eq!(value.to_str().unwrap(), "token:with:colons");
    }

    #[test]
    fn test_with_headers_empty() {
        let client = RpcClient::with_headers("http://localhost", &[]);
        assert!(client.headers.is_empty());
    }

    #[test]
    fn test_with_headers_stores_parsed() {
        let client = RpcClient::with_headers(
            "http://localhost",
            &[
                "X-API-Key: secret".to_string(),
                "Authorization: Bearer tok".to_string(),
            ],
        );
        assert_eq!(client.headers.len(), 2);
        assert_eq!(
            client.headers.get("x-api-key").unwrap().to_str().unwrap(),
            "secret"
        );
        assert_eq!(
            client.headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer tok"
        );
    }

    #[test]
    fn test_with_headers_skips_malformed() {
        let client = RpcClient::with_headers(
            "http://localhost",
            &[
                "Good: ok".to_string(),
                "NoColonHere".to_string(),
                "Also-Bad:".to_string(),
            ],
        );
        // Only the valid header should be stored.
        assert_eq!(client.headers.len(), 1);
        assert!(client.headers.contains_key("good"));
    }

    #[test]
    fn test_rpc_client_new_has_no_custom_headers() {
        let client = RpcClient::new("http://localhost");
        assert!(client.headers.is_empty());
    }
}
