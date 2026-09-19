//! Environment-variable guards for tests that must flip process-wide switches.
//!
//! `std::env::set_var` is `unsafe` in Rust 2024 because it races with any
//! concurrent reader in the same process. These guards serialize such tests
//! behind one lock and always restore the previous values on drop (even on
//! panic), mirroring the pattern used by `crates/vestige-mcp/src/telemetry.rs`.
//!
//! ## Rules
//!
//! * Create **exactly one** guard per test — the lock is not re-entrant, and a
//!   second guard in the same thread panics with an explanatory message rather
//!   than deadlocking (which is what it did before this check existed).
//! * Need several variables? Use [`EnvGuard::set_many`].

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard, OnceLock};

thread_local! {
    /// Whether *this* thread currently holds the environment lock.
    static HOLDING: Cell<bool> = const { Cell::new(false) };
}

/// Process-wide lock held for as long as any [`EnvGuard`] is alive.
fn env_lock() -> MutexGuard<'static, ()> {
    assert!(
        !HOLDING.with(Cell::get),
        "EnvGuard: this thread already holds the environment lock — create exactly ONE guard \
         per test and pass every variable to EnvGuard::set_many (the lock is not re-entrant)"
    );

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    HOLDING.with(|holding| holding.set(true));
    guard
}

/// Sets one or more environment variables for the lifetime of the guard.
pub struct EnvGuard {
    previous: Vec<(&'static str, Option<String>)>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Set `name=value`, restoring the previous state on drop.
    #[must_use]
    pub fn set(name: &'static str, value: String) -> Self {
        Self::set_many(vec![(name, value)])
    }

    /// Set several variables under one lock, restoring all of them on drop.
    #[must_use]
    pub fn set_many(vars: Vec<(&'static str, String)>) -> Self {
        let lock = env_lock();
        let mut previous = Vec::with_capacity(vars.len());
        for (name, value) in vars {
            previous.push((name, std::env::var(name).ok()));
            // SAFETY: serialized by `env_lock()`; restored in `Drop`.
            unsafe { std::env::set_var(name, value) };
        }
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: the lock taken in `set_many` is still held here.
        unsafe {
            for (name, value) in &self.previous {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        HOLDING.with(|holding| holding.set(false));
    }
}

/// Enable the deterministic, offline embedder (`VESTIGE_TEST_MOCK_EMBEDDINGS`).
///
/// Any test that reaches `Storage`'s embedding path must hold one of these:
/// without it `EmbeddingService::is_ready()` lazily initializes the real ONNX
/// model, which would download ~547 MB on a cold cache. With it, embeddings are
/// a pure hash of the text — deterministic, instant and network-free — so the
/// semantic path (embeddings → HNSW → RRF) is genuinely exercised.
#[must_use]
pub fn enable_mock_embeddings() -> EnvGuard {
    EnvGuard::set(
        vestige_core::embeddings::MOCK_EMBEDDINGS_ENV,
        "1".to_string(),
    )
}

/// Enable the mock embedder *and* raise `VESTIGE_RETENTION_TARGET` above any
/// achievable average retention, so consolidation's "store is below the
/// retention target" branch is guaranteed to be taken. Used by the regression
/// test that pins "consolidation never deletes".
#[must_use]
pub fn enable_mock_embeddings_below_retention_target(target: f64) -> EnvGuard {
    EnvGuard::set_many(vec![
        (
            vestige_core::embeddings::MOCK_EMBEDDINGS_ENV,
            "1".to_string(),
        ),
        ("VESTIGE_RETENTION_TARGET", target.to_string()),
    ])
}
