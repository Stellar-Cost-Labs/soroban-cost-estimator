use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{Quota, RateLimiter};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, trace, warn};

use crate::error::{AppError, AppResult};
use crate::rpc::retry::with_retry;

/// Default per-request HTTP timeout applied to every RPC call. Matches the
/// CLI's `--timeout` default (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Key identifying a deduplicable JSON-RPC request: `(method, serialized params)`.
type RequestKey = (String, String);

/// Private, shared deduplication state for a `RpcClient`.
///
/// Deduplication collapses identical JSON-RPC requests — the same method with
/// the same params — into a single network call. This matters for batch
/// operations such as `estimate-all`, where several functions share the same
/// WASM upload path and would otherwise transmit the identical upload request
/// over and over.
#[derive(Debug, Default)]
struct DedupState {
    /// Results of identical requests that already completed successfully,
    /// keyed by request. A cache hit skips the network entirely.
    completed: HashMap<RequestKey, Value>,
    /// Per-request serialization gates. The first caller for a key (the
    /// "leader") performs the request; concurrent identical callers wait on
    /// the gate, then read the cached result. Followers of a *failed* leader
    /// observe no cached result and simply become the next leader, so a
    /// retry only costs the request itself.
    in_flight: HashMap<RequestKey, Arc<Mutex<()>>>,
}

/// A minimal JSON-RPC 2.0 client for Soroban RPC endpoints.
///
/// Identical in-flight or completed requests (same method + params) are
/// deduplicated so a batch operation sends each distinct request only once.
///
/// An optional fixed-rate limiter (requests per second) can be attached to
/// cap the rate of *outbound* HTTP calls, so batch operations such as
/// `estimate-all` do not hammer the RPC endpoint and trip its rate limits.
/// Deduplicated requests that never reach the network are not throttled.
#[derive(Debug)]
pub struct RpcClient {
    url: String,
    fallback_url: Option<String>,
    client: reqwest::Client,
    dedup: Arc<Mutex<DedupState>>,
    /// Fixed-rate limiter shared by every network call, when enabled.
    limiter: Option<Arc<governor::DefaultDirectRateLimiter>>,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL, without rate
    /// limiting and with the default request timeout.
    pub fn new(url: &str) -> Self {
        Self::with_rate_limit(url, None)
    }

    /// Create a new RPC client pointing at the given URL, optionally capping
    /// outbound requests to `rps` requests per second.
    ///
    /// The limiter spaces consecutive outbound calls at least `1/rps` seconds
    /// apart (a fixed-rate limiter with a burst of 1). `None` or `Some(0)`
    /// disables rate limiting entirely. Values larger than `u32::MAX` are
    /// clamped.
    ///
    /// The underlying `reqwest::Client` is configured with connection pooling
    /// and TCP keep-alive so that HTTP connections are reused across multiple
    /// RPC calls within a single run, reducing handshake overhead.
    pub fn with_rate_limit(url: &str, rps: Option<u64>) -> Self {
        Self::with_options(url, rps, DEFAULT_TIMEOUT)
    }

    /// Create a new RPC client pointing at the given URL, optionally capping
    /// outbound requests to `rps` requests per second and bounding each HTTP
    /// request with `timeout`.
    ///
    /// The limiter spaces consecutive outbound calls at least `1/rps` seconds
    /// apart (a fixed-rate limiter with a burst of 1). `None` or `Some(0)`
    /// disables rate limiting entirely. Values larger than `u32::MAX` are
    /// clamped. `timeout` applies to the whole request (connect through
    /// response body) and is passed straight to reqwest.
    pub fn with_options(url: &str, rps: Option<u64>, timeout: Duration) -> Self {
        Self::with_fallback(url, None, rps, timeout)
    }

    /// Create a new RPC client pointing at the given URL, with an optional
    /// secondary URL used for failover, optionally capping outbound requests
    /// to `rps` requests per second and bounding each HTTP request with
    /// `timeout`.
    ///
    /// When a request to the primary endpoint fails with a network-level
    /// error (connection refused, timeout, DNS failure, etc.) and a fallback
    /// URL is configured, the request is retried against the fallback before
    /// the error is propagated. RPC-level errors (e.g. bad method, invalid
    /// params) are not retried against the fallback — they would fail there
    /// too.
    ///
    /// The limiter and timeout behave exactly as in [`Self::with_options`].
    pub fn with_fallback(
        url: &str,
        fallback_url: Option<&str>,
        rps: Option<u64>,
        timeout: Duration,
    ) -> Self {
        debug!(url, ?fallback_url, rps, ?timeout, "creating RPC client");
        Self {
            url: url.to_string(),
            fallback_url: fallback_url.map(String::from),
            // `ClientBuilder::build` only fails on invalid configuration (a
            // default builder cannot), so fall back to a plain client to keep
            // construction infallible.
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            dedup: Arc::new(Mutex::new(DedupState::default())),
            limiter: rps.and_then(build_rate_limiter),
        }
    }

