use std::future::Future;
use std::time::Duration;

use crate::error::{AppError, AppResult};

/// Default number of retries performed on transient RPC failures when the
/// caller does not specify a value.
pub const DEFAULT_MAX_RETRIES: usize = 3;

/// Base delay applied before the first retry. Each successive retry doubles
/// this value (exponential backoff): 500ms, 1s, 2s, ... for the default.
const BASE_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Executes an async operation, retrying transient failures up to
/// `max_retries` times with exponential backoff.
///
/// A `max_retries` of `0` effectively disables retries: the operation runs
/// once and any failure is returned immediately. Each retry waits
/// `500ms * 2^(attempt-1)` before the next attempt — 500ms, then 1s, then
/// 2s, etc. — so transient failures back off progressively rather than
/// hammering the endpoint.
///
/// Only retryable errors (see [`is_retryable`]) trigger a retry; permanent
/// errors are returned immediately.
pub async fn with_retry<F, Fut, T>(max_retries: usize, mut operation: F) -> AppResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    // `attempts` counts retries already performed. The initial attempt runs
    // before any check, so the operation executes once plus up to
    // `max_retries` extra times.
    let mut attempts = 0;
    let mut next_delay = BASE_RETRY_DELAY;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempts >= max_retries || !is_retryable(&error) {
                    return Err(error);
                }

                attempts += 1;
                tokio::time::sleep(next_delay).await;
                next_delay = next_delay.saturating_mul(2);
            }
        }
    }
}

/// Returns `true` when `error` represents a transient failure worth retrying.
fn is_retryable(error: &AppError) -> bool {
    matches!(error, AppError::Http(_))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::error::{AppError, AppResult};
    use crate::rpc::retry::with_retry;

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
}
