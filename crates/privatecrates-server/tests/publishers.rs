//! Who may publish (SPEC §6.4): only people who can create releases in a crate's owning repository, from CI as
//! well as from their machines, and only from workflows triggered by events that need that access.

mod common;

use common::{Crate, Harness, error_code, error_detail};
use privatecrates_common::audience;
use privatecrates_testkit::FakeGitHub;
use serde_json::Value;

const REPO: &str = "acme/story-engine";
const OWNERS: &str = "owners/story_engine.toml";

fn krate(version: &str) -> Crate {
    Crate::new("story_engine", version, REPO)
}

/// A user with `role` on the repository, and their login and ID as a workflow run's actor.
fn person(h: &Harness, repo: u64, login: &str, role: &str) -> (String, u64) {
    let token = h.fake.add_user(login, "ghu_", &[]);
    h.fake.set_repo_role(&token, repo, role);
    (login.to_owned(), h.fake.user_id(&token))
}

fn claims(h: &Harness, repo: u64, actor: (&str, u64), event: &str) -> Value {
    FakeGitHub::actions_claims_for(&h.org, REPO, repo, "release.yml", actor, event)
}

/// Publishes `krate` from a workflow run with these claims.
async fn publish(h: &Harness, krate: &Crate, claims: &Value) -> reqwest::Response {
    let token = h.fake.oidc_token(
        &audience::publish(&h.base(), &krate.name, &krate.version, &krate.cksum()),
        claims,
    );
    h.publish(krate, &token).await
}

async fn refused(response: reqwest::Response, code: &str) -> String {
    assert_eq!(response.status(), 403);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["errors"][0]["code"], code, "{body}");
    body["errors"][0]["detail"].as_str().unwrap().to_owned()
}

fn permission_lookups(h: &Harness, login: &str) -> usize {
    let path = format!("GET /repos/{REPO}/collaborators/{login}/permission");
    h.fake.calls().iter().filter(|c| **c == path).count()
}

#[tokio::test]
async fn writers_maintainers_and_admins_publish() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    for (version, login, role) in [
        ("0.1.0", "alice", "write"),
        ("0.2.0", "bob", "maintain"),
        ("0.3.0", "carol", "admin"),
    ] {
        let (login, id) = person(&h, repo, login, role);
        let response = publish(&h, &krate(version), &claims(&h, repo, (&login, id), "push")).await;
        assert_eq!(
            response.status(),
            200,
            "{role}: {}",
            response.text().await.unwrap()
        );
    }
    // The commit, and the release notes made from it, name the actor and the trigger.
    let message = h
        .fake
        .commits(h.org.storage_repo)
        .last()
        .unwrap()
        .message
        .clone();
    assert!(
        message.contains(
            "Published by carol via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v1 \
             (run 42, attempt 1), triggered by push on refs/tags/v1"
        ),
        "{message}"
    );
}

#[tokio::test]
async fn readers_triagers_and_outsiders_cannot_publish() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let reader = person(&h, repo, "rita", "read");
    let triager = person(&h, repo, "trish", "triage");
    let outsider_token = h.fake.add_user("otto", "ghu_", &[]);
    let outsider = ("otto".to_owned(), h.fake.user_id(&outsider_token));
    for (login, id) in [reader, triager, outsider] {
        // Neither a first publish nor a later one.
        let response = publish(&h, &krate("0.1.0"), &claims(&h, repo, (&login, id), "push")).await;
        let detail = refused(response, "publish::actor_cannot_release").await;
        assert!(detail.contains(&login), "{detail}");
        assert!(detail.contains(REPO), "{detail}");
        assert!(detail.contains("Write access"), "{detail}");
        assert!(detail.contains("create releases"), "{detail}");
    }
    assert!(h.fake.file(h.org.storage_repo, OWNERS).is_none());
    assert!(h.fake.releases(h.org.storage_repo).is_empty());

    h.publish_from_ci(REPO, repo, &krate("0.1.0")).await;
    let (login, id) = person(&h, repo, "rhea", "read");
    let response = publish(&h, &krate("0.2.0"), &claims(&h, repo, (&login, id), "push")).await;
    refused(response, "publish::actor_cannot_release").await;
    assert_eq!(h.fake.releases(h.org.storage_repo).len(), 1);
}

#[tokio::test]
async fn an_actor_id_must_match_the_login() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let (writer, writer_id) = person(&h, repo, "alice", "write");
    let (_, other_id) = person(&h, repo, "bob", "read");
    // The run was started by account `other_id`, whose login was `alice`; `alice` now names another account.
    let response = publish(
        &h,
        &krate("0.1.0"),
        &claims(&h, repo, (&writer, other_id), "push"),
    )
    .await;
    let detail = refused(response, "publish::actor_cannot_release").await;
    assert!(detail.contains("alice"), "{detail}");
    assert!(h.fake.releases(h.org.storage_repo).is_empty());
    // That answer was about neither account, so it is not cached for either.
    let response = publish(
        &h,
        &krate("0.1.0"),
        &claims(&h, repo, (&writer, writer_id), "push"),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(permission_lookups(&h, "alice"), 2);
}

