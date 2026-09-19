//! Telemetry / OpenTelemetry scaffolding.
//!
//! Vestige's default build emits structured logs through `tracing_subscriber::fmt`.
//! For distributed-trace export to an OTLP collector (Tempo, Jaeger, Honeycomb,
//! Datadog APM, etc.) an operator sets `VESTIGE_OTLP_ENDPOINT` (or the standard
//! `OTEL_EXPORTER_OTLP_ENDPOINT`) — and this module says what that actually does.
//!
//! What this module does today
//! ---------------------------
//! It reports the truth instead of the operator's hopes. No exporter is compiled
//! into this binary in *any* feature configuration (see
//! [`OTLP_EXPORTER_COMPILED_IN`]), so [`init`] never claims that spans were
//! shipped: an endpoint configured without an exporter is a
//! [`TelemetryStatus::NotWired`] **warning** that names the endpoint and says it
//! is not being dialed.
//!
//! The revision this replaces logged `telemetry exporter initialized`
//! (`main.rs`) and returned a status called `Initialized { endpoint }` while a
//! comment two lines above admitted the exporter wiring was still "a follow-up".
//! An operator who pointed the endpoint at a collector therefore got a green
//! startup line and an empty collector — a success report for an export that
//! never happened, which is worse than silence because it stops the
//! investigation.
//!
//! Spans are still emitted, and are useful without an exporter: every MCP tool
//! call runs inside a `mcp.tool_call` span (see `server::dispatch`) carrying the
//! tool name, a per-call id and the duration; tool failures are recorded as
//! `error` events inside that span. They land in the process log, and with no
//! exporter they go no further — which the startup line now says out loud.
//!
//! Adding the real exporter
//! ------------------------
//! It needs `opentelemetry` + `opentelemetry-otlp` + `tracing-opentelemetry`
//! (tonic/prost; roughly triples cold-build time by the old note in
//! `Cargo.toml`), so the dependency set stays a separate decision and the
//! `telemetry` feature currently pulls in nothing. When it lands: install the
//! layer inside [`init`], flip [`OTLP_EXPORTER_COMPILED_IN`], and add a success
//! variant to [`TelemetryStatus`]. `main` and the tests branch on
//! [`TelemetryStatus::level`] / [`TelemetryStatus::message`] — never on
//! `cfg!(feature)` — so nothing outside this module has to change.

use std::env;

/// Whether this binary actually ships spans to an OTLP collector.
///
/// `false` for every feature combination Vestige builds today, including
/// `--features telemetry`: that feature compiles this decision surface only and
/// adds no exporter dependency. Named (rather than an inline `false`) so the
/// exporter work has one switch to flip, and so
/// `no_status_reports_an_export_that_cannot_happen` can refuse to let someone
/// flip it without also wiring the exporter and its success message.
pub const OTLP_EXPORTER_COMPILED_IN: bool = false;

/// Severity [`TelemetryStatus::message`] deserves.
///
/// `main` maps this onto `tracing` directly, so the wording and the level can
/// never disagree about whether the operator has a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryLevel {
    /// True, but not a misconfiguration: spans exist, they just stay local.
    Info,
    /// The operator asked for export and this build cannot deliver it.
    Warn,
}

/// What [`init`] found after looking at the environment + cargo features.
/// Returned so `main` can log a single startup line about the choice — and so
/// the tests can assert on that line through the same seam `main` uses, instead
/// of parsing a global logger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryStatus {
    /// `telemetry` feature off and no endpoint configured: the default, fully
    /// local deployment. Nothing is exported, and nothing was requested.
    FeatureDisabled,
    /// `telemetry` feature on, no endpoint configured. Kept distinct from
    /// [`TelemetryStatus::FeatureDisabled`] because the operator enabled a flag
    /// and deserves to hear that this flag still wires no exporter.
    Disabled,
    /// An endpoint **is** configured, but no exporter is compiled in. The
    /// operator expects a collector to receive spans and none will: this is the
    /// case that used to log a fake success, and it must be loud.
    NotWired { endpoint: String },
}

