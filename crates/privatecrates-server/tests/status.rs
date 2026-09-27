//! `GET /api/status` (docs/trust-and-status.md §2): the server's own view, with nothing about any customer.

mod common;

use common::{Crate, Harness, Options};
use serde_json::Value;

#[tokio::test]
async fn the_status_shows_counts_and_nothing_about_customers() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;

    // Unauthenticated, on the apex host.
    let response = h.client.get(h.apex("/api/status")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let text = response.text().await.unwrap();
    for private in [
        "acme",
        "story",
        "crates-store",
        "ghs_",
        "ghu_",
        "localhost",
        "http",
    ] {
        assert!(!text.contains(private), "{private} in {text}");
    }
    let status: Value = serde_json::from_str(&text).unwrap();
    let keys = |v: &Value| {
        let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys(&status),
        ["github", "started_at", "stripe", "tenants", "version"]
    );
    assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));
    assert!(status["started_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(status["tenants"], 1);

    let github = &status["github"];
    assert_eq!(
        keys(github),
        [
            "errors",
            "last_error_at",
            "latency_ms_p50",
            "latency_ms_p95",
            "rate_limited",
            "requests",
            "window_seconds"
        ]
    );
    assert_eq!(github["window_seconds"], 300);
    // Discovery and the publish called GitHub; its 404s (no release yet, no draft) are answers, not errors.
    assert!(github["requests"].as_u64().unwrap() > 10, "{github}");
    assert_eq!(github["errors"], 0);
    assert_eq!(github["rate_limited"], 0);
    assert_eq!(github["last_error_at"], Value::Null);
    let p50 = github["latency_ms_p50"].as_u64().unwrap();
    assert!(p50 <= github["latency_ms_p95"].as_u64().unwrap());

    assert_eq!(
        status["stripe"],
        serde_json::json!({ "configured": false, "requests": 0, "errors": 0 })
    );

    // Registry hosts do not serve it.
    assert_eq!(h.get("/api/status", None).await.status(), 404);
}

#[tokio::test]
async fn stripe_calls_are_counted() {
    let h = Harness::start_with(Options {
        preview: false,
        stripe: true,
        ..Options::default()
    })
    .await;
    let status: Value = h
        .client
        .get(h.apex("/api/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Subscriptions were listed at start-up.
    assert_eq!(status["stripe"]["configured"], true);
    assert_eq!(status["stripe"]["requests"], 1);
    assert_eq!(status["stripe"]["errors"], 0);
}
