use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{AppError, AppResult};

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
/// This covers transport-level failures ([`AppError::Http`]) and transient
/// gateway statuses ([`AppError::RpcUnavailable`] — HTTP 502/503/504).
fn is_retryable(error: &AppError) -> bool {
    matches!(error, AppError::Http(_) | AppError::RpcUnavailable { .. })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::error::{AppError, AppResult};
    use crate::rpc::retry::{is_retryable, with_retry};

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
}