    /// Send a JSON-RPC request and deserialize the response.
    ///
    /// Requests are deduplicated by `(method, params)`: a request identical to
    /// one already completed returns the cached result without sending
    /// anything, and concurrent identical requests are collapsed into a single
    /// network call (single-flight). A failed leader does not poison its
    /// followers — the next waiter retries the request itself.
    ///
    /// # Network calls
    /// At most one HTTP POST for any distinct `(method, params)` pair; zero
    /// for a cache hit.
    pub async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> AppResult<T> {
        let key = (method.to_string(), params.to_string());

        loop {
            // Fast path: an identical request already completed successfully.
            if let Some(cached) = self.cached_result(&key).await {
                trace!(method, "deduplicated against completed request");
                return deserialize_result::<T>(cached);
            }

            // Claim (or reuse) the serialization gate for this key.
            let gate = {
                let mut state = self.dedup.lock().await;
                Arc::clone(
                    state
                        .in_flight
                        .entry(key.clone())
                        .or_insert_with(|| Arc::new(Mutex::new(()))),
                )
            };

            if let Ok(_guard) = gate.try_lock() {
                // Leader: perform the network request and publish the result
                // for any waiters before releasing the gate.
                let result = self.perform_call(method, params).await;
                let mut state = self.dedup.lock().await;
                if let Ok(value) = &result {
                    state.completed.insert(key.clone(), value.clone());
                }
                state.in_flight.remove(&key);
                return result.and_then(deserialize_result::<T>);
            }

            // Follower: wait for the leader to finish, then loop back to the
            // fast path. If the leader failed, nothing was cached and this
            // iteration becomes the new leader (a retry).
            let _follower_guard = gate.lock().await;
        }
    }

    /// Returns the cached result for `key`, if a prior identical request
    /// completed successfully.
    async fn cached_result(&self, key: &RequestKey) -> Option<Value> {
        let state = self.dedup.lock().await;
        state.completed.get(key).cloned()
    }

    /// Performs the actual HTTP POST and extracts the raw `result` value.
    ///
    /// Tries the primary endpoint first. If the primary fails with a
    /// network-level error (connection refused, timeout, DNS failure, etc.)
    /// and a fallback URL is configured, retries against the fallback.
    ///
    /// RPC-level errors (e.g. bad method, invalid params) are **not** retried
    /// against the fallback — they would fail there too.
    ///
    /// # Network calls
    /// Makes an HTTP POST to the configured RPC endpoint (and optionally the
    /// fallback).
    async fn perform_call(&self, method: &str, params: Value) -> AppResult<Value> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        trace!(method, "sending RPC request");
        match self.post_and_parse(method, &body, &self.url).await {
            Ok(result) => Ok(result),
            Err(e) if Self::is_network_error(&e) => {
                if let Some(ref fallback) = self.fallback_url {
                    warn!(
                        method,
                        primary = %self.url,
                        fallback = %fallback,
                        error = %e,
                        "primary RPC endpoint failed — trying fallback"
                    );
                    self.post_and_parse(method, &body, fallback).await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Check whether an error is a network-level failure (as opposed to an
    /// RPC-level error returned inside a successful HTTP response).
    fn is_network_error(error: &AppError) -> bool {
        match error {
            AppError::Http(e) => {
                // reqwest errors that indicate connectivity problems — these
                // are the cases where a fallback endpoint might succeed.
                e.is_connect() || e.is_timeout() || e.is_request()
            }
            _ => false,
        }
    }

    /// POST `body` to `url` (with retries), parse the JSON-RPC response, and
    /// extract the raw `result` value.
    async fn post_and_parse(&self, method: &str, body: &Value, url: &str) -> AppResult<Value> {
        let client = self.client.clone();
        let url = url.to_string();
        let request_body = body.clone();
        let limiter = self.limiter.clone();

        let response = with_retry(|| {
            let client = client.clone();
            let url = url.clone();
            let request_body = request_body.clone();
            let limiter = limiter.clone();

            async move {
                // Every outbound attempt (including retries) consumes a
                // token, so the wire rate never exceeds the configured
                // requests-per-second cap.
                if let Some(limiter) = &limiter {
                    limiter.until_ready().await;
                }
                client
                    .post(&url)
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(AppError::from)
            }
        })
        .await?;
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
        Ok(result.clone())
    }
}

/// Builds an optional fixed-rate limiter for `rps` requests per second.
///
/// Returns `None` when `rps` is zero (no limit) or when a valid period
/// cannot be derived (a defensive case — any `rps >= 1` yields a valid
/// period). The limiter uses a burst of 1, so consecutive outbound calls
/// are spaced exactly `1/rps` seconds apart.
fn build_rate_limiter(rps: u64) -> Option<Arc<governor::DefaultDirectRateLimiter>> {
    if rps == 0 {
        return None;
    }
    let rps = NonZeroU32::new(u32::try_from(rps).unwrap_or(u32::MAX))?;
    let period = std::time::Duration::from_secs_f64(1.0 / f64::from(rps.get()));
    let quota = Quota::with_period(period)?.allow_burst(NonZeroU32::new(1)?);
    Some(Arc::new(RateLimiter::direct(quota)))
}

/// Deserializes a raw JSON-RPC `result` value into the caller's type.
fn deserialize_result<T: serde::de::DeserializeOwned>(value: Value) -> AppResult<T> {
    serde_json::from_value(value)
        .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use crate::error::{AppError, AppResult};

    use super::{RpcClient, resolve_ws_endpoint};

    /// Spawns a tiny HTTP server that answers JSON-RPC `simulateTransaction`
    /// calls, counting how many were received. The first `fail_times` calls
    /// return a JSON-RPC error body instead of a result.
    async fn spawn_json_rpc_stub(fail_times: u32) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let addr = listener.local_addr().expect("no local address");
        let counter = Arc::new(AtomicUsize::new(0));
        let server_counter = Arc::clone(&counter);

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let counter = Arc::clone(&server_counter);
                tokio::spawn(async move {
                    let _ = handle_conn(stream, counter, fail_times).await;
                });
            }
        });

        (format!("http://{addr}"), counter)
    }

