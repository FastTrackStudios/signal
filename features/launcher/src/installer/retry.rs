//! Generic async retry with exponential backoff.

use std::time::Duration;

use tracing::warn;

/// Retry an async operation up to `max_attempts` times with exponential backoff.
///
/// Delays between attempts: `base_delay`, `base_delay * 2`, `base_delay * 4`, ...
/// The final attempt returns the error directly (no sleep after).
pub async fn with_retry<F, Fut, T>(
    label: &str,
    max_attempts: u32,
    base_delay: Duration,
    f: F,
) -> eyre::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = eyre::Result<T>>,
{
    let mut delay = base_delay;
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt == max_attempts {
                    return Err(e);
                }
                warn!(
                    "{label}: attempt {attempt}/{max_attempts} failed: {e:#}, retrying in {delay:?}"
                );
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
    }
    unreachable!()
}
