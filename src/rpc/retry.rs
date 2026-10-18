use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{AppError, AppResult};

/// State of an endpoint's circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation: requests flow to the endpoint.
    Closed,
    /// Tripped after repeated transport/gateway failures: requests fail fast
    /// with [`AppError::CircuitOpen`] until the cooldown elapses.
    Open,
    /// Cooldown has elapsed: a single recovery probe request is permitted.
    HalfOpen,
}

/// Configuration for optional endpoint circuit breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive transport/gateway failures required to trip the
    /// circuit open. Values of `0` are treated as `1`.
    pub failure_threshold: u32,
    /// Cooldown duration an open circuit waits before allowing a half-open
    /// recovery probe.
    pub cooldown: Duration,
}

impl CircuitBreakerConfig {
    /// Creates a new circuit breaker configuration.
    #[must_use]
    pub const fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_threshold,
            cooldown,
        }
    }
}

/// Per-endpoint circuit breaker tracking consecutive transport failures and
/// recovery probes.
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    half_open_probe_in_flight: bool,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("config", &self.config)
            .field("consecutive_failures", &self.consecutive_failures)
            .field("opened_at", &self.opened_at)
            .field("half_open_probe_in_flight", &self.half_open_probe_in_flight)
            .finish_non_exhaustive()
    }
}

impl CircuitBreaker {
    /// Creates a circuit breaker using [`Instant::now`] as the clock source.
    #[must_use]
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self::with_clock(config, Instant::now)
    }

    /// Creates a circuit breaker with a custom monotonic clock (used by unit
    /// tests to advance time deterministically).
    pub fn with_clock<C>(config: CircuitBreakerConfig, clock: C) -> Self
    where
        C: Fn() -> Instant + Send + Sync + 'static,
    {
        Self {
            config,
            consecutive_failures: 0,
            opened_at: None,
            half_open_probe_in_flight: false,
            clock: Arc::new(clock),
        }
    }

    /// Returns the current circuit state without mutating probe state.
    #[must_use]
    pub fn state(&self) -> CircuitState {
        match self.opened_at {
            None => CircuitState::Closed,
            Some(opened) => {
                if self.half_open_probe_in_flight
                    || (self.clock)().saturating_duration_since(opened) >= self.config.cooldown
                {
                    CircuitState::HalfOpen
                } else {
                    CircuitState::Open
                }
            }
        }
    }

    /// Checks whether a request to `endpoint` may proceed.
    ///
    /// Transitions from `Open` to `HalfOpen` once the cooldown has elapsed and
    /// admits a single probe request. Concurrent requests while a probe is in
    /// flight (or before the cooldown expires) fail fast with
    /// [`AppError::CircuitOpen`].
    pub fn allow_request(&mut self, endpoint: &str) -> AppResult<CircuitState> {
        let Some(opened) = self.opened_at else {
            return Ok(CircuitState::Closed);
        };

        let elapsed = (self.clock)().saturating_duration_since(opened);
        if elapsed >= self.config.cooldown && !self.half_open_probe_in_flight {
            self.half_open_probe_in_flight = true;
            tracing::debug!(
                endpoint,
                state = ?CircuitState::HalfOpen,
                "RPC circuit breaker entering half-open recovery probe"
            );
            return Ok(CircuitState::HalfOpen);
        }

        tracing::debug!(
            endpoint,
            state = ?CircuitState::Open,
            "RPC circuit breaker is open; skipping endpoint"
        );
        Err(AppError::CircuitOpen {
            endpoint: endpoint.to_string(),
        })
    }

    /// Records a successful response from `endpoint`, closing the circuit and
    /// resetting failure counters.
    pub fn record_success(&mut self, endpoint: &str) {
        let was_unhealthy = self.opened_at.is_some() || self.consecutive_failures > 0;
        self.consecutive_failures = 0;
        self.opened_at = None;
        self.half_open_probe_in_flight = false;
        if was_unhealthy {
            tracing::debug!(
                endpoint,
                state = ?CircuitState::Closed,
                "RPC circuit breaker closed after healthy response"
            );
        }
    }

    /// Records an error from `endpoint`.
    ///
    /// Only transport and gateway failures count toward opening the circuit;
    /// application-level RPC or simulation errors never trip the circuit, and
    /// if a half-open probe receives a valid HTTP JSON-RPC error response, the
    /// transport is considered recovered and the circuit closes.
    pub fn record_error(&mut self, endpoint: &str, error: &AppError) {
        if is_circuit_failure(error) {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            let was_probe = self.half_open_probe_in_flight;
            self.half_open_probe_in_flight = false;
            let threshold = self.config.failure_threshold.max(1);
            if was_probe || self.consecutive_failures >= threshold {
                self.opened_at = Some((self.clock)());
                tracing::debug!(
                    endpoint,
                    state = ?CircuitState::Open,
                    consecutive_failures = self.consecutive_failures,
                    "RPC circuit breaker opened"
                );
            }
        } else if self.half_open_probe_in_flight {
            // An application-level JSON-RPC response proves the endpoint's
            // transport recovered.
            self.record_success(endpoint);
        }
    }
}

