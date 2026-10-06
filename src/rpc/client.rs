use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use governor::{Quota, RateLimiter};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};
use crate::rpc::retry::{DEFAULT_MAX_RETRIES, with_retry};

/// Default per-request HTTP timeout applied to every RPC call. Matches the
/// CLI's `--timeout` default (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default TCP connection establishment timeout applied to every RPC call.
/// Matches the CLI's `--connect-timeout` default (5 seconds).
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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

/// A single-flight handle for one in-flight request.
///
/// The first caller for a request key (the "leader") owns a `SharedFuture`
/// and performs the network call. Concurrent identical callers (the
/// "followers") await the *same* handle instead of issuing their own request,
/// so N identical calls in flight share one underlying future and one HTTP
/// POST.
///
/// A leader that succeeds publishes its raw JSON-RPC `result`, which every
/// follower reads without touching the network. A leader that fails publishes
/// nothing: followers observe an empty outcome and become the next leader, so
/// one caller's failure never poisons the rest — they simply retry.
#[derive(Debug, Clone, Default)]
struct SharedFuture {
    inner: Arc<SharedFutureInner>,
}

/// Shared state behind a [`SharedFuture`].
#[derive(Debug, Default)]
struct SharedFutureInner {
    /// Successful outcome published by the leader, if it completed. Written
    /// once by the leader and read by every follower.
    outcome: Mutex<Option<Value>>,
    /// Set once the leader has finished — successfully or not.
    finished: AtomicBool,
    /// Wakes followers the moment `finished` flips to `true`.
    done: Notify,
}

impl SharedFuture {
    /// Publishes the leader's successful result for its followers.
    async fn publish(&self, value: Value) {
        *self.inner.outcome.lock().await = Some(value);
    }

    /// Returns the leader's published result, if it succeeded.
    async fn outcome(&self) -> Option<Value> {
        self.inner.outcome.lock().await.clone()
    }

    /// Waits until the leader has finished (success or failure).
    ///
    /// Interest is registered *before* `finished` is checked, so a leader that
    /// completes in the interim cannot produce a lost wakeup.
    async fn wait(&self) {
        let notified = self.inner.done.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.inner.finished.load(Ordering::Acquire) {
            notified.await;
        }
    }

    /// Marks this shared future complete and wakes every follower.
    fn finish(&self) {
        self.inner.finished.store(true, Ordering::Release);
        self.inner.done.notify_waiters();
    }
}

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
    /// keyed by request. A cache hit skips the network entirely and also lets
    /// a request arriving *after* its twin finished share the result.
    completed: HashMap<RequestKey, Value>,
    /// One single-flight handle per key currently in flight. Concurrent
    /// identical callers attach to the existing handle rather than opening a
    /// second request.
    in_flight: HashMap<RequestKey, SharedFuture>,
}

/// Response envelope for the `getHealth` JSON-RPC method.
///
/// Stellar RPC returns a `status` of `healthy`, `degraded`, or `unhealthy`
/// plus ledger-window information; the health check only needs `status`.
/// Extra fields in the response are ignored by serde.
#[derive(Debug, Deserialize)]
struct HealthResponse {
    /// Node health status: `healthy`, `degraded`, or `unhealthy`.
    status: String,
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
    /// Maximum number of retries on transient (HTTP) failures, with
    /// exponential backoff.
    max_retries: usize,
    /// TCP connection establishment timeout. Distinct from the total request
    /// timeout: it bounds only the initial connect (TCP/TLS handshake), so a
    /// dead or unreachable host fails fast instead of hanging for the full
    /// request timeout.
    #[cfg_attr(not(test), allow(dead_code))]
    connect_timeout: Duration,
    /// Custom HTTP headers attached to every outbound request.
    pub headers: HeaderMap,
    /// Whether to print verbose RPC request/response diagnostics to stderr.
    pub verbose: bool,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL, without rate
    /// limiting and with the default request timeout.
    pub fn new(url: &str) -> Self {
        Self::with_rate_limit(url, None, false)
    }

    /// Create a new RPC client pointing at the given URL, optionally capping
    /// outbound requests to `rps` requests per second. Retries use the
    /// [`DEFAULT_MAX_RETRIES`] default.
    ///
    /// The limiter spaces consecutive outbound calls at least `1/rps` seconds
    /// apart (a fixed-rate limiter with a burst of 1). `None` or `Some(0)`
    /// disables rate limiting entirely. Values larger than `u32::MAX` are
    /// clamped.
    ///
    /// The underlying `reqwest::Client` is configured with connection pooling
    /// and TCP keep-alive so that HTTP connections are reused across multiple
    /// RPC calls within a single run, reducing handshake overhead.
    pub fn with_rate_limit(url: &str, rps: Option<u64>, verbose: bool) -> Self {
        Self::with_options(url, rps, DEFAULT_TIMEOUT, DEFAULT_MAX_RETRIES, verbose)
    }

    /// Create a new RPC client pointing at the given URL, optionally capping
    /// outbound requests to `rps` requests per second, bounding each HTTP
    /// request with `timeout`, and retrying transient failures up to
    /// `max_retries` times with exponential backoff.
    ///
    /// The limiter spaces consecutive outbound calls at least `1/rps` seconds
    /// apart (a fixed-rate limiter with a burst of 1). `None` or `Some(0)`
    /// disables rate limiting entirely. Values larger than `u32::MAX` are
    /// clamped. `timeout` applies to the whole request (connect through
    /// response body) and is passed straight to reqwest. A `max_retries` of
    /// `0` disables retries.
    pub fn with_options(
        url: &str,
        rps: Option<u64>,
        timeout: Duration,
        max_retries: usize,
        verbose: bool,
    ) -> Self {
        Self::with_fallback(url, None, rps, timeout, max_retries, verbose)
    }

    /// Create a new RPC client pointing at the given URL, optionally capping
    /// outbound requests to `rps` requests per second, bounding each HTTP
    /// request with `timeout`, bounding the TCP connection establishment with
    /// `connect_timeout`, and retrying transient failures up to `max_retries`
    /// times with exponential backoff.
    ///
    /// Rate limiting, retry behavior, and `timeout` behave exactly as in
    /// [`Self::with_options`]. `connect_timeout` bounds only the initial
    /// connect — a zero duration disables it (the total request timeout then
    /// applies to the connect phase too).
    pub fn with_connect_timeout(
        url: &str,
        rps: Option<u64>,
        timeout: Duration,
        connect_timeout: Duration,
        max_retries: usize,
    ) -> Self {
        Self::from_parts(
            url,
            None,
            rps,
            timeout,
            connect_timeout,
            max_retries,
            HeaderMap::new(),
            false,
        )
    }

