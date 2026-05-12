//! Dashboard handlers — static page redirects
//!
//! Split out of the monolithic `handlers.rs`. See `mod.rs` for shared
//! helpers like `log_err`.

use axum::response::Redirect;

/// Redirect root to the React dashboard
pub async fn serve_dashboard() -> Redirect {
    Redirect::permanent("/dashboard")
}

/// Redirect legacy graph to dashboard graph page
pub async fn serve_graph() -> Redirect {
    Redirect::permanent("/dashboard/graph")
}
