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

/// Before an organisation runs out of GitHub allowance, the people who would notice are told: in `cargo publish`,
/// and on the account page.
#[tokio::test]
async fn a_low_allowance_is_warned_about_before_it_runs_out() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let first = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    h.publish_from_ci("acme/story-engine", repo, &first).await;

    // Plenty left: no warning.
    h.fake.set_allowance(4_000, 5_000);
    let second = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &second);
    let body: serde_json::Value = h.publish(&second, &token).await.json().await.unwrap();
    assert_eq!(body["warnings"]["other"], serde_json::json!([]));

    // 400 of 5,000 left: 92% used.
    h.fake.set_allowance(400, 5_000);
    let third = Crate::new("story_engine", "0.3.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &third);
    let body: serde_json::Value = h.publish(&third, &token).await.json().await.unwrap();
    let warning = body["warnings"]["other"][0].as_str().unwrap().to_owned();
    assert!(
        warning.starts_with(
            "acme has used 92% of its hourly GitHub API allowance (400 of 5000 calls left until"
        ),
        "{warning}"
    );
    assert!(warning.contains(&h.apex("/docs/setup#limits")), "{warning}");

    // The account page says the same.
    let admin = h.fake.add_user("ada", "ghu_", &[(repo, true)]);
    h.fake.add_member(&admin, &h.org, "admin");
    let session = h.sign_in(&admin).await;
    let doc: serde_json::Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    let allowance = &doc["orgs"][0]["tenant"]["github_allowance"];
    assert_eq!(allowance["remaining"], 400);
    assert_eq!(allowance["limit"], 5_000);
    assert_eq!(allowance["running_low"], true);
}

/// A developer who can read hundreds of repositories: every page of them is fetched, a few at a time, so a crate
/// owned by the last repository is as readable as one owned by the first.
#[tokio::test]
async fn a_developer_with_many_repositories_reads_crates_on_every_page() {
    let h = Harness::start().await;
    let repos: Vec<u64> = (0..250)
        .map(|i| h.repo(&format!("service-{i:03}")))
        .collect();
    let last = *repos.last().unwrap();
    h.publish_from_ci(
        "acme/service-249",
        last,
        &Crate::new("service_249", "0.1.0", "acme/service-249"),
    )
    .await;
    let access: Vec<(u64, bool)> = repos.iter().map(|&id| (id, false)).collect();
    let developer = h.fake.add_user("dev", "ghu_", &access);
    h.fake.clear_calls();
    let response = h.get("/index/se/rv/service_249", Some(&developer)).await;
    assert_eq!(response.status(), 200);
    let pages = h
        .fake
        .calls()
        .iter()
        .filter(|c| c.contains("/user/installations/"))
        .count();
    assert_eq!(pages, 3);
}
