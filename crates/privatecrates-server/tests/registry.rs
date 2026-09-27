//! The registry's behaviour over HTTP, against a fake GitHub.

mod common;

use common::{Crate, Harness, error_detail};
use privatecrates_common::audience;
use privatecrates_testkit::FakeGitHub;
use serde_json::Value;

#[tokio::test]
async fn config_json_needs_a_token_and_access() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");

    let response = h.get("/index/config.json", None).await;
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.headers()["www-authenticate"],
        format!("Cargo login_url=\"{}/login\"", h.base()).as_str()
    );

    let member = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let response = h.get("/index/config.json", Some(&member)).await;
    assert_eq!(response.status(), 200);
    let config: Value = response.json().await.unwrap();
    assert_eq!(config["auth-required"], true);
    assert_eq!(config["api"], h.base());

    let outsider = h.fake.add_user("mallory", "ghu_", &[]);
    assert_eq!(
        h.get("/index/config.json", Some(&outsider)).await.status(),
        403
    );

    let unknown_tenant = h
        .client
        .get(format!(
            "http://other.localhost:{}/index/config.json",
            h.port
        ))
        .header("Authorization", &member)
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_tenant.status(), 404);
}

#[tokio::test]
async fn trusted_publish_then_read_and_download() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let krate = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    h.publish_from_ci("acme/story-engine", repo, &krate).await;

    // The owners file records the repository and the workflow that first published.
    let owner = h
        .fake
        .file(h.org.storage_repo, "owners/story_engine.toml")
        .unwrap();
    assert!(owner.contains(&format!("repository_id = {repo}")));
    assert!(owner.contains("publish_workflows = [\"release.yml\"]"));

    // An immutable release holds the crate and its provenance.
    let releases = h.fake.releases(h.org.storage_repo);
    assert_eq!(
        releases,
        vec![(
            "story_engine-0.2.0".to_owned(),
            true,
            vec![
                "story_engine-0.2.0.crate".to_owned(),
                "story_engine-0.2.0.provenance.jwt".to_owned()
            ]
        )]
    );
    // Every write is a commit by the storage App, naming the publisher.
    let commits = h.fake.commits(h.org.storage_repo);
    let ours: Vec<_> = commits
        .iter()
        .filter(|c| c.by_installation.is_some())
        .collect();
    assert_eq!(ours.len(), 2);
    assert!(
        ours.iter()
            .all(|c| c.by_installation == Some(h.org.storage_installation))
    );
    assert!(
        ours[1]
            .message
            .contains("workflow acme/story-engine/.github/workflows/release.yml")
    );

    // A reader of the repository sees the index line and can download.
    let reader = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let response = h.get("/index/st/or/story_engine", Some(&reader)).await;
    assert_eq!(response.status(), 200);
    let etag = response.headers()["etag"].to_str().unwrap().to_owned();
    let line: Value = serde_json::from_str(response.text().await.unwrap().trim()).unwrap();
    assert_eq!(line["vers"], "0.2.0");
    assert_eq!(line["cksum"], krate.cksum());
    assert_eq!(line["yanked"], false);

    let not_modified = h
        .client
        .get(h.url("/index/st/or/story_engine"))
        .header("Authorization", &reader)
        .header("If-None-Match", &etag)
        .send()
        .await
        .unwrap();
    assert_eq!(not_modified.status(), 304);

    let download = h
        .get("/api/v1/crates/story_engine/0.2.0/download", Some(&reader))
        .await;
    assert_eq!(download.status(), 302);
    let location = download.headers()["location"].to_str().unwrap().to_owned();
    let bytes = reqwest::get(location).await.unwrap().bytes().await.unwrap();
    assert_eq!(bytes.to_vec(), krate.bytes());

    // Someone who cannot read the repository cannot tell the crate exists.
    let outsider = h
        .fake
        .add_user("mallory", "ghu_", &[(h.repo("other"), true)]);
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&outsider))
            .await
            .status(),
        404
    );
    assert_eq!(
        h.get(
            "/api/v1/crates/story_engine/0.2.0/download",
            Some(&outsider)
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        h.get("/index/no/ne/nonexistent", Some(&reader))
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn the_provenance_asset_is_the_publish_token() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let krate = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &krate);
    assert_eq!(h.publish(&krate, &token).await.status(), 200);
    let provenance = h
        .fake
        .asset(
            h.org.storage_repo,
            "story_engine-0.2.0",
            "story_engine-0.2.0.provenance.jwt",
        )
        .unwrap();
    assert_eq!(provenance, token.as_bytes());
}

