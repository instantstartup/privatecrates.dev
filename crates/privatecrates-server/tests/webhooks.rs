//! GitHub webhooks (SPEC §7): verification, and their effect on tenants, permission caches and storage snapshots.

mod common;

use common::{Crate, Harness, READER_WEBHOOK_SECRET, STORAGE_WEBHOOK_SECRET};
use serde_json::{Value, json};

/// Delivers a webhook from the reader App, which sends most events.
async fn webhook(h: &Harness, event: &str, delivery: &str, payload: &Value) -> reqwest::Response {
    h.github_webhook(READER_WEBHOOK_SECRET, event, delivery, payload)
        .await
}

/// A crate published from `acme/story-engine`, and the index path to read it at.
async fn published(h: &Harness) -> (u64, &'static str) {
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    (repo, "/index/st/or/story_engine")
}

#[tokio::test]
async fn deliveries_must_be_signed() {
    let h = Harness::start().await;
    let url = format!("http://localhost:{}/webhooks/github", h.port);
    let unsigned = h
        .client
        .post(&url)
        .header("X-GitHub-Event", "ping")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(unsigned.status(), 401);
    let forged = h
        .client
        .post(&url)
        .header("X-GitHub-Event", "ping")
        .header("X-Hub-Signature-256", format!("sha256={}", "00".repeat(32)))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(forged.status(), 401);
    let zen = json!({ "zen": "Design for failure." });
    let other_secret = h
        .github_webhook(b"another App's secret", "ping", "d-1", &zen)
        .await;
    assert_eq!(other_secret.status(), 401);

    // Each App signs with its own secret. Events we do not use are accepted and ignored.
    let reader = h
        .github_webhook(READER_WEBHOOK_SECRET, "ping", "d-2", &zen)
        .await;
    assert_eq!(reader.status(), 204);
    let storage = h
        .github_webhook(STORAGE_WEBHOOK_SECRET, "ping", "d-3", &zen)
        .await;
    assert_eq!(storage.status(), 204);
}

#[tokio::test]
async fn removing_a_member_cuts_off_access_at_once() {
    let h = Harness::start().await;
    let (repo, path) = published(&h).await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);

    // GitHub removes her; the cached repository set still lets her read until a webhook says so.
    h.fake.set_user_repos(&alice, &[]);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);
    let removed = json!({
        "action": "member_removed",
        "membership": { "user": { "id": h.fake.user_id(&alice), "login": "alice" }, "role": "member" },
        "installation": { "id": h.org.reader_installation },
    });
    assert_eq!(
        webhook(&h, "organization", "d-1", &removed).await.status(),
        204
    );
    assert_eq!(h.get(path, Some(&alice)).await.status(), 404);

    // A redelivery is ignored: the set cached since is not dropped again.
    h.fake.set_user_repos(&alice, &[(repo, false)]);
    assert_eq!(
        webhook(&h, "organization", "d-1", &removed).await.status(),
        204
    );
    assert_eq!(h.get(path, Some(&alice)).await.status(), 404);

    // Adding her to a team with access is a new delivery.
    let added = json!({
        "action": "added",
        "member": { "id": h.fake.user_id(&alice), "login": "alice" },
        "installation": { "id": h.org.reader_installation },
    });
    assert_eq!(webhook(&h, "membership", "d-2", &added).await.status(), 204);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);
}

#[tokio::test]
async fn revoking_the_app_drops_the_users_sets_everywhere() {
    let h = Harness::start().await;
    let (repo, path) = published(&h).await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let bob = h.fake.add_user("bob", "ghu_", &[(repo, false)]);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);
    assert_eq!(h.get(path, Some(&bob)).await.status(), 200);
    h.fake.set_user_repos(&alice, &[]);
    h.fake.set_user_repos(&bob, &[]);

    // The event has no installation: it is about the App as a whole.
    let revoked = json!({ "action": "revoked", "sender": { "id": h.fake.user_id(&alice) } });
    assert_eq!(
        webhook(&h, "github_app_authorization", "d-1", &revoked)
            .await
            .status(),
        204
    );
    assert_eq!(h.get(path, Some(&alice)).await.status(), 404);
    // Only that user's sets were dropped.
    assert_eq!(h.get(path, Some(&bob)).await.status(), 200);
}

#[tokio::test]
async fn team_and_repository_changes_drop_the_tenants_sets() {
    let h = Harness::start().await;
    let (repo, path) = published(&h).await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let pat = h.fake.add_user("bob", "gho_", &[(repo, false)]);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);
    assert_eq!(h.get(path, Some(&pat)).await.status(), 200);
    h.fake.set_user_repos(&alice, &[]);
    h.fake.set_user_repos(&pat, &[]);

    let team = json!({
        "action": "removed_from_repository",
        "team": { "id": 5, "slug": "writers" },
        "repository": { "id": repo },
        "installation": { "id": h.org.reader_installation },
    });
    assert_eq!(webhook(&h, "team", "d-1", &team).await.status(), 204);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 404);
    assert_eq!(h.get(path, Some(&pat)).await.status(), 404);

    h.fake.set_user_repos(&alice, &[(repo, false)]);
    let renamed = json!({
        "action": "renamed",
        "repository": { "id": repo, "full_name": "acme/tale-engine" },
        "installation": { "id": h.org.reader_installation },
    });
    assert_eq!(
        webhook(&h, "repository", "d-2", &renamed).await.status(),
        204
    );
    assert_eq!(h.get(path, Some(&alice)).await.status(), 200);
    // Owners files are not rewritten on a rename: the repository ID is what counts.
    let owner = h
        .fake
        .file(h.org.storage_repo, "owners/story_engine.toml")
        .unwrap();
    assert!(
        owner.contains("repository = \"acme/story-engine\""),
        "{owner}"
    );
}

