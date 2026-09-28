//! Registries for personal GitHub accounts: set up by their owner alone, and always free (SPEC §2.2).

mod common;

use common::{Crate, Harness, Options, error_code};
use privatecrates_common::{TERMS_VERSION, audience};
use privatecrates_testkit::{READER_APP_ID, STORAGE_APP_ID};
use serde_json::{Value, json};

#[tokio::test]
async fn a_personal_account_has_a_free_registry() {
    // Billing on, with a member limit a personal account would never reach anyway: it is free regardless.
    let h = Harness::start_with(Options {
        preview: false,
        stripe: true,
        ..Options::default()
    })
    .await;
    let alice_token = h.fake.add_user("alice", "ghu_", &[]);
    let alice = h.fake.add_personal_account(&alice_token);
    let session = h.sign_in(&alice_token).await;
    let orgs = |doc: &Value| {
        doc["orgs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["login"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };

    // Listed once the reader App is installed on it, as an organisation is.
    let doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    assert!(orgs(&doc).is_empty());
    h.fake.install_app(&alice, READER_APP_ID);
    let doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(orgs(&doc), ["alice"]);
    let account = &doc["orgs"][0];
    assert_eq!(account["personal"], true);
    assert_eq!(account["role"], "admin");
    assert_eq!(account["plan"], "free");
    assert_eq!(account["trial_available"], false);

    let onboarding: Value = h
        .api_get("/api/orgs/alice/onboarding", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(onboarding["personal"], true);
    h.fake.install_app(&alice, STORAGE_APP_ID);
    let response = h
        .api_post("/api/orgs/alice/settings", &session)
        .body(json!({ "slug": "alice", "accept_terms": TERMS_VERSION }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    assert!(
        h.fake
            .file(alice.storage_repo, "privatecrates.toml")
            .is_some()
    );
    let onboarding: Value = h
        .api_get("/api/orgs/alice/onboarding", &session)
        .await
        .json()
        .await
        .unwrap();
    for step in onboarding["steps"].as_array().unwrap() {
        assert_eq!(step["status"], "done", "{step}");
    }

    // Nothing to pay, ever.
    for action in ["trial", "checkout"] {
        let response = h
            .api_post(&format!("/api/orgs/alice/{action}"), &session)
            .body(json!({ "billing_email": "alice@example.com" }).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 409, "{action}");
        assert_eq!(error_code(response).await, "billing::personal_account");
    }

    // Published from Actions in one of alice's repositories; read by a collaborator, not by anyone else.
    let engine = h.fake.add_repo(&alice, "engine");
    let bob = h.fake.add_user("bob", "ghu_", &[(engine, false)]);
    let mallory = h.fake.add_user("mallory", "ghu_", &[]);
    let base = format!("http://alice.localhost:{}", h.port);
    let krate = Crate::new("engine", "0.1.0", "alice/engine");
    let claims = h
        .fake
        .actions_claims(&alice, "alice/engine", engine, "release.yml");
    let token = h.fake.oidc_token(
        &audience::publish(&base, &krate.name, &krate.version, &krate.cksum()),
        &claims,
    );
    let published = h
        .client
        .put(format!("{base}/api/v1/crates/new"))
        .header("Authorization", token)
        .body(krate.body())
        .send()
        .await
        .unwrap();
    assert_eq!(
        published.status(),
        200,
        "{}",
        published.text().await.unwrap()
    );
    let get = |path: &str, token: &str| {
        h.client
            .get(format!("{base}{path}"))
            .header("Authorization", token)
            .send()
    };
    assert_eq!(get("/index/config.json", &bob).await.unwrap().status(), 200);
    assert_eq!(
        get("/index/en/gi/engine", &bob).await.unwrap().status(),
        200
    );
    assert_eq!(
        get("/index/config.json", &mallory).await.unwrap().status(),
        403
    );
    assert_eq!(
        get("/index/en/gi/engine", &mallory).await.unwrap().status(),
        404
    );

    // Only alice manages it: to anyone else it is not an organisation of theirs.
    let bob_session = h.sign_in(&bob).await;
    let response = h.api_get("/api/orgs/alice/onboarding", &bob_session).await;
    assert_eq!(response.status(), 404);
    assert_eq!(error_code(response).await, "account::org_not_found");
}