#[tokio::test]
async fn permission_lookups_are_cached() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.2.0", "acme/story-engine"),
    )
    .await;
    let reader = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    h.fake.clear_calls();
    for _ in 0..5 {
        assert_eq!(
            h.get("/index/st/or/story_engine", Some(&reader))
                .await
                .status(),
            200
        );
        assert_eq!(
            h.get("/index/config.json", Some(&reader)).await.status(),
            200
        );
    }
    let calls = h.fake.calls();
    let user_calls = calls.iter().filter(|c| c.contains("/user")).count();
    assert_eq!(
        user_calls, 2,
        "one /user and one repository-set call: {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("/git/")),
        "index served from cache: {calls:?}"
    );
}

#[tokio::test]
async fn publish_token_is_bound_to_the_bytes() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let krate = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &krate);

    let mut tampered = krate.clone();
    tampered.extra = "pub fn evil() {}".into();
    let response = h.publish(&tampered, &token).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("audience"));

    let mut other_version = krate.clone();
    other_version.version = "0.3.0".into();
    assert_eq!(h.publish(&other_version, &token).await.status(), 403);

    // A token for reading is not a publish token.
    let claims = FakeGitHub::actions_claims(&h.org, "acme/story-engine", repo, "release.yml");
    let read_token = h.fake.oidc_token(&audience::read(&h.base()), &claims);
    assert_eq!(h.publish(&krate, &read_token).await.status(), 403);

    assert!(h.fake.releases(h.org.storage_repo).is_empty());
}

#[tokio::test]
async fn first_publish_must_come_from_the_declared_repository() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let other = h.repo("other");
    let krate = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    let token = h.publish_token("acme/other", other, "release.yml", &krate);
    let response = h.publish(&krate, &token).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("package.repository"));

    let mut undeclared = krate.clone();
    undeclared.repository = None;
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &undeclared);
    assert_eq!(h.publish(&undeclared, &token).await.status(), 403);

    // A workflow in another organisation.
    let mut foreign = FakeGitHub::actions_claims(&h.org, "acme/story-engine", repo, "release.yml");
    foreign["repository_owner_id"] = "999999".into();
    let token = h.fake.oidc_token(
        &audience::publish(&h.base(), &krate.name, &krate.version, &krate.cksum()),
        &foreign,
    );
    assert_eq!(h.publish(&krate, &token).await.status(), 403);
}

#[tokio::test]
async fn later_publishes_must_use_an_allowed_workflow_and_environment() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;

    let next = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "sneaky.yml", &next);
    let response = h.publish(&next, &token).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("sneaky.yml"));

    // Another repository cannot publish a crate it does not own, even with the same workflow name.
    let other = h.repo("other");
    let token = h.publish_token("acme/other", other, "release.yml", &next);
    assert_eq!(h.publish(&next, &token).await.status(), 403);

    // An administrator requires an environment.
    h.fake.write_file(
        h.org.storage_repo,
        "owners/story_engine.toml",
        &format!(
            "repository_id = {repo}\nrepository = \"acme/story-engine\"\npublish_workflows = [\"release.yml\"]\npublish_environment = \"crates\"\n"
        ),
    );
    h.refresh().await;
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &next);
    assert_eq!(h.publish(&next, &token).await.status(), 403);
    let mut claims = FakeGitHub::actions_claims(&h.org, "acme/story-engine", repo, "release.yml");
    claims["environment"] = "crates".into();
    let token = h.fake.oidc_token(
        &audience::publish(&h.base(), &next.name, &next.version, &next.cksum()),
        &claims,
    );
    assert_eq!(h.publish(&next, &token).await.status(), 200);
}

#[tokio::test]
async fn manual_publishing_is_opt_in_and_needs_push() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let pusher = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    let reader = h.fake.add_user("bob", "ghu_", &[(repo, false)]);

    // A brand-new crate cannot be published from a machine.
    let first = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    let response = h.publish(&first, &pusher).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("GitHub Actions only"));

    h.publish_from_ci("acme/story-engine", repo, &first).await;
    let next = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    assert_eq!(h.publish(&next, &pusher).await.status(), 403);

    h.fake.write_file(
        h.org.storage_repo,
        "owners/story_engine.toml",
        &format!(
            "repository_id = {repo}\nrepository = \"acme/story-engine\"\npublish_workflows = [\"release.yml\"]\nallow_manual_publish = true\n"
        ),
    );
    h.refresh().await;
    let response = h.publish(&next, &reader).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("push access"));
    assert_eq!(h.publish(&next, &pusher).await.status(), 200);

    // Manual publishes have no provenance asset, and the commit says so.
    let releases = h.fake.releases(h.org.storage_repo);
    assert_eq!(releases[1].2, vec!["story_engine-0.2.0.crate".to_owned()]);
    let commits = h.fake.commits(h.org.storage_repo);
    assert!(
        commits
            .last()
            .unwrap()
            .message
            .contains("user alice (manual publish, no provenance)")
    );
}