    async fn handle_conn(
        mut stream: TcpStream,
        counter: Arc<AtomicUsize>,
        fail_times: u32,
    ) -> std::io::Result<()> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let header_end = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("request headers must end")
            + 4;
        let content_length: usize = String::from_utf8_lossy(&buf[..header_end])
            .lines()
            .find_map(|line| {
                line.trim()
                    .to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse().ok())
            })
            .unwrap_or(0);
        while buf.len() < header_end + content_length {
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
        }

        let call_no = counter.fetch_add(1, Ordering::SeqCst);
        let body = if (call_no as u32) < fail_times {
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"stubbed failure"}}"#
        } else {
            r#"{"jsonrpc":"2.0","id":1,"result":{"pong":true}}"#
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await?;
        stream.flush().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_dedup_sequential_identical_requests() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::new(&url);
        let params = serde_json::json!({"k": "v"});

        let _: Value = client
            .call("test.method", params.clone())
            .await
            .expect("first call");
        let _: Value = client
            .call("test.method", params)
            .await
            .expect("deduped call");

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "identical requests must hit the network once"
        );
    }

    #[tokio::test]
    async fn test_dedup_distinct_params_not_deduplicated() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::new(&url);

        let _: Value = client
            .call("test.method", serde_json::json!({"k": 1}))
            .await
            .expect("first distinct call");
        let _: Value = client
            .call("test.method", serde_json::json!({"k": 2}))
            .await
            .expect("second distinct call");

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "distinct requests must both hit the network"
        );
    }

    #[tokio::test]
    async fn test_dedup_concurrent_identical_requests() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = Arc::new(RpcClient::new(&url));

        let mut handles = Vec::new();
        for _ in 0..5 {
            let client = Arc::clone(&client);
            handles.push(tokio::spawn(async move {
                let _: Value = client
                    .call("test.method", serde_json::json!({"k": "v"}))
                    .await
                    .expect("deduped concurrent call");
            }));
        }
        for handle in handles {
            handle.await.expect("task should not panic");
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "concurrent identical requests must hit the network once"
        );
    }

    /// A failed leader reports the error to its own caller but must not poison
    /// waiters: a follower observes no cached result and retries the request
    /// itself, so the follower succeeds at the cost of one extra network
    /// attempt. Exactly one of the two identical callers ends up successful.
    #[tokio::test]
    async fn test_dedup_failed_leader_followers_retry() {
        let (url, counter) = spawn_json_rpc_stub(1).await;
        let client = Arc::new(RpcClient::new(&url));

        let params = serde_json::json!({"k": "v"});
        let task_a = {
            let client = Arc::clone(&client);
            let params = params.clone();
            tokio::spawn(async move { client.call::<Value>("test.method", params).await })
        };
        let task_b = {
            let client = Arc::clone(&client);
            let params = params.clone();
            tokio::spawn(async move { client.call::<Value>("test.method", params).await })
        };

        let (ra, rb) = (task_a.await.expect("task"), task_b.await.expect("task"));
        assert_eq!(
            usize::from(ra.is_ok()) + usize::from(rb.is_ok()),
            1,
            "exactly one caller succeeds; the other sees the leader's error"
        );
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "follower retried once after the leader's failure"
        );
    }

    /// With a 20 req/s cap (50 ms spacing), two back-to-back *distinct*
    /// requests must be spaced ~50 ms apart — the limiter must actually
    /// throttle the wire.
    #[tokio::test]
    async fn test_rate_limiter_spaces_outbound_requests() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_rate_limit(&url, Some(20));

        let start = std::time::Instant::now();
        let _: Value = client
            .call("test.method", serde_json::json!({"k": 1}))
            .await
            .expect("first call");
        let _: Value = client
            .call("test.method", serde_json::json!({"k": 2}))
            .await
            .expect("second call");
        let elapsed = start.elapsed();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "both distinct requests must hit the network"
        );
        assert!(
            elapsed.as_millis() >= 45,
            "20 req/s must space requests ~50 ms apart; elapsed: {elapsed:?}"
        );
    }

    /// Rate limiting must only throttle requests that actually reach the
    /// network: an identical request served from the dedup cache skips the
    /// limiter entirely and returns immediately.
    #[tokio::test]
    async fn test_rate_limiter_preserves_dedup() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_rate_limit(&url, Some(20));
        let params = serde_json::json!({"k": "v"});

        let start = std::time::Instant::now();
        let _: Value = client
            .call("test.method", params.clone())
            .await
            .expect("first call");
        let _: Value = client
            .call("test.method", params)
            .await
            .expect("deduped call");

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "identical requests must hit the network once"
        );
        assert!(
            start.elapsed().as_millis() < 45,
            "a deduped request must not wait on the rate limiter"
        );
    }

    /// The default constructor must not throttle anything; `Some(0)` must be
    /// treated as "no limit" rather than a zero-period limiter.
    #[tokio::test]
    async fn test_no_rate_limit_when_disabled() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_rate_limit(&url, Some(0));

        let start = std::time::Instant::now();
        let _: Value = client
            .call("test.method", serde_json::json!({"k": 1}))
            .await
            .expect("first call");
        let _: Value = client
            .call("test.method", serde_json::json!({"k": 2}))
            .await
            .expect("second call");

        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert!(
            start.elapsed().as_millis() < 45,
            "disabled rate limiting must not delay requests"
        );
    }

    /// Spawns an HTTP server that accepts connections but never responds, so
    /// a client with a short timeout observes a request-timeout error instead
    /// of hanging forever.
    async fn spawn_hanging_stub() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let addr = listener.local_addr().expect("no local address");

        tokio::spawn(async move {
            while let Ok((_stream, _)) = listener.accept().await {
                // Never respond — force the client's request timeout to fire.
                std::future::pending::<()>().await;
            }
        });

        format!("http://{addr}")
    }

    /// A per-request timeout configured via `with_options` must actually
    /// bound the request: against a server that accepts but never answers,
    /// the retry loop gives up and surfaces an HTTP error rather than
    /// waiting forever.
    #[tokio::test]
    async fn test_request_timeout_applies() {
        let url = spawn_hanging_stub().await;
        let client = RpcClient::with_options(&url, None, Duration::from_millis(100));

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(
            result.is_err(),
            "a hanging server must eventually produce a timeout error"
        );
    }

    /// Reserves a port and immediately closes it, so connecting to it yields
    /// a network-level connection-refused error (rather than a timeout).
    async fn refused_port_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let addr = listener.local_addr().expect("no local address");
        drop(listener);
        format!("http://{addr}")
    }

    /// When the primary endpoint is unreachable (connection refused) and a
    /// fallback URL is configured, the request must fail over to the fallback
    /// instead of propagating the network error.
    #[tokio::test]
    async fn test_failover_uses_fallback_when_primary_unreachable() {
        let dead_url = refused_port_url().await;
        let (fallback_url, fallback_counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_fallback(
            &dead_url,
            Some(&fallback_url),
            None,
            Duration::from_secs(30),
        );

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(
            result.is_ok(),
            "request should fail over to the fallback endpoint"
        );
        assert_eq!(
            fallback_counter.load(Ordering::SeqCst),
            1,
            "fallback endpoint should have served exactly one request"
        );
    }

    /// RPC-level errors (a JSON-RPC error body inside a successful HTTP
    /// response) mean the primary is reachable, so they must **not** trigger
    /// a failover attempt against the fallback.
    #[tokio::test]
    async fn test_no_failover_on_rpc_error() {
        let (primary_url, _) = spawn_json_rpc_stub(1).await;
        let (fallback_url, fallback_counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_fallback(
            &primary_url,
            Some(&fallback_url),
            None,
            Duration::from_secs(30),
        );

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(result.is_err(), "RPC-level errors must be propagated");
        assert_eq!(
            fallback_counter.load(Ordering::SeqCst),
            0,
            "fallback must not be contacted for RPC-level errors"
        );
    }

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