impl TelemetryStatus {
    /// The level `main` logs [`TelemetryStatus::message`] at.
    pub fn level(&self) -> TelemetryLevel {
        match self {
            Self::FeatureDisabled | Self::Disabled => TelemetryLevel::Info,
            Self::NotWired { .. } => TelemetryLevel::Warn,
        }
    }

    /// The single startup line: what telemetry is (not) doing, in operator
    /// terms, including what to do about it.
    ///
    /// `main` logs exactly this string and the tests assert on exactly this
    /// string, so the wording an operator sees and the wording the tests pin
    /// cannot drift apart. No memory content and no secret can reach it: the
    /// only value it interpolates is the operator's own endpoint.
    pub fn message(&self) -> String {
        match self {
            Self::FeatureDisabled => "OTLP export: not available in this build — no exporter is \
                 compiled into this binary (built without `--features telemetry`, and that feature \
                 wires no exporter yet) and no endpoint is set. Tracing spans stay in the process \
                 log (RUST_LOG=info); nothing is exported."
                .to_string(),
            Self::Disabled => "OTLP export: not available in this build — no exporter is compiled \
                 into this binary (`--features telemetry` wires none yet) even though the feature \
                 is enabled, and no endpoint is configured (VESTIGE_OTLP_ENDPOINT / \
                 OTEL_EXPORTER_OTLP_ENDPOINT). Tracing spans stay in the process log; nothing is \
                 exported."
                .to_string(),
            Self::NotWired { endpoint } => format!(
                "OTLP export: endpoint {endpoint} is configured but no exporter is compiled into \
                 this binary — no spans will be exported to it. Unset VESTIGE_OTLP_ENDPOINT / \
                 OTEL_EXPORTER_OTLP_ENDPOINT to silence this, or accept log-only tracing \
                 (RUST_LOG=info)."
            ),
        }
    }
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

/// Decide what telemetry is actually running, based on `config`.
///
/// Callers call this exactly once at startup and log
/// [`TelemetryStatus::message`] at [`TelemetryStatus::level`]. No branch here
/// installs anything into the tracing stack, because there is no exporter to
/// install in any configuration: a branch whose only reachable arm was "not
/// wired" is exactly the shape that let the old code report a success that
/// could not happen. When the exporter lands it is installed here, and that is
/// where [`TelemetryStatus`] grows its success variant.
pub fn init(config: &TelemetryConfig) -> TelemetryStatus {
    match &config.endpoint {
        // An endpoint is the operator stating an intent: "spans should leave
        // this process". They will not, so warn instead of logging a success.
        Some(endpoint) => TelemetryStatus::NotWired {
            endpoint: endpoint.clone(),
        },
        None if cfg!(feature = "telemetry") => TelemetryStatus::Disabled,
        None => TelemetryStatus::FeatureDisabled,
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
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
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

    /// Substrings that would claim spans reached a collector. The audit finding
    /// this module fixes was a startup line saying exactly that while the
    /// exporter admitted it did not exist.
    const EXPORT_SUCCESS_CLAIMS: [&str; 3] = ["initialized", "exporting to", "export succeeded"];

    fn assert_no_export_claim(message: &str) {
        let lowered = message.to_lowercase();
        for claim in EXPORT_SUCCESS_CLAIMS {
            assert!(
                !lowered.contains(claim),
                "startup line reports an export that cannot happen ({claim:?}): {message}"
            );
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
        unsafe {
            env::set_var("VESTIGE_OTLP_ENDPOINT", "http://collector:4317");
        }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint.as_deref(), Some("http://collector:4317"));
    }

    #[test]
    fn from_env_falls_back_to_otel_standard_var() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe {
            env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://standard:4318");
        }
        let cfg = TelemetryConfig::from_env();
        assert_eq!(cfg.endpoint.as_deref(), Some("http://standard:4318"));
    }

    #[test]
    fn from_env_treats_empty_string_as_unset() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe {
            env::set_var("VESTIGE_OTLP_ENDPOINT", "   ");
        }
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

    /// The failure mode the audit found, pinned end to end at the seam `main`
    /// logs through: a startup with nothing configured must state that there is
    /// no exporter, not report an export.
    #[test]
    fn startup_without_endpoint_never_reports_an_export() {
        let _g = EnvGuard::snapshot_and_clear();
        let status = init(&TelemetryConfig::from_env());

        let expected = if cfg!(feature = "telemetry") {
            TelemetryStatus::Disabled
        } else {
            TelemetryStatus::FeatureDisabled
        };
        assert_eq!(status, expected);
        assert_eq!(status.level(), TelemetryLevel::Info);

        let message = status.message();
        assert!(message.contains("no exporter is compiled"), "{message}");
        assert!(message.contains("nothing is exported"), "{message}");
        assert_no_export_claim(&message);
    }

    /// The other half of the finding: `VESTIGE_OTLP_ENDPOINT` set with no
    /// exporter compiled in used to be answered with `Initialized` + an `info!`
    /// line. It must warn, and the warning must name the endpoint that is being
    /// ignored so the operator knows which setting to remove.
    #[test]
    fn configured_endpoint_without_exporter_warns_and_names_the_endpoint() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe {
            env::set_var("VESTIGE_OTLP_ENDPOINT", "http://collector.invalid:4317");
        }
        let status = init(&TelemetryConfig::from_env());

        assert_eq!(
            status,
            TelemetryStatus::NotWired {
                endpoint: "http://collector.invalid:4317".to_string()
            }
        );
        assert_eq!(status.level(), TelemetryLevel::Warn);

        let message = status.message();
        assert!(
            message.contains("http://collector.invalid:4317"),
            "{message}"
        );
        assert!(message.contains("no exporter is compiled"), "{message}");
        assert!(message.contains("no spans will be exported"), "{message}");
        assert_no_export_claim(&message);
    }

