//! Origin/Host validation shared by the MCP transport and the dashboard.
//!
//! Both servers bind to loopback, and both were protected only by CORS — which sets response
//! headers and never rejects a request. A page on any website could therefore POST to
//! `127.0.0.1:3927` (or `:3928`) and the request would execute; the browser merely withheld
//! the reply. The MCP transport spec makes Origin validation a MUST for exactly this reason,
//! and the same reasoning applies to the dashboard, which can read, edit and delete every
//! memory.
//!
//! Extracted into one module because two copies of a security check drift: the transport got
//! the guard first and the dashboard kept the hole.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Origins a browser may call from: loopback plus anything in `VESTIGE_CORS_ORIGINS`.
///
/// `extra` lets a caller add build-time origins (the Vite dev server in debug builds).
pub fn allowed_origins(port: u16, extra: &[String]) -> Vec<String> {
    let mut origins = vec![
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://[::1]:{port}"),
    ];
    origins.extend(extra.iter().cloned());
    if let Ok(env) = std::env::var("VESTIGE_CORS_ORIGINS") {
        origins.extend(
            env.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    origins
}

/// `Host` values the server answers to: loopback plus `VESTIGE_ALLOWED_HOSTS`.
///
/// A request whose `Host` is neither is the DNS-rebinding shape — the browser was tricked
/// into resolving an attacker-controlled name to 127.0.0.1, so the name in the header is not
/// ours even though the connection is.
pub fn allowed_hosts(port: u16) -> Vec<String> {
    let mut hosts = vec![
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ];
    if let Ok(env) = std::env::var("VESTIGE_ALLOWED_HOSTS") {
        hosts.extend(
            env.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    hosts
}

/// The decision for one request: allowed, or rejected with a reason worth logging.
pub enum Decision {
    /// Carry on.
    Allow,
    /// Refuse; the payload names what was wrong.
    Reject(&'static str),
}

/// Apply the Origin/Host policy to a set of request headers.
///
/// An absent `Origin` is allowed: non-browser clients do not send one, and DNS rebinding is a
/// browser attack. `Host` is checked on every request.
pub fn evaluate(origins: &[String], hosts: &[String], headers: &HeaderMap) -> Decision {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let host_allowed = host
        .map(|h| hosts.iter().any(|allowed| allowed.eq_ignore_ascii_case(h)))
        .unwrap_or(false);
    if !host_allowed {
        return Decision::Reject("Host header is missing or not allowed");
    }

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let origin_allowed = origins
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(origin));
        if !origin_allowed {
            return Decision::Reject("Origin header is not allowed");
        }
    }

    Decision::Allow
}

/// 403 with a JSON error body, as the transport spec prescribes for a rejected Origin.
pub fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": message,
            "code": -32600,
        })),
    )
        .into_response()
}

/// Shared state for [`check`]: the two allowlists, resolved once at startup.
#[derive(Clone)]
pub struct OriginGuard {
    origins: Arc<Vec<String>>,
    hosts: Arc<Vec<String>>,
    /// Name used in log lines, so an operator can tell which listener refused.
    listener: &'static str,
}

impl OriginGuard {
    /// Build a guard for a listener on `port`.
    pub fn for_port(listener: &'static str, port: u16, extra_origins: &[String]) -> Self {
        Self {
            origins: Arc::new(allowed_origins(port, extra_origins)),
            hosts: Arc::new(allowed_hosts(port)),
            listener,
        }
    }

    /// Axum middleware: refuse a request a browser should never be able to make.
    pub async fn check(State(guard): State<Self>, request: Request, next: Next) -> Response {
        match evaluate(&guard.origins, &guard.hosts, request.headers()) {
            Decision::Allow => next.run(request).await,
            Decision::Reject(reason) => {
                tracing::warn!(
                    listener = guard.listener,
                    origin = request
                        .headers()
                        .get(header::ORIGIN)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<none>"),
                    host = request
                        .headers()
                        .get(header::HOST)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<none>"),
                    reason,
                    "rejected request: {}",
                    reason
                );
                forbidden(reason)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(host: &str, origin: Option<&str>) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            map.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        map
    }

    #[test]
    fn defaults_are_loopback_only() {
        let origins = allowed_origins(3927, &[]);
        let hosts = allowed_hosts(3927);

        assert!(origins.contains(&"http://127.0.0.1:3927".to_string()));
        assert!(origins.contains(&"http://localhost:3927".to_string()));
        assert!(!origins.iter().any(|o| o.contains("evil")));
        assert_eq!(
            hosts.len(),
            3,
            "loopback v4, v6 and localhost — nothing else"
        );
    }

    #[test]
    fn extra_origins_are_honoured_for_the_dev_server() {
        let origins = allowed_origins(3927, &["http://localhost:5173".to_string()]);
        assert!(origins.contains(&"http://localhost:5173".to_string()));

        let decision = evaluate(
            &origins,
            &allowed_hosts(3927),
            &headers("127.0.0.1:3927", Some("http://localhost:5173")),
        );
        assert!(matches!(decision, Decision::Allow));
    }

    #[test]
    fn foreign_origin_and_host_are_rejected() {
        let origins = allowed_origins(3927, &[]);
        let hosts = allowed_hosts(3927);

        assert!(matches!(
            evaluate(
                &origins,
                &hosts,
                &headers("127.0.0.1:3927", Some("https://evil.example"))
            ),
            Decision::Reject(_)
        ));
        assert!(matches!(
            evaluate(&origins, &hosts, &headers("attacker.example:3927", None)),
            Decision::Reject(_)
        ));
    }

    #[test]
    fn missing_origin_is_allowed_but_missing_host_is_not() {
        let origins = allowed_origins(3927, &[]);
        let hosts = allowed_hosts(3927);

        assert!(matches!(
            evaluate(&origins, &hosts, &headers("localhost:3927", None)),
            Decision::Allow
        ));
        assert!(matches!(
            evaluate(&origins, &hosts, &HeaderMap::new()),
            Decision::Reject("Host header is missing or not allowed")
        ));
    }
}
