//! Telemetry / OpenTelemetry scaffolding.
//!
//! Vestige's default build emits structured logs through `tracing_subscriber::fmt`.
//! When you want distributed-trace export to an OTLP collector (Tempo, Jaeger,
//! Honeycomb, Datadog APM, etc.), build with `--features telemetry` and set
//! `VESTIGE_OTLP_ENDPOINT` (or the standard `OTEL_EXPORTER_OTLP_ENDPOINT`).
//!
//! Why feature-gated?
//! ------------------
//! The opentelemetry crate stack pulls in tonic + prost + reqwest by default,
//! which roughly triples cold-build time. Operators who don't need APM should
//! never pay that cost. We keep the *interface* stable here so call sites
//! (`main.rs` and the upcoming `vestige` CLI) don't branch on `cfg!(feature)`.
//!
//! The actual OTLP exporter wiring (a `tracing-opentelemetry` layer attached
//! to `tracing_subscriber::Registry`) is a follow-up once we settle the dep
//! versions; the layer code goes inside the `telemetry` cfg block of
//! [`init`] and the public surface in this module does not change.

use std::env;

/// What [`init`] decided to do after looking at the environment + cargo
/// features. Returned so `main` can log a single line about the choice — that
/// line is the operator's "did APM actually start?" signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryStatus {
    /// `telemetry` feature off at compile time. No endpoint will ever be
    /// honored in this binary. Operators who hit this should rebuild with
    /// `--features telemetry`.
    FeatureDisabled,
    /// Feature compiled in, but no endpoint configured — same behavior as
    /// `FeatureDisabled` at runtime, kept distinct so the operator sees that
    /// the binary *could* export if they set the env var.
    Disabled,
    /// Endpoint discovered and the exporter is wired into the tracing stack.
    /// `endpoint` is what we will dial out to.
    Initialized { endpoint: String },
}

/// Telemetry configuration resolved from the environment.
///
/// We prefer `VESTIGE_OTLP_ENDPOINT` (vestige-namespaced, easy to set in
/// systemd units / docker-compose) and fall back to the OTel-standard
/// `OTEL_EXPORTER_OTLP_ENDPOINT` so existing collectors keep working.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelemetryConfig {
    /// OTLP collector endpoint (e.g. `http://otel-collector:4317`).
    pub endpoint: Option<String>,
}

impl TelemetryConfig {
    /// Read configuration from process environment. Trims whitespace and
    /// treats empty strings as unset — operators who clear a value should not
    /// get a "set to empty string" surprise.
    pub fn from_env() -> Self {
        let endpoint = env::var("VESTIGE_OTLP_ENDPOINT")
            .or_else(|_| env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        Self { endpoint }
    }
}

/// Initialize OTel export based on `config`. Idempotent at the call-site
/// level: callers should call this exactly once at startup. Returns the
/// status enum so the caller can log the outcome.
///
/// When the `telemetry` feature is off, this is a no-op and returns
/// [`TelemetryStatus::FeatureDisabled`] regardless of `config`.
pub fn init(config: &TelemetryConfig) -> TelemetryStatus {
    #[cfg(not(feature = "telemetry"))]
    {
        let _ = config;
        TelemetryStatus::FeatureDisabled
    }

    #[cfg(feature = "telemetry")]
    {
        let Some(endpoint) = config.endpoint.clone() else {
            return TelemetryStatus::Disabled;
        };
        // Real OTLP exporter installation lands here. We intentionally keep
        // this branch reachable + dep-free for now so the public surface
        // can be tested and tracing-subscriber callers can switch over to
        // calling `telemetry::init` unconditionally.
        tracing::info!(
            endpoint = %endpoint,
            "telemetry feature enabled; OTLP exporter wiring is a follow-up"
        );
        TelemetryStatus::Initialized { endpoint }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Tests in this module mutate process-wide env vars and must serialize
    /// against each other. Cargo runs tests in parallel by default, so without
    /// this lock the snapshot/restore dance races with itself.
    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        // PoisonError can only happen if a previous test panicked while
        // holding the lock. The env state was already restored by `EnvGuard`'s
        // `Drop` even on unwind, so the lock content (`()`) is meaningful
        // again — recover and continue.
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Helper: snapshot + clear the env vars we look at so concurrent tests
    /// don't poison each other. We restore on drop.
    struct EnvGuard {
        vestige: Option<String>,
        otel: Option<String>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn snapshot_and_clear() -> Self {
            let lock = env_lock();
            let vestige = env::var("VESTIGE_OTLP_ENDPOINT").ok();
            let otel = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
            // SAFETY: serialized via `env_lock()` so no other Rust code in
            // this process reads/writes these vars concurrently. We always
            // restore in `Drop` even on unwind.
            unsafe {
                env::remove_var("VESTIGE_OTLP_ENDPOINT");
                env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
            }
            EnvGuard {
                vestige,
                otel,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.vestige {
                    Some(v) => env::set_var("VESTIGE_OTLP_ENDPOINT", v),
                    None => env::remove_var("VESTIGE_OTLP_ENDPOINT"),
                }
                match &self.otel {
                    Some(v) => env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", v),
                    None => env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT"),
                }
            }
        }
    }

    #[test]
    fn from_env_returns_none_when_unset() {
        let _g = EnvGuard::snapshot_and_clear();
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint, None);
    }

    #[test]
    fn from_env_reads_vestige_namespaced_var() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe { env::set_var("VESTIGE_OTLP_ENDPOINT", "http://collector:4317"); }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint.as_deref(), Some("http://collector:4317"));
    }

    #[test]
    fn from_env_falls_back_to_otel_standard_var() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe { env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://standard:4318"); }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint.as_deref(), Some("http://standard:4318"));
    }

    #[test]
    fn from_env_treats_empty_string_as_unset() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe { env::set_var("VESTIGE_OTLP_ENDPOINT", "   "); }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint, None);
    }

    #[test]
    fn vestige_var_takes_precedence_over_otel_var() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe {
            env::set_var("VESTIGE_OTLP_ENDPOINT", "http://vestige:4317");
            env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://standard:4318");
        }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint.as_deref(), Some("http://vestige:4317"));
    }

    #[test]
    #[cfg(not(feature = "telemetry"))]
    fn init_reports_feature_disabled_without_feature() {
        let cfg = TelemetryConfig {
            endpoint: Some("http://collector:4317".into()),
        };
        assert_eq!(init(&cfg), TelemetryStatus::FeatureDisabled);
    }

    #[test]
    #[cfg(feature = "telemetry")]
    fn init_reports_disabled_when_endpoint_missing() {
        let cfg = TelemetryConfig { endpoint: None };
        assert_eq!(init(&cfg), TelemetryStatus::Disabled);
    }

    #[test]
    #[cfg(feature = "telemetry")]
    fn init_reports_initialized_with_endpoint() {
        let cfg = TelemetryConfig {
            endpoint: Some("http://collector:4317".into()),
        };
        let status = init(&cfg);
        match status {
            TelemetryStatus::Initialized { endpoint } => {
                assert_eq!(endpoint, "http://collector:4317");
            }
            other => panic!("expected Initialized, got {other:?}"),
        }
    }
}