    /// Create a new RPC client pointing at the given URL, with an optional
    /// secondary URL used for failover, optionally capping outbound requests
    /// to `rps` requests per second, bounding each HTTP request with
    /// `timeout`, and retrying transient failures up to `max_retries` times
    /// with exponential backoff.
    ///
    /// When a request to the primary endpoint fails with a network-level
    /// error (connection refused, timeout, DNS failure, etc.) or returns a
    /// transient gateway status (HTTP 502/503/504) and a fallback URL is
    /// configured, the request is retried against the fallback before the
    /// error is propagated. RPC-level errors (e.g. bad method, invalid
    /// params) are not retried against the fallback — they would fail there
    /// too.
    ///
    /// The limiter, timeout, and retry behavior behave exactly as in
    /// [`Self::with_options`].
    pub fn with_fallback(
        url: &str,
        fallback_url: Option<&str>,
        rps: Option<u64>,
        timeout: Duration,
        max_retries: usize,
        verbose: bool,
    ) -> Self {
        Self::from_parts(
            url,
            fallback_url,
            rps,
            timeout,
            DEFAULT_CONNECT_TIMEOUT,
            max_retries,
            HeaderMap::new(),
            verbose,
        )
    }

    /// Create a new RPC client that attaches custom HTTP headers (each a
    /// `KEY=VALUE` or `KEY: VALUE` string) to every request, without rate
    /// limiting, with the default request timeout, the default connect
    /// timeout, and the default retry policy.
    ///
    /// # Errors
    /// Returns [`AppError::InvalidHeader`] when any entry is malformed
    /// (missing `=`/`:`, empty name, or a name/value `reqwest` rejects).
    /// Headers are validated up front rather than silently dropped, so a
    /// typo can never quietly leave a request unauthenticated.
    pub fn with_headers(url: &str, headers: &[String], verbose: bool) -> AppResult<Self> {
        Self::with_fallback_headers_connect_timeout(
            url,
            None,
            None,
            DEFAULT_TIMEOUT,
            DEFAULT_CONNECT_TIMEOUT,
            DEFAULT_MAX_RETRIES,
            headers,
            verbose,
        )
    }

    /// Create a new RPC client with an optional fallback URL, optional rate
    /// limit, request timeout, retry policy, and custom HTTP headers attached
    /// to every request.
    ///
    /// Behaves exactly like [`Self::with_fallback_headers_connect_timeout`] and
    /// additionally attaches the parsed `KEY=VALUE` / `KEY: VALUE` headers to
    /// every outbound request. Uses the [`DEFAULT_CONNECT_TIMEOUT`] default.
    ///
    /// # Errors
    /// Returns [`AppError::InvalidHeader`] when any supplied header is
    /// malformed. Validation happens before any network traffic so a bad
    /// `--header` argument fails immediately.
    pub fn with_fallback_headers(
        url: &str,
        fallback_url: Option<&str>,
        rps: Option<u64>,
        timeout: Duration,
        max_retries: usize,
        headers: &[String],
        verbose: bool,
    ) -> AppResult<Self> {
        Self::with_fallback_headers_connect_timeout(
            url,
            fallback_url,
            rps,
            timeout,
            DEFAULT_CONNECT_TIMEOUT,
            max_retries,
            headers,
            verbose,
        )
    }

    /// Create a new RPC client with an optional fallback URL, optional rate
    /// limit, request timeout, TCP connect timeout, retry policy, and custom
    /// HTTP headers attached to every request.
    ///
    /// Behaves exactly like [`Self::with_fallback_headers`], additionally
    /// bounding the TCP connection establishment with `connect_timeout`.
    /// A zero `connect_timeout` disables the connect timeout (the total
    /// request timeout then applies to the connect phase too).
    /// # Errors
    /// Returns [`AppError::InvalidHeader`] when any supplied header is
    /// malformed.
    pub fn with_fallback_headers_connect_timeout(
        url: &str,
        fallback_url: Option<&str>,
        rps: Option<u64>,
        timeout: Duration,
        connect_timeout: Duration,
        max_retries: usize,
        headers: &[String],
        verbose: bool,
    ) -> AppResult<Self> {
        debug!(
            url,
            ?fallback_url,
            rps,
            ?timeout,
            ?connect_timeout,
            max_retries,
            headers = %redacted_headers(headers),
            "creating RPC client"
        );
        let headers = parse_headers(headers)?;
        Ok(Self::from_parts(
            url,
            fallback_url,
            rps,
            timeout,
            connect_timeout,
            max_retries,
            headers,
            verbose,
        ))
    }

    /// Shared, infallible construction path for every public constructor.
    ///
    /// `headers` has already been parsed and validated by the caller, so this
    /// only wires the client together.
    fn from_parts(
        url: &str,
        fallback_url: Option<&str>,
        rps: Option<u64>,
        timeout: Duration,
        connect_timeout: Duration,
        max_retries: usize,
        headers: HeaderMap,
        verbose: bool,
    ) -> Self {
        Self {
            url: url.to_string(),
            fallback_url: fallback_url.map(String::from),
            // `ClientBuilder::build` only fails on invalid configuration (a
            // default builder cannot), so fall back to a plain client to keep
            // construction infallible.
            client: reqwest::Client::builder()
                .timeout(timeout)
                .connect_timeout(connect_timeout)
                .tcp_keepalive(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(90))
                .default_headers(headers.clone())
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            dedup: Arc::new(Mutex::new(DedupState::default())),
            limiter: rps.and_then(build_rate_limiter),
            max_retries,
            connect_timeout,
            headers,
            verbose,
        }
    }

    /// Returns the custom HTTP headers configured for this client.
    ///
    /// These are the headers parsed from the `--header` values and attached to
    /// every outbound request; exposed for diagnostics and for tests that pin
    /// down header parsing/merging behavior.
    pub fn custom_headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The custom HTTP headers attached to every outbound request, with
    /// sensitive values replaced by `<redacted>`.
    ///
    /// Safe to print at any log level: credentials never reach the log.
    pub fn redacted_headers(&self) -> HeaderMap {
        redact_sensitive(&self.headers)
    }