#[tokio::test]
async fn a_deleted_owning_repository_leaves_the_crate_to_organisation_owners() {
    let h = Harness::start().await;
    let (repo, path) = published(&h).await;
    let reader = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let owner = h
        .fake
        .add_user("olivia", "ghu_", &[(h.repo("other"), true)]);
    h.fake.add_member(&owner, &h.org, "admin");
    let member = h
        .fake
        .add_user("mallory", "ghu_", &[(h.repo("another"), true)]);
    assert_eq!(h.get(path, Some(&reader)).await.status(), 200);
    // While the repository exists, owning the organisation is not enough without access to it.
    assert_eq!(h.get(path, Some(&owner)).await.status(), 404);

    h.fake.delete_repo(repo);
    let deleted = json!({
        "action": "deleted",
        "repository": { "id": repo, "full_name": "acme/story-engine" },
        "installation": { "id": h.org.reader_installation },
    });
    assert_eq!(
        webhook(&h, "repository", "d-1", &deleted).await.status(),
        204
    );
    assert_eq!(h.get(path, Some(&reader)).await.status(), 404);
    assert_eq!(h.get(path, Some(&member)).await.status(), 404);
    assert_eq!(h.get(path, Some(&owner)).await.status(), 200);
    assert_eq!(
        h.get("/api/v1/crates/story_engine/0.1.0/download", Some(&owner))
            .await
            .status(),
        302
    );
}

/// Delivers a push to a branch of the storage repository.
async fn push(h: &Harness, delivery: &str, branch: &str) -> reqwest::StatusCode {
    let payload = json!({
        "ref": format!("refs/heads/{branch}"),
        "repository": { "id": h.org.storage_repo, "full_name": "acme/crates-store" },
        "installation": { "id": h.org.storage_installation },
    });
    h.github_webhook(STORAGE_WEBHOOK_SECRET, "push", delivery, &payload)
        .await
        .status()
}

#[tokio::test]
async fn a_push_to_the_storage_repository_reloads_it() {
    let h = Harness::start().await;
    let (repo, path) = published(&h).await;
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let index = h.get(path, Some(&alice)).await.text().await.unwrap();
    assert!(index.contains("\"yanked\":false"));

    // An administrator yanks the version with a commit of their own.
    let edited = index.replace("\"yanked\":false", "\"yanked\":true");
    h.fake
        .write_file(h.org.storage_repo, "index/st/or/story_engine", &edited);
    assert_eq!(h.get(path, Some(&alice)).await.text().await.unwrap(), index);
    // Other branches do not matter.
    assert_eq!(push(&h, "d-1", "draft").await, 204);
    assert_eq!(h.get(path, Some(&alice)).await.text().await.unwrap(), index);
    assert_eq!(push(&h, "d-2", "main").await, 204);
    assert_eq!(
        h.get(path, Some(&alice)).await.text().await.unwrap(),
        edited
    );

    // A changed slug takes effect at once.
    h.fake.write_file(
        h.org.storage_repo,
        "privatecrates.toml",
        "slug = \"other\"\n",
    );
    assert_eq!(push(&h, "d-3", "main").await, 204);
    assert_eq!(h.get(path, Some(&alice)).await.status(), 404);
    let moved = h
        .client
        .get(format!("http://other.localhost:{}{path}", h.port))
        .header("Authorization", &alice)
        .send()
        .await
        .unwrap();
    assert_eq!(moved.status(), 200);
}

#[tokio::test]
async fn a_new_installation_adds_a_tenant() {
    let h = Harness::start().await;
    let config = format!("http://other.localhost:{}/index/config.json", h.port);
    let globex = h.fake.add_org("globex", "other");
    let repo = h.fake.add_repo(&globex, "tools");
    let member = h.fake.add_user("gina", "ghu_", &[(repo, false)]);
    let get = || {
        h.client
            .get(&config)
            .header("Authorization", &member)
            .send()
    };
    // Not a registry yet: the same answer as a registry the caller cannot use.
    assert_eq!(get().await.unwrap().status(), 403);

    let installed = json!({
        "action": "created",
        "installation": { "id": globex.storage_installation, "account": { "login": "globex", "id": globex.id } },
    });
    assert_eq!(
        h.github_webhook(STORAGE_WEBHOOK_SECRET, "installation", "d-1", &installed)
            .await
            .status(),
        204
    );
    assert_eq!(get().await.unwrap().status(), 200);
}
