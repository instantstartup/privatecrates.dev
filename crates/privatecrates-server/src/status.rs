//! `GET /api/status` (docs/trust-and-status.md §2): the server's own view of its health, for the status page. Nothing
//! about any customer: no organisation names, URLs or tokens, and tenants only as a count.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;

use crate::{AppState, account::rfc3339, metrics::WINDOW};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/status", get(status))
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    let github = state.gh.metrics().summary();
    let stripe = state
        .billing
        .metrics()
        .map(|m| m.summary())
        .unwrap_or_default();
    let mut response = Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "started_at": rfc3339(Some(state.started_at)),
        "github": {
            "window_seconds": WINDOW.as_secs(),
            "requests": github.requests,
            "errors": github.errors,
            "rate_limited": github.rate_limited,
            "latency_ms_p50": github.latency_ms_p50,
            "latency_ms_p95": github.latency_ms_p95,
            "last_error_at": rfc3339(github.last_error_at),
        },
        "stripe": {
            "configured": state.billing.enabled(),
            "requests": stripe.requests,
            // Rate limits count as errors: the status page shows no more detail for Stripe.
            "errors": stripe.errors + stripe.rate_limited,
        },
        "tenants": state.tenants.all().len(),
    }))
    .into_response();
    // A probe must see the server as it is now.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