    /// Validate that the RPC endpoint is reachable and healthy before any
    /// simulation is run.
    ///
    /// Issues a lightweight `getHealth` JSON-RPC call and fails fast with a
    /// clear, actionable error when the endpoint cannot be reached or reports
    /// a status other than `healthy` — so a misconfigured `--rpc-url` (or a
    /// down RPC node) is surfaced up front instead of surfacing midway through
    /// an expensive batch of simulations.
    ///
    /// # Network calls
    /// Makes at most one `getHealth` RPC call to the configured endpoint.
    pub async fn health_check(&self) -> AppResult<()> {
        let health: HealthResponse = self
            .call("getHealth", serde_json::json!({}))
            .await
            .map_err(|e| {
                AppError::Rpc {
                    status: -1,
                    message: format!(
                        "unable to reach RPC endpoint {url}: {e}. Check --rpc-url / --network and that the node is reachable.",
                        url = self.url
                    ),
                }
            })?;

        if health.status == "healthy" {
            debug!(url = self.url, "RPC endpoint health check passed");
            Ok(())
        } else {
            Err(AppError::Rpc {
                status: -1,
                message: format!(
                    "RPC endpoint {url} reported unhealthy status: {status}. Check --rpc-url / --network.",
                    url = self.url,
                    status = health.status,
                ),
            })
        }
    }

    /// Send a JSON-RPC request and deserialize the response.
    ///
    /// Requests are deduplicated by `(method, params)`:
    ///
    /// * a request identical to one that already **completed** returns the
    ///   cached result without sending anything;
    /// * a request identical to one currently **in flight** attaches to that
    ///   request's [`SharedFuture`] and awaits it, so concurrent duplicates
    ///   share a single underlying future and a single HTTP POST.
    ///
    /// A failed leader publishes no outcome, so its followers loop back and
    /// become the next leader — a failure costs one retry, never a poisoned
    /// batch.
    ///
    /// # Network calls
    /// At most one HTTP POST for any distinct `(method, params)` pair while it
    /// is in flight; zero for a cache hit or a follower.
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

            // Attach to the in-flight single-flight handle for this key, or
            // become its leader by installing a fresh one. Attachment happens
            // under the state lock so two callers can never both become
            // leader for the same key.
            let (shared, is_leader) = {
                let mut state = self.dedup.lock().await;
                if let Some(existing) = state.in_flight.get(&key) {
                    (existing.clone(), false)
                } else {
                    let handle = SharedFuture::default();
                    state.in_flight.insert(key.clone(), handle.clone());
                    (handle, true)
                }
            };

            if !is_leader {
                // Follower: await the leader's shared future. A successful
                // leader published its result for us; a failed one published
                // nothing, so loop back and become the next leader (a retry).
                shared.wait().await;
                if let Some(value) = shared.outcome().await {
                    trace!(method, "deduplicated against in-flight request");
                    return deserialize_result::<T>(value);
                }
                continue;
            }

            // Leader: perform the network request, publish the result for any
            // followers, then release the key so later callers can reuse the
            // completed result.
            let result = self.perform_call(method, params).await;
            if let Ok(value) = &result {
                shared.publish(value.clone()).await;
            }
            {
                let mut state = self.dedup.lock().await;
                if let Ok(value) = &result {
                    state.completed.insert(key.clone(), value.clone());
                }
                state.in_flight.remove(&key);
            }
            shared.finish();
            return result.and_then(deserialize_result::<T>);
        }
    }

    /// Send a JSON-RPC request, bypassing request deduplication.
    ///
    /// Every call reaches the network. This is what benchmarking paths (e.g.
    /// `estimate --repeat`) use: otherwise an identical request would be
    /// served from the dedup cache and only the first run would be measured.
    ///
    /// # Network calls
    /// One HTTP POST (plus any transient-failure retries).
    pub async fn call_uncached<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> AppResult<T> {
        self.perform_call(method, params)
            .await
            .and_then(deserialize_result::<T>)
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
    /// or a transient gateway status (HTTP 502/503/504) and a fallback URL is
    /// configured, retries against the fallback, logging a notice to stderr.
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

        trace!(
            method,
            header_count = self.headers.len(),
            "sending RPC request"
        );
        match self
            .post_and_parse(method, &body, &self.url, self.connect_timeout)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) if Self::is_failover_trigger(&e) => {
                if let Some(ref fallback) = self.fallback_url {
                    // The operator-facing notice goes to stderr so it never
                    // contaminates machine-readable stdout (json/csv/markdown).
                    eprintln!("Primary RPC failed, failing over to fallback endpoint: {fallback}");
                    debug!(
                        method,
                        primary = %self.url,
                        fallback = %fallback,
                        error = %e,
                        "primary RPC endpoint failed — failing over to fallback"
                    );
                    self.post_and_parse(method, &body, fallback, self.connect_timeout)
                        .await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// True when `error` should trigger a failover attempt against the
    /// configured fallback endpoint.
    ///
    /// This covers transport-level failures (connection refused, timeout,
    /// DNS failure, ...) and transient gateway statuses (HTTP 502/503/504).
    /// RPC-level errors returned inside a successful HTTP response — bad
    /// method, invalid params — are **not** failover triggers: they would
    /// fail identically on the fallback.
    fn is_failover_trigger(error: &AppError) -> bool {
        match error {
            AppError::Http(e) => {
                // reqwest errors that indicate connectivity problems — these
                // are the cases where a fallback endpoint might succeed.
                e.is_connect() || e.is_timeout() || e.is_request()
            }
            AppError::ConnectTimeout { .. } | AppError::RpcUnavailable { .. } => true,
            // Transient gateway statuses surfaced as HttpStatus (502/503/504)
            // are also failover triggers; other HttpStatus values (429/500 or
            // deterministic 4xx) are not — 429/500 are retried but not failed
            // over, matching the gateway-only failover contract.
            AppError::HttpStatus { status, .. } => matches!(status, 502..=504),
            _ => false,
        }
    }

    /// POST `body` to `url` (with retries), parse the JSON-RPC response, and
    /// extract the raw `result` value.
    async fn post_and_parse(
        &self,
        method: &str,
        body: &Value,
        url: &str,
        connect_timeout: Duration,
    ) -> AppResult<Value> {
        let client = self.client.clone();
        let url = url.to_string();
        let request_body = body.clone();
        let limiter = self.limiter.clone();

        let start = std::time::Instant::now();
        let request_body_str = serde_json::to_string(&request_body).unwrap_or_default();
        let payload_size = request_body_str.len();
        if self.verbose {
            eprintln!("[RPC] POST {} ({} bytes)", url, payload_size);
            eprintln!("[RPC] -> {}", request_body_str);
        }

        let response = with_retry(self.max_retries, || {
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
                let response = client
                    .post(&url)
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| connect_timeout_error(e, connect_timeout))?;

                // Transient statuses (429/5xx) must be turned into errors so
                // `with_retry` can back off and retry them. Non-transient
                // statuses fall through and are parsed as JSON-RPC below.
                let status = response.status();
                if is_transient_status(status) {
                    return Err(AppError::HttpStatus {
                        status: status.as_u16(),
                        retry_after: parse_retry_after(response.headers()),
                        message: format!("RPC endpoint returned transient HTTP status {status}"),
                    });
                }
                Ok::<reqwest::Response, AppError>(response)
            }
        })
        .await?;
        let status = response.status();

        // A 502/503/504 means the endpoint is briefly unavailable rather than
        // misconfigured. Surface it as a dedicated error *before* parsing the
        // body — gateways commonly return an HTML error page, and both the
        // retry loop and the failover logic key off this variant.
        let status_code = status.as_u16();
        if matches!(status_code, 502..=504) {
            debug!(
                method,
                status = status_code,
                "transient gateway error from RPC endpoint"
            );
            return Err(AppError::RpcUnavailable {
                status: status_code,
            });
        }

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

        let elapsed = start.elapsed();
        if self.verbose {
            let response_str = serde_json::to_string(&result).unwrap_or_default();
            eprintln!("[RPC] <- HTTP {} ({} ms)", status, elapsed.as_millis());
            eprintln!("[RPC] <- {}", response_str);
        }

        debug!(
            method,
            status = %status,
            result = %serde_json::to_string(result).unwrap_or_default(),
            "RPC response received"
        );
        trace!(method, "RPC call succeeded");
        Ok(result.clone())
    }
}