#[tokio::test]
async fn only_push_release_and_workflow_dispatch_may_publish() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let (login, id) = person(&h, repo, "alice", "write");
    for event in [
        "pull_request_target",
        "issue_comment",
        "pull_request",
        "workflow_run",
        "schedule",
        "merge_group",
        "repository_dispatch",
    ] {
        let response = publish(&h, &krate("0.1.0"), &claims(&h, repo, (&login, id), event)).await;
        let detail = refused(response, "publish::trigger_not_allowed").await;
        assert!(detail.contains(&format!("`{event}`")), "{detail}");
        assert!(detail.contains("without it"), "{detail}");
    }
    assert!(h.fake.releases(h.org.storage_repo).is_empty());
    for (version, event) in [
        ("0.1.0", "push"),
        ("0.2.0", "release"),
        ("0.3.0", "workflow_dispatch"),
    ] {
        let response = publish(&h, &krate(version), &claims(&h, repo, (&login, id), event)).await;
        assert_eq!(response.status(), 200, "{event}");
    }
}

#[tokio::test]
async fn listed_bots_publish_without_write_access() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    // A crate's first version is published by a person.
    let bot = ("release-please[bot]", 41898282);
    let response = publish(&h, &krate("0.1.0"), &claims(&h, repo, bot, "push")).await;
    let detail = refused(response, "publish::actor_cannot_release").await;
    assert!(detail.contains("publish_bots"), "{detail}");
    h.publish_from_ci(REPO, repo, &krate("0.1.0")).await;

    h.fake.write_file(
        h.org.storage_repo,
        OWNERS,
        &format!(
            "repository_id = {repo}\nrepository = \"{REPO}\"\npublish_workflows = [\"release.yml\"]\n\
             publish_bots = [\"release-please[bot]\"]\n"
        ),
    );
    h.refresh().await;
    h.fake.clear_calls();
    let response = publish(&h, &krate("0.2.0"), &claims(&h, repo, bot, "push")).await;
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    assert_eq!(permission_lookups(&h, "release-please[bot]"), 0);

    // The trigger rule still applies to a listed bot.
    let response = publish(
        &h,
        &krate("0.3.0"),
        &claims(&h, repo, bot, "pull_request_target"),
    )
    .await;
    refused(response, "publish::trigger_not_allowed").await;

    // A bot that is not listed is refused.
    let unlisted = ("renovate[bot]", 29139614);
    let response = publish(&h, &krate("0.3.0"), &claims(&h, repo, unlisted, "push")).await;
    let detail = refused(response, "publish::actor_cannot_release").await;
    assert!(
        detail.contains("renovate[bot]") && detail.contains("publish_bots"),
        "{detail}"
    );
}

#[tokio::test]
async fn an_actors_permission_is_cached_briefly() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let (login, id) = person(&h, repo, "alice", "write");
    h.fake.clear_calls();
    for version in ["0.1.0", "0.2.0", "0.3.0"] {
        let response = publish(&h, &krate(version), &claims(&h, repo, (&login, id), "push")).await;
        assert_eq!(response.status(), 200);
    }
    assert_eq!(permission_lookups(&h, "alice"), 1, "{:?}", h.fake.calls());

    // Each actor is looked up once, and so is a refusal.
    let (reader, reader_id) = person(&h, repo, "rita", "read");
    for _ in 0..2 {
        let response = publish(
            &h,
            &krate("0.4.0"),
            &claims(&h, repo, (&reader, reader_id), "push"),
        )
        .await;
        refused(response, "publish::actor_cannot_release").await;
    }
    assert_eq!(permission_lookups(&h, "rita"), 1);
}

#[tokio::test]
async fn manual_publishing_needs_write_access() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(REPO, repo, &krate("0.1.0")).await;
    h.fake.write_file(
        h.org.storage_repo,
        OWNERS,
        &format!(
            "repository_id = {repo}\nrepository = \"{REPO}\"\npublish_workflows = [\"release.yml\"]\n\
             allow_manual_publish = true\n"
        ),
    );
    h.refresh().await;
    for role in ["read", "triage"] {
        let token = h.fake.add_user(role, "ghu_", &[]);
        h.fake.set_repo_role(&token, repo, role);
        let response = h.publish(&krate("0.2.0"), &token).await;
        assert_eq!(response.status(), 403, "{role}");
        assert_eq!(error_code(response).await, "auth::push_required", "{role}");
    }
    for (version, role) in [
        ("0.2.0", "write"),
        ("0.3.0", "maintain"),
        ("0.4.0", "admin"),
    ] {
        let token = h.fake.add_user(role, "ghu_", &[]);
        h.fake.set_repo_role(&token, repo, role);
        let response = h.publish(&krate(version), &token).await;
        assert_eq!(
            response.status(),
            200,
            "{role}: {}",
            error_detail(response).await
        );
    }
}