    /// The OTel-standard variable takes the same path as the
    /// vestige-namespaced one — operators with an existing collector config
    /// must not be the ones who get silence.
    #[test]
    fn standard_otel_endpoint_without_exporter_also_warns() {
        let _g = EnvGuard::snapshot_and_clear();
        unsafe {
            env::set_var(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://standard.invalid:4318",
            );
        }
        let status = init(&TelemetryConfig::from_env());

        assert_eq!(status.level(), TelemetryLevel::Warn);
        assert!(status.message().contains("http://standard.invalid:4318"));
    }

    /// Guard against re-introducing the fake success: while
    /// [`OTLP_EXPORTER_COMPILED_IN`] is false, no status this build can report
    /// may claim an export. Flipping the constant without wiring a real
    /// exporter (and giving it a success variant + message) fails here on
    /// purpose.
    #[test]
    fn no_status_reports_an_export_that_cannot_happen() {
        // Written as a branch rather than `assert!(!OTLP_EXPORTER_COMPILED_IN)`:
        // the constant is `false` today, and clippy folds that form into
        // `assert!(true)` and rejects it under `-D warnings`, which would delete
        // the guard instead of keeping it.
        if OTLP_EXPORTER_COMPILED_IN {
            panic!(
                "an exporter is now compiled in: wire it in `init`, give TelemetryStatus a success \
                 variant with an honest 'initialized' message, and update this test together with \
                 the constant"
            );
        }

        let statuses = [
            TelemetryStatus::FeatureDisabled,
            TelemetryStatus::Disabled,
            TelemetryStatus::NotWired {
                endpoint: "http://collector.invalid:4317".to_string(),
            },
        ];
        for status in &statuses {
            let message = status.message();
            assert!(
                message.contains("no exporter is compiled"),
                "{status:?} must state that no exporter exists: {message}"
            );
            assert_no_export_claim(&message);
        }
    }
}