/// Rewrap a reqwest error whose connect phase timed out into an
/// [`AppError::ConnectTimeout`], so an unreachable host produces a distinct,
/// actionable message instead of a generic request timeout.
///
/// In reqwest, an expired `connect_timeout` surfaces as a timeout-flagged
/// error whose source chain also carries a connect error (a `TimedOut` marker
/// inside a hyper connect error), so `is_timeout() && is_connect()` separates
/// it from a whole-request timeout (`is_timeout()` without `is_connect()`).
/// Every other error maps to [`AppError::Http`] as before.
fn connect_timeout_error(error: reqwest::Error, connect_timeout: Duration) -> AppError {
    if !connect_timeout.is_zero() && error.is_timeout() && error.is_connect() {
        AppError::ConnectTimeout {
            seconds: connect_timeout.as_secs(),
            source: error,
        }
    } else {
        AppError::from(error)
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

/// Returns `true` when `status` is a transient HTTP failure worth retrying.
///
/// Rate limiting (429) and server-side errors (500/502/503/504) are
/// transient; everything else — including deterministic 4xx client errors —
/// is returned to the caller as-is.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
}

/// Parses a `Retry-After` response header into a delay, if present.
///
/// Only the delta-seconds form (e.g. `Retry-After: 5`) is supported; the
/// HTTP-date form and unparseable values yield `None`, falling back to the
/// retry loop's computed backoff.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

/// Deserializes a raw JSON-RPC `result` value into the caller's type.
fn deserialize_result<T: serde::de::DeserializeOwned>(value: Value) -> AppResult<T> {
    serde_json::from_value(value)
        .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
}

/// Header names whose values are credentials and must never be logged.
///
/// Matching is case-insensitive (HTTP header names are), so `X-Api-Key` and
/// `x-api-key` are both redacted.
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "cookie",
];

/// Placeholder substituted for a sensitive header value in logs.
const REDACTED: &str = "<redacted>";

/// True when `name` identifies a header whose value must never be logged.
#[must_use]
pub fn is_sensitive_header(name: &HeaderName) -> bool {
    let lower = name.as_str().to_ascii_lowercase();
    SENSITIVE_HEADERS.iter().any(|s| *s == lower)
}

/// Returns a copy of `headers` with every sensitive value replaced by
/// `<redacted>`.
///
/// Used for diagnostics (`--verbose` / `debug!` logs) so an API key or bearer
/// token is never written to a terminal, log file, or CI transcript.
#[must_use]
pub fn redact_sensitive(headers: &HeaderMap) -> HeaderMap {
    let mut redacted = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        if is_sensitive_header(name) {
            let placeholder = HeaderValue::from_static(REDACTED);
            redacted.insert(name.clone(), placeholder);
        } else {
            redacted.insert(name.clone(), value.clone());
        }
    }
    redacted
}

/// Renders raw `"Key: Value"` / `"Key=Value"` CLI arguments for logging, with
/// sensitive values redacted before they ever reach a log sink.
///
/// Operates on the raw strings (rather than a parsed `HeaderMap`) so it can be
/// called before validation, while a malformed argument is still being
/// reported.
fn redacted_headers(raw_headers: &[String]) -> String {
    if raw_headers.is_empty() {
        return "(none)".to_string();
    }
    let rendered: Vec<String> = raw_headers
        .iter()
        .map(|raw| match split_header(raw) {
            Some((name, _)) if is_sensitive_header_text(name) => format!("{name}={REDACTED}"),
            _ => raw.clone(),
        })
        .collect();
    rendered.join(", ")
}

/// True when the (case-insensitive) `name` is a credential-bearing header.
fn is_sensitive_header_text(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    SENSITIVE_HEADERS.iter().any(|s| *s == lower)
}

/// Splits a raw header argument into `(name, value)`.
///
/// Both spellings are accepted: `KEY=VALUE` (documented) and `KEY: VALUE`
/// (the conventional HTTP form). The **earliest** separator wins, so
/// `Authorization=Bearer a:b` and `Authorization: Bearer a=b` both split after
/// the header name instead of truncating the value at a later separator.
fn split_header(raw: &str) -> Option<(&str, &str)> {
    let equals = raw.find('=');
    let colon = raw.find(':');
    let position = match (equals, colon) {
        (Some(equals), Some(colon)) => equals.min(colon),
        (Some(equals), None) => equals,
        (None, Some(colon)) => colon,
        (None, None) => return None,
    };
    let (name, value) = raw.split_at(position);
    Some((name.trim(), value[1..].trim()))
}

/// Parse a single raw header argument into an HTTP header name and value.
///
/// # Errors
/// Returns [`AppError::InvalidHeader`] when the argument has no `=`/`:`
/// separator, when the header name is empty, or when `reqwest` rejects the
/// name or value (illegal characters, non-visible ASCII, …).
pub fn parse_header(raw: &str) -> AppResult<(HeaderName, HeaderValue)> {
    let Some((name_str, value_str)) = split_header(raw) else {
        return Err(AppError::InvalidHeader(format!(
            "'{raw}' is not a KEY=VALUE pair (or KEY: VALUE)"
        )));
    };

    if name_str.is_empty() {
        return Err(AppError::InvalidHeader(format!(
            "'{raw}' has an empty header name"
        )));
    }

    let name = HeaderName::from_bytes(name_str.as_bytes())
        .map_err(|e| AppError::InvalidHeader(format!("invalid header name '{name_str}': {e}")))?;

    if value_str.is_empty() {
        return Err(AppError::InvalidHeader(format!(
            "header '{name_str}' has an empty value"
        )));
    }

    let value = HeaderValue::from_str(value_str).map_err(|e| {
        AppError::InvalidHeader(format!("invalid header value for '{name_str}': {e}"))
    })?;

    Ok((name, value))
}