#[tokio::test]
async fn versions_are_immutable() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let krate = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    h.publish_from_ci("acme/story-engine", repo, &krate).await;

    let mut again = krate.clone();
    again.extra = "// different".into();
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &again);
    let response = h.publish(&again, &token).await;
    assert_eq!(response.status(), 409);
    assert!(error_detail(response).await.contains("already published"));
}

#[tokio::test]
async fn publishing_needs_immutable_releases() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.fake.set_immutable_releases(h.org.storage_repo, false);
    let krate = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &krate);
    let response = h.publish(&krate, &token).await;
    assert_eq!(response.status(), 424);
    assert!(error_detail(response).await.contains("immutable releases"));
    assert!(
        h.fake
            .file(h.org.storage_repo, "index/st/or/story_engine")
            .is_none()
    );
    assert!(h.fake.releases(h.org.storage_repo).is_empty());

    // Once enabled, the same publish succeeds.
    h.fake.set_immutable_releases(h.org.storage_repo, true);
    assert_eq!(h.publish(&krate, &token).await.status(), 200);
}

#[tokio::test]
async fn a_crash_before_the_index_append_is_recovered() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;

    // Same bytes: the leftover release is reused.
    let krate = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    h.fake.add_release(
        h.org.storage_repo,
        "story_engine-0.2.0",
        "story_engine-0.2.0.crate",
        &krate.bytes(),
    );
    h.publish_from_ci("acme/story-engine", repo, &krate).await;
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(index.contains("\"vers\":\"0.2.0\""));

    // Different bytes: the version is lost for good.
    let other = Crate::new("story_engine", "0.3.0", "acme/story-engine");
    h.fake.add_release(
        h.org.storage_repo,
        "story_engine-0.3.0",
        "story_engine-0.3.0.crate",
        b"something else",
    );
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &other);
    let response = h.publish(&other, &token).await;
    assert_eq!(response.status(), 409);
    assert!(
        error_detail(response)
            .await
            .contains("publish a new version")
    );
}

#[tokio::test]
async fn name_clashes_with_crates_io() {
    let h = Harness::start().await;
    let repo = h.repo("serde");
    h.fake.add_crates_io_crate("serde");
    let krate = Crate::new("serde", "1.0.0", "acme/serde");
    let token = h.publish_token("acme/serde", repo, "release.yml", &krate);
    let response = h.publish(&krate, &token).await;
    assert_eq!(response.status(), 403);
    assert!(error_detail(response).await.contains("exists on crates.io"));

    h.fake.write_file(
        h.org.storage_repo,
        "privatecrates.toml",
        "slug = \"acme\"\nname_clash = \"warn\"\n",
    );
    h.state.discover().await.unwrap();
    let response = h.publish(&krate, &token).await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert!(
        body["warnings"]["other"][0]
            .as_str()
            .unwrap()
            .contains("crates.io")
    );
}

#[tokio::test]
async fn dependencies_must_be_here_or_on_crates_io() {
    let h = Harness::start().await;
    let core_repo = h.repo("story-core");
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-core",
        core_repo,
        &Crate::new("story_core", "0.1.0", "acme/story-core"),
    )
    .await;

    let bad = Crate::new("story_engine", "0.1.0", "acme/story-engine").dep("missing", "^1", None);
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &bad);
    let response = h.publish(&bad, &token).await;
    assert_eq!(response.status(), 400);
    assert!(error_detail(response).await.contains("missing"));

    let foreign = Crate::new("story_engine", "0.1.0", "acme/story-engine").dep(
        "x",
        "^1",
        Some("sparse+https://other.example/index/"),
    );
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &foreign);
    assert_eq!(h.publish(&foreign, &token).await.status(), 400);

    let good = Crate::new("story_engine", "0.1.0", "acme/story-engine")
        .dep("story_core", "^0.1", None)
        .dep(
            "serde",
            "^1",
            Some("https://github.com/rust-lang/crates.io-index"),
        );
    h.publish_from_ci("acme/story-engine", repo, &good).await;
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    let line: Value = serde_json::from_str(index.trim()).unwrap();
    assert_eq!(line["deps"][0]["name"], "story_core");
    assert!(line["deps"][0].get("registry").is_none());
    assert_eq!(
        line["deps"][1]["registry"],
        "https://github.com/rust-lang/crates.io-index"
    );
}

