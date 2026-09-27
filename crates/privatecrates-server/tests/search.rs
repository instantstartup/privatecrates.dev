//! Search (SPEC §9): private crates the caller can read, labelled, then crates.io's results.

mod common;

use common::{Crate, Harness};
use serde_json::Value;

async fn search(h: &Harness, query: &str, token: &str) -> Value {
    let response = h
        .get(
            &format!("/api/v1/crates?q={query}&per_page=10"),
            Some(token),
        )
        .await;
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

/// (name, max_version, description) of each result.
fn results(body: &Value) -> Vec<(String, String, String)> {
    body["crates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_owned(),
                c["max_version"].as_str().unwrap().to_owned(),
                c["description"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn names(body: &Value) -> Vec<String> {
    results(body).into_iter().map(|(name, ..)| name).collect()
}

#[tokio::test]
async fn private_results_come_first_labelled_and_filtered() {
    let h = Harness::start().await;
    let stories = h.repo("story-engine");
    let secrets = h.repo("secrets");
    h.publish_from_ci(
        "acme/story-engine",
        stories,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine")
            .described("Tell tales\nwith branching plots", &["narrative"]),
    )
    .await;
    h.publish_from_ci(
        "acme/story-engine",
        stories,
        &Crate::new("story_engine", "0.2.0", "acme/story-engine")
            .described("Tell tales", &["narrative"]),
    )
    .await;
    h.publish_from_ci(
        "acme/secrets",
        secrets,
        &Crate::new("story_secrets", "1.0.0", "acme/secrets").described("Hidden", &[]),
    )
    .await;
    h.fake.add_crates_io_crate("storybook");
    let alice = h.fake.add_user("alice", "ghu_", &[(stories, false)]);

    let body = search(&h, "story", &alice).await;
    assert_eq!(
        results(&body),
        vec![
            (
                "story_engine".into(),
                "0.2.0".into(),
                "[acme] Tell tales".into()
            ),
            (
                "storybook".into(),
                "1.0.0".into(),
                "storybook from crates.io".into()
            ),
        ]
    );
    assert_eq!(body["meta"]["total"], 2);

    // Keywords and descriptions match too; a crate she cannot read never does.
    assert_eq!(
        names(&search(&h, "narrative", &alice).await),
        ["story_engine"]
    );
    assert_eq!(names(&search(&h, "tales", &alice).await), ["story_engine"]);
    assert!(names(&search(&h, "secrets", &alice).await).is_empty());
    let bob = h
        .fake
        .add_user("bob", "ghu_", &[(stories, false), (secrets, false)]);
    assert_eq!(
        names(&search(&h, "story_", &bob).await),
        ["story_engine", "story_secrets"]
    );

    // Yanking the latest version shows the one before, with its own description.
    let yank = h
        .client
        .delete(h.url("/api/v1/crates/story_engine/0.2.0/yank"))
        .header(
            "Authorization",
            h.fake.add_user("carol", "ghu_", &[(stories, true)]),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(yank.status(), 200);
    assert_eq!(
        results(&search(&h, "story_engine", &alice).await)[0],
        (
            "story_engine".into(),
            "0.1.0".into(),
            "[acme] Tell tales with branching plots".into()
        )
    );

    // Search needs a token like everything else.
    assert_eq!(h.get("/api/v1/crates?q=story", None).await.status(), 401);
}

#[tokio::test]
async fn names_also_on_crates_io_are_flagged() {
    let h = Harness::start().await;
    h.fake.write_file(
        h.org.storage_repo,
        "privatecrates.toml",
        "slug = \"acme\"\nname_clash = \"warn\"\n",
    );
    h.state.discover().await.unwrap();
    h.fake.add_crates_io_crate("serde_story");
    let repo = h.repo("serde-story");
    h.publish_from_ci(
        "acme/serde-story",
        repo,
        &Crate::new("serde_story", "0.1.0", "acme/serde-story").described("Ours", &[]),
    )
    .await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    assert_eq!(
        results(&search(&h, "serde_story", &alice).await),
        vec![
            (
                "serde_story".into(),
                "0.1.0".into(),
                "[acme] Ours (a crate with this name also exists on crates.io)".into()
            ),
            (
                "serde_story".into(),
                "1.0.0".into(),
                "serde_story from crates.io".into()
            ),
        ]
    );
}

#[tokio::test]
async fn private_results_are_returned_alone_when_crates_io_is_down() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    h.fake.add_crates_io_crate("storybook");
    h.fake.set_crates_io_down(true);
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let body = search(&h, "story", &alice).await;
    assert_eq!(
        results(&body),
        vec![("story_engine".into(), "0.1.0".into(), "[acme]".into())]
    );
    assert_eq!(body["meta"]["total"], 1);
}

#[tokio::test]
async fn the_index_is_built_with_the_storage_app_and_cached() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine").described("Tales", &[]),
    )
    .await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    search(&h, "story", &alice).await;
    let asset_downloads = |calls: &[String]| {
        calls
            .iter()
            .filter(|c| c.starts_with("GET /signed/"))
            .count()
    };
    assert_eq!(asset_downloads(&h.fake.calls()), 1);

    // Later searches cost no GitHub calls at all while the repository set is cached.
    h.fake.clear_calls();
    search(&h, "tales", &alice).await;
    search(&h, "story", &alice).await;
    let github: Vec<String> = h
        .fake
        .calls()
        .into_iter()
        .filter(|c| !c.contains("/api/v1/crates"))
        .collect();
    assert!(github.is_empty(), "{github:?}");
}