/// Parse a list of raw header arguments into a [`HeaderMap`].
///
/// # Errors
/// Returns the first [`AppError::InvalidHeader`] encountered, so a malformed
/// `--header` argument fails the command before any network traffic.
pub fn parse_headers(raw_headers: &[String]) -> AppResult<HeaderMap> {
    let mut headers = HeaderMap::with_capacity(raw_headers.len());
    for raw in raw_headers {
        let (name, value) = parse_header(raw)?;
        headers.insert(name, value);
    }
    Ok(headers)
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
    use crate::rpc::retry::DEFAULT_MAX_RETRIES;

    use super::{RpcClient, resolve_ws_endpoint};

    /// Spawns a tiny HTTP server that answers JSON-RPC
    /// `simulateTransaction`-style calls with `{"result":{"pong":true}}`,
    /// counting how many were received. The first `fail_times` calls return
    /// a JSON-RPC error body instead of a result.
    async fn spawn_json_rpc_stub(fail_times: u32) -> (String, Arc<AtomicUsize>) {
        spawn_json_rpc_stub_with_result(fail_times, r#"{"pong":true}"#).await
    }

    /// Like [`spawn_json_rpc_stub`], but each response is held back by `delay`
    /// before being written. This keeps the leader's request genuinely in
    /// flight, so concurrent followers must coalesce against its shared future
    /// rather than racing to completion.
    async fn spawn_delayed_json_rpc_stub(delay: Duration) -> (String, Arc<AtomicUsize>) {
        spawn_json_rpc_stub_with_delay(0, r#"{"pong":true}"#, delay).await
    }

    /// Like [`spawn_json_rpc_stub`], but successful responses embed `result_body`
    /// verbatim as the JSON-RPC `result` value — for stubbing methods with a
    /// specific response shape (e.g. `getHealth`).
    async fn spawn_json_rpc_stub_with_result(
        fail_times: u32,
        result_body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
        spawn_json_rpc_stub_with_delay(fail_times, result_body, Duration::ZERO).await
    }

    /// Spawns a JSON-RPC stub that fails the first `fail_times` calls, returns
    /// `result_body` on success, and waits `delay` before answering every
    /// request.
    async fn spawn_json_rpc_stub_with_delay(
        fail_times: u32,
        result_body: &'static str,
        delay: Duration,
    ) -> (String, Arc<AtomicUsize>) {
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
                    let _ = handle_conn(stream, counter, fail_times, 200, result_body, delay).await;
                });
            }
        });

        (format!("http://{addr}"), counter)
    }

    /// Spawns a stub server whose first `fail_times` responses carry HTTP
    /// `fail_status` (e.g. 503) and whose later responses are 200 JSON-RPC
    /// successes. Exercises the transient-status retry path.
    async fn spawn_stub(
        fail_times: u32,
        fail_status: u16,
        result_body: &'static str,
    ) -> (String, Arc<AtomicUsize>) {
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
                    let _ = handle_conn(
                        stream,
                        counter,
                        fail_times,
                        fail_status,
                        result_body,
                        Duration::ZERO,
                    )
                    .await;
                });
            }
        });

        (format!("http://{addr}"), counter)
    }

    /// HTTP reason phrase for the stubbed failure status.
    fn status_reason(status: u16) -> &'static str {
        match status {
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "OK",
        }
    }

    async fn handle_conn(
        mut stream: TcpStream,
        counter: Arc<AtomicUsize>,
        fail_times: u32,
        fail_status: u16,
        result_body: &'static str,
        delay: Duration,
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
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let (status, body) = if (call_no as u32) < fail_times {
            if fail_status == 200 {
                (
                    200u16,
                    r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"stubbed failure"}}"#
                        .to_string(),
                )
            } else {
                (
                    fail_status,
                    r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"stubbed failure"}}"#
                        .to_string(),
                )
            }
        } else {
            (
                200u16,
                format!(r#"{{"jsonrpc":"2.0","id":1,"result":{result_body}}}"#),
            )
        };
        let response = format!(
            "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            status_reason(status),
            body.len()
        );
        stream.write_all(response.as_bytes()).await?;
        stream.flush().await?;
        Ok(())
    }

    /// Spawns a tiny HTTP server that always answers with `status` and a
    /// plain-text (non-JSON) body, counting how many requests it received.
    /// Used to exercise transient gateway failures (HTTP 502/503/504) and
    /// other non-JSON error statuses.
    async fn spawn_status_stub(status: u16) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let addr = listener.local_addr().expect("no local address");
        let counter = Arc::new(AtomicUsize::new(0));
        let server_counter = Arc::clone(&counter);

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let counter = Arc::clone(&server_counter);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    counter.fetch_add(1, Ordering::SeqCst);
                    let body = "upstream unavailable";
                    let response = format!(
                        "HTTP/1.1 {status} Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });

        (format!("http://{addr}"), counter)
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

    /// `call_uncached` must bypass dedup: identical requests both reach the
    /// network, which is what makes `estimate --repeat` benchmark N real runs.
    #[tokio::test]
    async fn test_call_uncached_bypasses_dedup() {
        let (url, counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::new(&url);
        let params = serde_json::json!({"k": "v"});

        let _: Value = client
            .call_uncached("test.method", params.clone())
            .await
            .expect("first uncached call");
        let _: Value = client
            .call_uncached("test.method", params)
            .await
            .expect("second uncached call");

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "identical uncached requests must both hit the network"
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

    /// Many identical calls fired concurrently against a deliberately slow
    /// endpoint must all attach to the leader's single in-flight shared future:
    /// exactly one HTTP request, and every caller observes the leader's result.
    #[tokio::test]
    async fn test_dedup_many_concurrent_identical_requests_share_one_future() {
        let (url, counter) = spawn_delayed_json_rpc_stub(Duration::from_millis(50)).await;
        let client = Arc::new(RpcClient::new(&url));

        let mut handles = Vec::new();
        for _ in 0..32 {
            let client = Arc::clone(&client);
            handles.push(tokio::spawn(async move {
                client
                    .call::<Value>("test.method", serde_json::json!({"k": "v"}))
                    .await
            }));
        }

        for handle in handles {
            let value = handle
                .await
                .expect("task should not panic")
                .expect("shared future must resolve for every follower");
            assert_eq!(
                value,
                serde_json::json!({"pong": true}),
                "every follower must observe the leader's result"
            );
        }
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "all identical in-flight requests must share a single future"
        );
    }

    /// Concurrent calls split across two distinct payloads must coalesce into
    /// exactly two in-flight futures — one network request per distinct key.
    #[tokio::test]
    async fn test_dedup_concurrent_mixed_payloads() {
        let (url, counter) = spawn_delayed_json_rpc_stub(Duration::from_millis(50)).await;
        let client = Arc::new(RpcClient::new(&url));

        let mut handles = Vec::new();
        for i in 0..40 {
            let client = Arc::clone(&client);
            let k = if i % 2 == 0 { "even" } else { "odd" };
            handles.push(tokio::spawn(async move {
                let _: Value = client
                    .call("test.method", serde_json::json!({ "k": k }))
                    .await
                    .expect("mixed concurrent call");
            }));
        }
        for handle in handles {
            handle.await.expect("task should not panic");
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "only the two distinct payloads may hit the network"
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
        let client = RpcClient::with_rate_limit(&url, Some(20), false);

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
        let client = RpcClient::with_rate_limit(&url, Some(20), false);
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
        let client = RpcClient::with_rate_limit(&url, Some(0), false);

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
        let client = RpcClient::with_options(&url, None, Duration::from_millis(100), 0, false);

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(
            result.is_err(),
            "a hanging server must eventually produce a timeout error"
        );
    }
    /// The `connect_timeout` supplied to `with_connect_timeout` must be
    /// stored verbatim on the client (asserts the configuration is actually
    /// wired through, per the issue's acceptance criteria).
    #[test]
    fn test_connect_timeout_config_is_stored() {
        let client = RpcClient::with_connect_timeout(
            "http://localhost",
            None,
            Duration::from_secs(30),
            Duration::from_secs(7),
            3,
        );
        assert_eq!(client.connect_timeout, Duration::from_secs(7));
    }

    /// Constructors without an explicit connect timeout must fall back to
    /// [`super::DEFAULT_CONNECT_TIMEOUT`], which matches the CLI's
    /// `--connect-timeout` default of 5 seconds.
    #[test]
    fn test_default_connect_timeout_is_five_seconds() {
        let client =
            RpcClient::with_options("http://localhost", None, Duration::from_secs(30), 3, false);
        assert_eq!(client.connect_timeout, Duration::from_secs(5));

        let client = RpcClient::with_fallback_headers(
            "http://localhost",
            None,
            None,
            Duration::from_secs(30),
            3,
            &[],
            false,
        )
        .expect("valid headers");
        assert_eq!(client.connect_timeout, Duration::from_secs(5));
    }

    /// A zero connect timeout is the documented "disabled" sentinel: it must
    /// be preserved as-is so callers can observe the disable convention.
    #[test]
    fn test_zero_connect_timeout_is_preserved_as_disabled() {
        let client = RpcClient::with_connect_timeout(
            "http://localhost",
            None,
            Duration::from_secs(30),
            Duration::ZERO,
            3,
        );
        assert_eq!(client.connect_timeout, Duration::ZERO);
    }

    /// A connect timeout failure must surface the distinct, actionable
    /// message (acceptance criterion), separate from the generic request
    /// timeout error.
    #[tokio::test]
    async fn test_connect_timeout_error_message_is_distinct() {
        let source = reqwest::get("http://127.0.0.1:1")
            .await
            .expect_err("port 1 must refuse");
        let error = AppError::ConnectTimeout { seconds: 5, source };
        let message = error.to_string();
        assert_eq!(
            message,
            "failed to establish connection to RPC host within 5 seconds"
        );
    }

    /// Only a genuine "connect phase timed out" error (timeout-flagged AND
    /// connect-flagged) may be rewrapped as [`AppError::ConnectTimeout`]. A
    /// connection-refused error is connect-flagged but not timeout-flagged,
    /// so it must stay a plain [`AppError::Http`].
    #[tokio::test]
    async fn test_connect_timeout_error_does_not_absorb_refused_connections() {
        let refused = reqwest::get("http://127.0.0.1:1")
            .await
            .expect_err("port 1 must refuse");

        let mapped = super::connect_timeout_error(refused, Duration::from_secs(5));
        assert!(
            matches!(mapped, AppError::Http(_)),
            "connection refused must not be rewrapped as a connect timeout: {mapped}"
        );
    }

    /// With the connect timeout disabled (zero), even a timeout-flagged,
    /// connect-flagged error must stay a plain [`AppError::Http`].
    #[tokio::test]
    async fn test_disabled_connect_timeout_skips_rewrapping() {
        let refused = reqwest::get("http://127.0.0.1:1")
            .await
            .expect_err("port 1 must refuse");

        let mapped = super::connect_timeout_error(refused, Duration::ZERO);
        assert!(matches!(mapped, AppError::Http(_)));
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
            DEFAULT_MAX_RETRIES,
            false,
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
            DEFAULT_MAX_RETRIES,
            false,
        );

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(result.is_err(), "RPC-level errors must be propagated");
        assert_eq!(
            fallback_counter.load(Ordering::SeqCst),
            0,
            "fallback must not be contacted for RPC-level errors"
        );
    }

    /// A primary endpoint answering 502/503/504 is transiently unavailable:
    /// the request must fail over to the fallback instead of surfacing the
    /// gateway error. `max_retries: 0` keeps the test fast.
    #[tokio::test]
    async fn test_failover_on_primary_gateway_error() {
        for status in [502u16, 503, 504] {
            let (primary_url, primary_counter) = spawn_status_stub(status).await;
            let (fallback_url, fallback_counter) = spawn_json_rpc_stub(0).await;
            let client = RpcClient::with_fallback(
                &primary_url,
                Some(&fallback_url),
                None,
                Duration::from_secs(30),
                0,
                false,
            );

            let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

            assert!(
                result.is_ok(),
                "HTTP {status} from the primary must fail over to the fallback: {result:?}"
            );
            assert_eq!(
                primary_counter.load(Ordering::SeqCst),
                1,
                "primary should be tried exactly once with retries disabled"
            );
            assert_eq!(
                fallback_counter.load(Ordering::SeqCst),
                1,
                "fallback endpoint should have served the request"
            );
        }
    }

    /// A non-transient HTTP error (e.g. 500) is not a failover trigger: the
    /// error must propagate without contacting the fallback.
    #[tokio::test]
    async fn test_no_failover_on_non_gateway_http_error() {
        let (primary_url, _) = spawn_status_stub(500).await;
        let (fallback_url, fallback_counter) = spawn_json_rpc_stub(0).await;
        let client = RpcClient::with_fallback(
            &primary_url,
            Some(&fallback_url),
            None,
            Duration::from_secs(30),
            0,
            false,
        );

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(result.is_err(), "HTTP 500 must be propagated");
        assert_eq!(
            fallback_counter.load(Ordering::SeqCst),
            0,
            "fallback must not be contacted for a non-gateway HTTP error"
        );
    }

    /// A reachable endpoint reporting `healthy` must pass the health check.
    #[tokio::test]
    async fn test_health_check_ok_when_healthy() {
        let (url, _) =
            spawn_json_rpc_stub_with_result(0, r#"{"status":"healthy","latestLedger":42}"#).await;
        let client = RpcClient::new(&url);

        client
            .health_check()
            .await
            .expect("healthy endpoint passes");
    }

    /// A reachable endpoint reporting anything other than `healthy` (e.g.
    /// `degraded`) must fail the health check with an actionable error.
    #[tokio::test]
    async fn test_health_check_errs_on_non_healthy_status() {
        let (url, _) = spawn_json_rpc_stub_with_result(0, r#"{"status":"degraded"}"#).await;
        let client = RpcClient::new(&url);

        let err = client
            .health_check()
            .await
            .expect_err("degraded endpoint fails");
        let message = err.to_string();
        assert!(
            message.contains("reported unhealthy status: degraded"),
            "unexpected error: {message}"
        );
    }

    /// An unreachable endpoint must fail the health check fast with a message
    /// pointing at `--rpc-url` / `--network` rather than surfacing later mid-
    /// simulation.
    #[tokio::test]
    async fn test_health_check_errs_when_endpoint_unreachable() {
        let dead_url = refused_port_url().await;
        // `max_retries: 0` keeps this fast: a refused connection is retryable,
        // and the default backoff (0.5s + 1s + 2s) is irrelevant to the
        // health check failing fast on an unreachable endpoint.
        let client = RpcClient::with_options(&dead_url, None, Duration::from_millis(500), 0, false);

        let err = client
            .health_check()
            .await
            .expect_err("dead endpoint fails");
        let message = err.to_string();
        assert!(
            message.contains("unable to reach RPC endpoint") && message.contains("--rpc-url"),
            "unexpected error: {message}"
        );
    }

    /// A transient HTTP status (503) must be retried; once the stub starts
    /// answering 200 the call succeeds.
    #[tokio::test]
    async fn test_transient_status_is_retried_then_succeeds() {
        let (url, counter) = spawn_stub(1, 503, r#"{"pong":true}"#).await;
        let client = RpcClient::with_options(&url, None, Duration::from_secs(5), 3, false);

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(
            result.is_ok(),
            "503 should be retried then succeed: {result:?}"
        );
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    /// Persistent transient statuses consume every retry, and the final error
    /// preserves the HTTP status.
    #[tokio::test]
    async fn test_transient_status_exhausts_retries() {
        let (url, counter) = spawn_stub(100, 503, r#"{"pong":true}"#).await;
        let client = RpcClient::with_options(&url, None, Duration::from_secs(5), 2, false);

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        match result {
            Err(AppError::HttpStatus { status, .. }) => assert_eq!(status, 503),
            other => panic!("expected HttpStatus(503), got {other:?}"),
        }
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    /// A deterministic 4xx client error must not be retried.
    #[tokio::test]
    async fn test_client_error_status_is_not_retried() {
        let (url, counter) = spawn_stub(100, 400, r#"{"pong":true}"#).await;
        let client = RpcClient::with_options(&url, None, Duration::from_secs(5), 3, false);

        let result: AppResult<Value> = client.call("test.method", serde_json::json!({})).await;

        assert!(result.is_err(), "400 must surface as an error");
        assert_eq!(counter.load(Ordering::SeqCst), 1, "400 must not be retried");
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

#[cfg(test)]
mod header_tests {
    use super::*;

    fn parse_ok(raw: &str) -> (String, String) {
        let (name, value) =
            parse_header(raw).unwrap_or_else(|e| panic!("expected {raw:?} to parse: {e}"));
        (
            name.as_str().to_string(),
            value.to_str().unwrap_or_default().to_string(),
        )
    }

    fn parse_err(raw: &str) -> AppError {
        parse_header(raw).expect_err(&format!("expected {raw:?} to be rejected"))
    }

    #[test]
    fn test_parse_header_equals_separator() {
        assert_eq!(
            parse_ok("X-API-Key=secret123"),
            ("x-api-key".to_string(), "secret123".to_string())
        );
    }

    #[test]
    fn test_parse_header_colon_separator_still_accepted() {
        assert_eq!(
            parse_ok("X-API-Key: secret123"),
            ("x-api-key".to_string(), "secret123".to_string())
        );
    }

    #[test]
    fn test_parse_header_with_spaces() {
        assert_eq!(
            parse_ok(" Authorization = Bearer tok "),
            ("authorization".to_string(), "Bearer tok".to_string())
        );
        assert_eq!(
            parse_ok(" Authorization : Bearer tok "),
            ("authorization".to_string(), "Bearer tok".to_string())
        );
    }

    /// A `Bearer` token may legitimately contain both `=` and `:`. The earliest
    /// separator must win so the value is never truncated.
    #[test]
    fn test_parse_header_value_with_colons() {
        assert_eq!(
            parse_ok("X-Auth=token:with:colons"),
            ("x-auth".to_string(), "token:with:colons".to_string())
        );
        assert_eq!(
            parse_ok("X-Auth: token:with:colons"),
            ("x-auth".to_string(), "token:with:colons".to_string())
        );
    }

    /// `=` is documented, but `:` is the conventional HTTP spelling and both
    /// appear in the wild. The earliest separator wins in either direction.
    #[test]
    fn test_parse_header_accepts_both_separators() {
        assert_eq!(
            parse_ok("X-Auth: Bearer a=b"),
            ("x-auth".to_string(), "Bearer a=b".to_string())
        );
        assert_eq!(
            parse_ok("X-Auth=Bearer a:b"),
            ("x-auth".to_string(), "Bearer a:b".to_string())
        );
    }

    #[test]
    fn test_parse_header_missing_separator_is_rejected() {
        let err = parse_err("NoSeparator");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(
            err.to_string().contains("KEY=VALUE"),
            "the error should name the expected format; got: {err}"
        );
    }

    #[test]
    fn test_parse_header_empty_name_is_rejected() {
        assert!(matches!(parse_err("= value"), AppError::InvalidHeader(_)));
        assert!(matches!(parse_err(": value"), AppError::InvalidHeader(_)));
    }

    #[test]
    fn test_parse_header_empty_value_is_rejected() {
        let err = parse_err("X-Custom=");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(err.to_string().contains("empty value"), "got: {err}");
    }

    /// A header name with a space is not a legal HTTP token, so `reqwest`
    /// rejects it — the error must surface rather than be dropped.
    #[test]
    fn test_parse_header_illegal_name_is_rejected() {
        let err = parse_err("X Custom=value");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(
            err.to_string().contains("invalid header name"),
            "got: {err}"
        );
    }

    /// A value containing a raw newline is not a legal header value.
    #[test]
    fn test_parse_header_illegal_value_is_rejected() {
        let err = parse_err("X-Custom=bad\nvalue");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(
            err.to_string().contains("invalid header value"),
            "got: {err}"
        );
    }

    #[test]
    fn test_parse_headers_empty() {
        assert!(parse_headers(&[]).expect("no headers").is_empty());
    }

    #[test]
    fn test_parse_headers_stores_parsed() {
        let headers = parse_headers(&[
            "X-API-Key: secret".to_string(),
            "Authorization: Bearer tok".to_string(),
        ])
        .expect("valid headers");
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn test_with_headers_empty() {
        let client =
            RpcClient::with_headers("http://localhost", &[], false).expect("valid headers");
        assert!(client.custom_headers().is_empty());
        assert!(client.headers.is_empty());
    }

    #[test]
    fn test_with_headers_stores_parsed() {
        let client = RpcClient::with_headers(
            "http://localhost",
            &[
                "X-API-Key=secret".to_string(),
                "Authorization=Bearer tok".to_string(),
            ],
            false,
        )
        .expect("valid headers");
        assert_eq!(client.custom_headers().len(), 2);
        assert_eq!(
            client
                .custom_headers()
                .get("x-api-key")
                .unwrap()
                .to_str()
                .unwrap(),
            "secret"
        );
        assert_eq!(
            client
                .custom_headers()
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer tok"
        );
    }

    /// Malformed arguments must fail the command: silently dropping them would
    /// send unauthenticated requests to a private endpoint.
    #[test]
    fn test_parse_headers_rejects_malformed() {
        let err = parse_headers(&[
            "Good: ok".to_string(),
            "NoColonHere".to_string(),
            "Also-Bad:".to_string(),
        ])
        .expect_err("a malformed entry must be rejected");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(err.to_string().contains("NoColonHere"), "got: {err}");
    }

    #[test]
    fn test_with_headers_rejects_malformed() {
        let err = RpcClient::with_headers(
            "http://localhost",
            &[
                "Good=ok".to_string(),
                "NoSeparator".to_string(),
                "Also-Bad=".to_string(),
            ],
            false,
        )
        .expect_err("a malformed entry must be rejected");
        assert!(matches!(err, AppError::InvalidHeader(_)));
        assert!(err.to_string().contains("NoSeparator"), "got: {err}");
    }

    #[test]
    fn test_rpc_client_new_has_no_customheaders() {
        let client = RpcClient::new("http://localhost");
        assert!(client.custom_headers().is_empty());
    }

    // ── Redaction ────────────────────────────────────────────────

    #[test]
    fn test_sensitive_header_detection_is_case_insensitive() {
        for name in [
            "Authorization",
            "authorization",
            "AUTHORIZATION",
            "X-API-Key",
            "x-api-key",
            "Api-Key",
            "Proxy-Authorization",
            "Cookie",
        ] {
            let parsed = HeaderName::from_bytes(name.as_bytes()).expect("valid name");
            assert!(is_sensitive_header(&parsed), "{name} must be redacted");
        }
        for name in ["X-Trace-Id", "User-Agent", "Accept"] {
            let parsed = HeaderName::from_bytes(name.as_bytes()).expect("valid name");
            assert!(!is_sensitive_header(&parsed), "{name} must not be redacted");
        }
    }

    #[test]
    fn test_redact_sensitive_replaces_values_but_keeps_names() {
        let raw = vec![
            "Authorization=Bearer super-secret".to_string(),
            "X-API-Key=key-123".to_string(),
            "X-Trace-Id=trace-9".to_string(),
        ];
        let parsed = parse_headers(&raw).expect("valid headers");
        let redacted = redact_sensitive(&parsed);

        assert_eq!(
            redacted.get("authorization").unwrap().to_str().unwrap(),
            REDACTED
        );
        assert_eq!(
            redacted.get("x-api-key").unwrap().to_str().unwrap(),
            REDACTED
        );
        // Non-sensitive headers stay readable — that is the point of logging them.
        assert_eq!(
            redacted.get("x-trace-id").unwrap().to_str().unwrap(),
            "trace-9"
        );
    }

    #[test]
    fn test_client_exposes_redacted_headers_only() {
        let client = RpcClient::with_headers(
            "http://localhost",
            &["Authorization=Bearer super-secret".to_string()],
            false,
        )
        .expect("valid headers");
        let redacted = client.redacted_headers();
        assert_eq!(
            redacted.get("authorization").unwrap().to_str().unwrap(),
            REDACTED
        );
        // The live value is still on the client — only the log copy is masked.
        assert_eq!(
            client
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer super-secret"
        );
    }

    /// The debug log renders raw CLI arguments, so it must not leak a secret
    /// even before/without parsing.
    #[test]
    fn test_redacted_headers_renders_raw_arguments_safely() {
        let raw = vec![
            "Authorization=Bearer super-secret".to_string(),
            "X-API-Key: key-123".to_string(),
            "X-Trace-Id=trace-9".to_string(),
        ];
        let rendered = redacted_headers(&raw);
        assert!(!rendered.contains("super-secret"), "leaked: {rendered}");
        assert!(!rendered.contains("key-123"), "leaked: {rendered}");
        assert!(
            rendered.contains("trace-9"),
            "lost a safe value: {rendered}"
        );
        assert!(
            rendered.contains("Authorization"),
            "lost the name: {rendered}"
        );
    }

    #[test]
    fn test_redacted_headers_handles_empty_and_malformed() {
        assert_eq!(redacted_headers(&[]), "(none)");
        // A malformed argument has no name to classify; it is echoed verbatim
        // because it cannot carry a parsed credential name.
        assert_eq!(redacted_headers(&["garbage".to_string()]), "garbage");
    }
}
