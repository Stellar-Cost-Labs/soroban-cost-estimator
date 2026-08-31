use std::future::Future;
use std::time::Duration;

use crate::error::{AppError, AppResult};

const MAX_RETRIES: usize = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Executes an async operation with a limited number of retries.
pub async fn with_retry<F, Fut, T>(mut operation: F) -> AppResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = AppResult<T>>,
{
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempts >= MAX_RETRIES {
                    return Err(error);
                }

                attempts += 1;

                if is_retryable(&error) {
                    tokio::time::sleep(RETRY_DELAY).await;
                } else {
                    return Err(error);
                }
            }
        }
    }
}

fn is_retryable(error: &AppError) -> bool {
    matches!(error, AppError::Http(_))
}
