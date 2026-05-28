//! Per-request timeout helpers for the MCP HTTP transport.
//!
//! Background — audit 2026-05-22:
//! `vestige-mcp` had **zero** uses of `tokio::time::timeout` in the request
//! flow. A single hung handler (ONNX runtime stall, FTS5 worst-case query,
//! deadlock in a downstream library) would pin one of the 50 concurrent
//! worker slots indefinitely. After 50 such events the server stops
//! accepting work without ever crashing — which is the worst failure mode.
//!
//! We don't currently differentiate per-tool budgets. `consolidate`, `dream`,
//! and `backup` are the only tools that can legitimately run for minutes;
//! everything else completes in <5 s. Setting a single generous ceiling
//! (5 min) defends against the runaway case without breaking valid long
//! tools. When per-tool budgets become necessary (e.g. when search SLO
//! tightens to <1 s), the callsite already passes a `Duration`, so this
//! helper does not need to change.

use std::future::Future;
use std::time::Duration;

use tokio::time::error::Elapsed;

/// Apply a hard timeout to a request future.
///
/// On timeout the future is cancelled (its `Drop` runs, which is enough
/// for our handlers — they only hold tokio mutexes and SQLite handles, both
/// of which release cleanly on drop).
///
/// Returns `Ok(value)` on success, `Err(Elapsed)` if the budget was
/// exhausted. The HTTP layer maps the latter to a 504 Gateway Timeout with
/// a structured error.
pub async fn with_timeout<F, T>(budget: Duration, fut: F) -> Result<T, Elapsed>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(budget, fut).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn returns_value_when_future_completes_inside_budget() {
        let result = with_timeout(Duration::from_secs(5), async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test(start_paused = true)]
    async fn returns_elapsed_when_future_exceeds_budget() {
        let result = with_timeout(Duration::from_millis(50), async {
            sleep(Duration::from_secs(60)).await;
            "never"
        })
        .await;
        assert!(result.is_err(), "expected timeout, got {:?}", result);
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_future_drops_promptly() {
        // Detect cancellation via a Drop side-effect — if the timeout
        // *doesn't* cancel the inner future, this counter stays at 0.
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct DropMarker(Arc<AtomicUsize>);
        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let marker = DropMarker(Arc::clone(&counter));

        let _ = with_timeout(Duration::from_millis(10), async move {
            let _hold = marker; // capture into the future
            sleep(Duration::from_secs(60)).await;
        })
        .await;

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "cancelled future must drop its captured state"
        );
    }
}