/// Returns `true` when `error` represents an endpoint transport or gateway
/// failure that should count toward tripping the circuit breaker.
fn is_circuit_failure(error: &AppError) -> bool {
    match error {
        AppError::Http(e) => e.is_connect() || e.is_timeout() || e.is_request(),
        AppError::ConnectTimeout { .. } | AppError::RpcUnavailable { .. } => true,
        AppError::HttpStatus { status, .. } => matches!(status, 500 | 502 | 503 | 504),
        _ => false,
    }
}

/// Default number of retries performed on transient RPC failures when the
/// caller does not specify a value.
pub const DEFAULT_MAX_RETRIES: usize = 3;

/// Base delay applied before the first retry. Each successive retry doubles
/// this value (exponential backoff): 500ms, 1s, 2s, ... for the default,
/// plus a random jitter offset drawn from the same range.
const BASE_RETRY_DELAY: Duration = Duration::from_millis(500);

/// HTTP statuses that indicate a transient failure worth retrying: rate
/// limiting (429) and server-side errors (500/502/503/504). Deterministic
/// client errors such as 400 are never retried.
const TRANSIENT_STATUSES: [u16; 5] = [429, 500, 502, 503, 504];

/// Executes an async operation, retrying transient failures up to
/// `max_retries` times with exponential backoff and full jitter.
///
/// A `max_retries` of `0` effectively disables retries: the operation runs
/// once and any failure is returned immediately. Each retry waits
/// `base * 2^attempt` (base 500ms) plus a random offset of up to the same
/// amount — 500–1000ms, then 1000–2000ms, then 2000–4000ms, etc. — so
/// transient failures back off progressively without synchronizing retries
/// across concurrent callers.
///
/// Only retryable errors (see [`is_retryable`]) trigger a retry; permanent
/// errors, including deterministic client errors and invalid XDR, are
/// returned immediately. When the failing error carries a `Retry-After`
/// hint (HTTP 429), that value is honored in place of the computed backoff.
///
/// Retry attempts are logged to stderr when debug logging is enabled (i.e.
/// when the CLI is run with `--verbose`).
pub async fn with_retry<F, Fut, T>(max_retries: usize, mut operation: F) -> AppResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    // `attempts` counts retries already performed. The initial attempt runs
    // before any check, so the operation executes once plus up to
    // `max_retries` extra times.
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempts >= max_retries || !is_retryable(&error) {
                    return Err(error);
                }

                let delay = retry_delay(&error, attempts);
                log_retry(&error, attempts + 1, max_retries, delay);
                tokio::time::sleep(delay).await;
                attempts += 1;
            }
        }
    }
}

/// Returns `true` when `error` represents a transient failure worth retrying.
///
/// Retryable transport failures are connection/timeout/request errors; an
/// HTTP status error is retryable only for the statuses in
/// [`TRANSIENT_STATUSES`]; transient gateway statuses
/// ([`AppError::RpcUnavailable`]) and connect timeouts are also retryable.
/// Everything else — RPC-level errors, XDR decode failures, deterministic
/// 4xx client errors — is deterministic and returned as-is.
fn is_retryable(error: &AppError) -> bool {
    match error {
        AppError::Http(e) => e.is_connect() || e.is_timeout() || e.is_request(),
        AppError::HttpStatus { status, .. } => TRANSIENT_STATUSES.contains(status),
        AppError::RpcUnavailable { .. } | AppError::ConnectTimeout { .. } => true,
        _ => false,
    }
}

/// Computes the delay before the retry following `attempt` (0-based).
///
/// A `Retry-After` hint, when present, is authoritative. Otherwise the delay
/// is the exponential backoff `500ms * 2^attempt` plus a random jitter offset
/// of up to the same magnitude.
fn retry_delay(error: &AppError, attempt: usize) -> Duration {
    if let AppError::HttpStatus {
        retry_after: Some(delay),
        ..
    } = error
    {
        return *delay;
    }

    let exponential = BASE_RETRY_DELAY.saturating_mul(1u32 << attempt.min(16));
    exponential.saturating_add(random_below(exponential))
}