#[tokio::test]
async fn yank_and_unyank_need_push() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    let pusher = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    let reader = h.fake.add_user("bob", "ghu_", &[(repo, false)]);
    let outsider = h.fake.add_user("mallory", "ghu_", &[]);
    let yank = |token: String| {
        h.client
            .delete(h.url("/api/v1/crates/story_engine/0.1.0/yank"))
            .header("Authorization", token)
            .send()
    };
    assert_eq!(yank(outsider).await.unwrap().status(), 404);
    assert_eq!(yank(reader).await.unwrap().status(), 403);
    let response = yank(pusher.clone()).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap()["ok"], true);
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(index.contains("\"yanked\":true"));
    assert!(
        h.fake
            .commits(h.org.storage_repo)
            .last()
            .unwrap()
            .message
            .contains("Yank story_engine 0.1.0")
    );

    let unyank = h
        .client
        .put(h.url("/api/v1/crates/story_engine/0.1.0/unyank"))
        .header("Authorization", &pusher)
        .send()
        .await
        .unwrap();
    assert_eq!(unyank.status(), 200);
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(index.contains("\"yanked\":false"));

    let missing = h
        .client
        .delete(h.url("/api/v1/crates/story_engine/9.9.9/yank"))
        .header("Authorization", &pusher)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn sso_errors_are_explained() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let token = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    h.fake.block_sso(&token);
    let response = h.get("/index/config.json", Some(&token)).await;
    assert_eq!(response.status(), 403);
    let detail = error_detail(response).await;
    assert!(detail.contains("single sign-on"), "{detail}");
    assert!(
        detail.contains("https://github.com/orgs/acme/sso"),
        "{detail}"
    );
}

#[tokio::test]
async fn ci_reads_with_an_exchanged_token() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let consumer = h.repo("website");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;

    let claims = FakeGitHub::actions_claims(&h.org, "acme/website", consumer, "ci.yml");
    let oidc = h.fake.oidc_token(&audience::read(&h.base()), &claims);

    // OIDC tokens are for publishing and exchange only.
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&oidc))
            .await
            .status(),
        401
    );

    let response = h
        .client
        .post(h.url("/api/v1/oidc/exchange"))
        .header("Authorization", format!("Bearer {oidc}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let token = body["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("pcr_"));

    assert_eq!(
        h.get("/index/config.json", Some(&token)).await.status(),
        200
    );
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&token))
            .await
            .status(),
        200
    );
    let download = h
        .get("/api/v1/crates/story_engine/0.1.0/download", Some(&token))
        .await;
    assert_eq!(download.status(), 302);

    // Read-only: it cannot publish or yank.
    let next = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    assert_eq!(h.publish(&next, &token).await.status(), 403);

    // A token with the wrong audience is refused.
    let wrong = h.fake.oidc_token("https://elsewhere.example", &claims);
    let response = h
        .client
        .post(h.url("/api/v1/oidc/exchange"))
        .header("Authorization", wrong)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn other_github_tokens_are_checked_per_repository() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    let oauth = h.fake.add_user("alice", "gho_", &[(repo, false)]);
    assert_eq!(
        h.get("/index/config.json", Some(&oauth)).await.status(),
        200
    );
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&oauth))
            .await
            .status(),
        200
    );
    let other = h.fake.add_user("bob", "gho_", &[]);
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&other))
            .await
            .status(),
        404
    );
    assert_eq!(
        h.get("/index/st/or/story_engine", Some("ghp_bogus"))
            .await
            .status(),
        401
    );
}

#[tokio::test]
async fn concurrent_publishes_of_different_crates() {
    let h = Harness::start().await;
    let a = h.repo("a-crate");
    let b = h.repo("b-crate");
    let ka = Crate::new("a_crate", "1.0.0", "acme/a-crate");
    let kb = Crate::new("b_crate", "1.0.0", "acme/b-crate");
    let ta = h.publish_token("acme/a-crate", a, "release.yml", &ka);
    let tb = h.publish_token("acme/b-crate", b, "release.yml", &kb);
    let (ra, rb) = tokio::join!(h.publish(&ka, &ta), h.publish(&kb, &tb));
    assert_eq!(ra.status(), 200);
    assert_eq!(rb.status(), 200);
    assert!(
        h.fake
            .file(h.org.storage_repo, "index/a_/cr/a_crate")
            .is_some()
    );
    assert!(
        h.fake
            .file(h.org.storage_repo, "index/b_/cr/b_crate")
            .is_some()
    );
}

#[tokio::test]
async fn an_index_edited_elsewhere_is_picked_up_on_conflict() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    // Someone else's commit changes the file behind the server's back.
    let current = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    h.fake.write_file(
        h.org.storage_repo,
        "index/st/or/story_engine",
        &current.replace("\"yanked\":false", "\"yanked\":true"),
    );
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.2.0", "acme/story-engine"),
    )
    .await;
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert_eq!(index.lines().count(), 2);
    assert!(index.lines().next().unwrap().contains("\"yanked\":true"));
}
