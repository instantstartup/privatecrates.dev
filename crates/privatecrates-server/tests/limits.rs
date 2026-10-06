//! GitHub's rate limits and outages: what we do when GitHub says wait, or fails for a moment.

mod common;

use common::{Crate, Harness, error_code};

const ASSETS: &str = "/releases/assets/";

/// A published crate, and a token that can read it.
async fn published(h: &Harness) -> String {
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.2.0", "acme/story-engine"),
    )
    .await;
    h.fake.add_user("alice", "ghu_", &[(repo, false)])
}

async fn download(h: &Harness, reader: &str) -> reqwest::Response {
    h.get("/api/v1/crates/story_engine/0.2.0/download", Some(reader))
        .await
}

fn asset_calls(h: &Harness) -> usize {
    h.fake.calls().iter().filter(|c| c.contains(ASSETS)).count()
}

#[tokio::test]
async fn a_short_wait_is_taken_inside_the_request() {
    let h = Harness::start().await;
    let reader = published(&h).await;
    h.fake.fail_next(ASSETS, 429, &[("retry-after", "1")], "");
    let response = download(&h, &reader).await;
    assert_eq!(response.status(), 302);
    assert_eq!(asset_calls(&h), 2);
}

#[tokio::test]
async fn a_long_wait_is_passed_to_cargo_and_github_is_left_alone() {
    let h = Harness::start().await;
    let reader = published(&h).await;
    h.fake.fail_next(
        ASSETS,
        403,
        &[("retry-after", "120"), ("x-ratelimit-remaining", "40")],
        r#"{"message":"You have exceeded a secondary rate limit."}"#,
    );
    let response = download(&h, &reader).await;
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers()["retry-after"], "120");
    assert_eq!(error_code(response).await, "github::rate_limited");
    assert_eq!(asset_calls(&h), 1);

    // Until GitHub said, nothing more is sent with that installation's token: the next caller waits too.
    let response = download(&h, &reader).await;
    assert_eq!(response.status(), 503);
    let wait: u64 = response.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((110..=120).contains(&wait), "{wait}");
    assert_eq!(asset_calls(&h), 1);
}

#[tokio::test]
async fn a_secondary_limit_without_a_time_waits_a_minute() {
    let h = Harness::start().await;
    let reader = published(&h).await;
    h.fake.fail_next(
        ASSETS,
        403,
        &[],
        r#"{"message":"You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}"#,
    );
    let response = download(&h, &reader).await;
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers()["retry-after"], "60");
}

#[tokio::test]
async fn a_permission_refusal_is_not_a_rate_limit() {
    let h = Harness::start().await;
    let reader = published(&h).await;
    h.fake.fail_next(
        ASSETS,
        403,
        &[],
        r#"{"message":"Resource not accessible by integration"}"#,
    );
    let response = download(&h, &reader).await;
    assert_ne!(response.status(), 503);
    assert!(response.headers().get("retry-after").is_none());
    // And nothing was paused: the next download works.
    assert_eq!(download(&h, &reader).await.status(), 302);
}

#[tokio::test]
async fn a_passing_github_outage_is_retried() {
    let h = Harness::start().await;
    let reader = published(&h).await;
    h.fake.fail_next(ASSETS, 502, &[], "Bad gateway");
    h.fake.fail_next(ASSETS, 504, &[], "Gateway timeout");
    let response = download(&h, &reader).await;
    assert_eq!(response.status(), 302);
    assert_eq!(asset_calls(&h), 3);
}