/// Returns a pseudo-random `Duration` in `[0, max)`.
///
/// Uses the current wall-clock sub-second component as entropy so the retry
/// loop needs no RNG dependency; the jitter only has to decorrelate retries,
/// not be cryptographically random.
fn random_below(max: Duration) -> Duration {
    let nanos = max.as_nanos();
    if nanos == 0 {
        return Duration::ZERO;
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    let jitter_nanos = (u128::from(seed) * nanos / 1_000_000_000).min(u128::from(u64::MAX));
    Duration::from_nanos(jitter_nanos as u64)
}

/// Logs a retry attempt to stderr when debug logging is enabled.
///
/// Debug logging is turned on by the CLI's `--verbose` flag, so this stays
/// silent in normal runs while giving operators visibility into retry
/// behavior (and how long each attempt will wait) when they ask for it.
fn log_retry(error: &AppError, attempt: usize, max_retries: usize, delay: Duration) {
    if tracing::enabled!(tracing::Level::DEBUG) {
        eprintln!(
            "RPC request failed ({error}); retrying in {delay:?} (attempt {attempt}/{max_retries})"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::error::{AppError, AppResult};
    use crate::rpc::retry::{BASE_RETRY_DELAY, is_retryable, retry_delay, with_retry};

    /// Returns the URL of an ephemeral `127.0.0.1` port that is guaranteed
    /// closed: a listener is bound to it, its address captured, then the
    /// listener is dropped. Connecting yields a deterministic
    /// connection-refused transport error.
    async fn closed_port_url() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local address");
        drop(listener);
        format!("http://{addr}")
    }

    /// Produces a genuine transient `AppError::Http` by attempting to connect
    /// to a port that refuses connections.
    async fn transient_error(url: &str) -> AppError {
        let err = reqwest::get(url).await.expect_err("connection must refuse");
        AppError::from(err)
    }

    /// A transient failure must be retried `max_retries` times before the
    /// final failure surfaces: one initial attempt plus `max_retries` retries.
    #[tokio::test]
    async fn retries_transient_failure_until_exhausted() {
        let url = closed_port_url().await;
        let attempts = AtomicUsize::new(0);
        let result: AppResult<()> = with_retry(3, || {
            let url = url.clone();
            let attempts = &attempts;
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(transient_error(&url).await)
            }
        })
        .await;

        assert!(result.is_err(), "exhausted retries must still fail");
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    /// A success on a later attempt must be returned and stop the retry loop.
    #[tokio::test]
    async fn recovers_after_transient_failures() {
        let url = closed_port_url().await;
        let attempts = AtomicUsize::new(0);
        let result: AppResult<&'static str> = with_retry(5, || {
            let url = url.clone();
            let attempts = &attempts;
            async move {
                if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(transient_error(&url).await)
                } else {
                    Ok("recovered")
                }
            }
        })
        .await;

        assert_eq!(result.expect("must recover"), "recovered");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    /// A `max_retries` of zero runs the operation once and surfaces any
    /// failure immediately, without retrying.
    #[tokio::test]
    async fn zero_max_retries_runs_once() {
        let url = closed_port_url().await;
        let attempts = AtomicUsize::new(0);
        let result: AppResult<()> = with_retry(0, || {
            let url = url.clone();
            let attempts = &attempts;
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(transient_error(&url).await)
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    /// Transient gateway statuses (HTTP 502/503/504) are retryable, while
    /// RPC-level errors returned in a successful HTTP response are not.
    #[test]
    fn gateway_status_errors_are_retryable() {
        for status in [502u16, 503, 504] {
            assert!(is_retryable(&AppError::RpcUnavailable { status }));
        }
        assert!(!is_retryable(&AppError::Rpc {
            status: -32000,
            message: "permanent".to_string(),
        }));
    }

    /// Errors that are not transient must never be retried.
    #[tokio::test]
    async fn non_retryable_error_returns_immediately() {
        let attempts = AtomicUsize::new(0);
        let result: AppResult<()> = with_retry(3, || {
            let attempts = &attempts;
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(AppError::Rpc {
                    status: -32000,
                    message: "permanent".to_string(),
                })
            }
        })
        .await;

        assert!(matches!(result, Err(AppError::Rpc { .. })));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn transient_statuses_are_retryable() {
        for status in [429u16, 500, 502, 503, 504] {
            let error = AppError::HttpStatus {
                status,
                retry_after: None,
                message: String::new(),
            };
            assert!(is_retryable(&error), "{status} must be retryable");
        }
    }

    #[test]
    fn deterministic_client_statuses_are_not_retryable() {
        for status in [400u16, 401, 403, 404, 422] {
            let error = AppError::HttpStatus {
                status,
                retry_after: None,
                message: String::new(),
            };
            assert!(!is_retryable(&error), "{status} must not be retryable");
        }
    }

    #[test]
    fn rpc_and_xdr_errors_are_not_retryable() {
        assert!(!is_retryable(&AppError::Rpc {
            status: -1,
            message: "nope".to_string(),
        }));
        assert!(!is_retryable(&AppError::XdrDecode("bad".to_string())));
    }

    /// `Retry-After` overrides the computed exponential backoff entirely.
    #[test]
    fn retry_after_hint_overrides_backoff() {
        let error = AppError::HttpStatus {
            status: 429,
            retry_after: Some(Duration::from_secs(7)),
            message: String::new(),
        };
        assert_eq!(retry_delay(&error, 2), Duration::from_secs(7));
    }

    /// Without a hint, the delay is `base * 2^attempt` plus jitter of up to
    /// the same magnitude, so it always lands in
    /// `[exponential, 2 * exponential)`.
    #[test]
    fn backoff_is_exponential_with_bounded_jitter() {
        for attempt in 0..4usize {
            let exponential = BASE_RETRY_DELAY.saturating_mul(1u32 << attempt);
            let error = AppError::HttpStatus {
                status: 503,
                retry_after: None,
                message: String::new(),
            };
            for _ in 0..25 {
                let delay = retry_delay(&error, attempt);
                assert!(
                    delay >= exponential && delay < exponential.saturating_mul(2),
                    "attempt {attempt}: delay {delay:?} outside [{exponential:?}, {:?})",
                    exponential.saturating_mul(2)
                );
            }
        }
    }

    #[test]
    fn circuit_breaker_transitions_closed_open_half_open_closed_with_injected_clock() {
        use super::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let base = Instant::now();
        let offset = Arc::new(Mutex::new(Duration::ZERO));
        let clock_offset = Arc::clone(&offset);
        let mut breaker = CircuitBreaker::with_clock(
            CircuitBreakerConfig::new(2, Duration::from_secs(30)),
            move || base + *clock_offset.lock().expect("clock lock"),
        );

        let endpoint = "http://primary.local:8080";
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(
            breaker.allow_request(endpoint).expect("closed admits"),
            CircuitState::Closed
        );

        // First gateway failure keeps circuit closed (threshold = 2).
        breaker.record_error(endpoint, &AppError::RpcUnavailable { status: 503 });
        assert_eq!(breaker.state(), CircuitState::Closed);

        // Application-level RPC errors never count toward opening the circuit.
        breaker.record_error(
            endpoint,
            &AppError::Rpc {
                status: -32602,
                message: "invalid params".to_string(),
            },
        );
        assert_eq!(breaker.state(), CircuitState::Closed);

        // Second gateway failure reaches threshold and opens the circuit.
        breaker.record_error(endpoint, &AppError::RpcUnavailable { status: 502 });
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(matches!(
            breaker.allow_request(endpoint),
            Err(AppError::CircuitOpen { .. })
        ));

        // Before cooldown expires (29s < 30s), requests still fail fast.
        *offset.lock().expect("clock lock") = Duration::from_secs(29);
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(matches!(
            breaker.allow_request(endpoint),
            Err(AppError::CircuitOpen { .. })
        ));

        // Once cooldown elapses (30s), circuit reports HalfOpen and admits one probe.
        *offset.lock().expect("clock lock") = Duration::from_secs(30);
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        assert_eq!(
            breaker
                .allow_request(endpoint)
                .expect("half-open admits probe"),
            CircuitState::HalfOpen
        );
        // While probe is in flight, a second concurrent caller is rejected.
        assert!(matches!(
            breaker.allow_request(endpoint),
            Err(AppError::CircuitOpen { .. })
        ));

        // If the half-open probe fails with a gateway error, circuit re-opens immediately.
        breaker.record_error(endpoint, &AppError::RpcUnavailable { status: 504 });
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(matches!(
            breaker.allow_request(endpoint),
            Err(AppError::CircuitOpen { .. })
        ));

        // Advance another 30s cooldown, admit probe, and record success to close circuit.
        *offset.lock().expect("clock lock") = Duration::from_secs(60);
        assert_eq!(
            breaker
                .allow_request(endpoint)
                .expect("second probe admitted"),
            CircuitState::HalfOpen
        );
        breaker.record_success(endpoint);
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert_eq!(
            breaker.allow_request(endpoint).expect("closed admits"),
            CircuitState::Closed
        );
    }
}
