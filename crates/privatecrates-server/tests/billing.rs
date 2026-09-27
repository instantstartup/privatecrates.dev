//! Stripe billing and its enforcement (docs/website-api.md).

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use common::{Crate, Harness, Options, error_code, error_detail};
use privatecrates_testkit::stripe::{self, FakeStripe};
use serde_json::{Value, json};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

const DAY: u64 = 24 * 60 * 60;

async fn start() -> (Harness, FakeStripe) {
    let h = Harness::start_with(Options {
        stripe: true,
        ..Options::default()
    })
    .await;
    let stripe = h.stripe.clone().unwrap();
    (h, stripe)
}

async fn deliver(h: &Harness, body: &str, signature: Option<&str>) -> reqwest::Response {
    let mut request = h
        .client
        .post(h.apex("/webhooks/stripe"))
        .header("Content-Type", "application/json")
        .body(body.to_owned());
    if let Some(signature) = signature {
        request = request.header("Stripe-Signature", signature);
    }
    request.send().await.unwrap()
}

async fn send_event(h: &Harness, event_type: &str, object: Value) {
    let (body, signature) = stripe::webhook(event_type, object, now());
    let response = deliver(h, &body, Some(&signature)).await;
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
}

/// Signs in an admin of `acme`, and returns a crate repository and a reader's token.
async fn setup(h: &Harness) -> (String, u64, String) {
    let admin = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&admin, &h.org, "admin");
    let session = h.sign_in(&admin).await;
    let repo = h.repo("story-engine");
    let reader = h.fake.add_user("bob", "ghu_", &[(repo, true)]);
    (session, repo, reader)
}

#[tokio::test]
async fn checkout_starts_a_trial_and_the_webhook_activates_the_registry() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup(&h).await;

    // No subscription yet: the registry is not served.
    let response = h.get("/index/config.json", Some(&reader)).await;
    assert_eq!(response.status(), 402);
    assert_eq!(error_code(response).await, "billing::subscription_inactive");
    let session_doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(session_doc["orgs"][0]["tenant"]["status"], Value::Null);
    let onboarding: Value = h
        .api_get("/api/orgs/acme/onboarding", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(onboarding["steps"][4]["status"], "todo");

    let response = h
        .api_post("/api/orgs/acme/checkout", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert!(
        body["url"]
            .as_str()
            .unwrap()
            .starts_with("https://checkout.stripe.com/")
    );
    let checkout = stripe.checkouts().pop().unwrap();
    let org_id = h.org.id.to_string();
    assert_eq!(checkout["mode"], "subscription");
    assert_eq!(checkout["line_items[0][price]"], stripe::PRICE_ID);
    assert_eq!(checkout["line_items[0][quantity]"], "1");
    assert_eq!(checkout["subscription_data[trial_period_days]"], "14");
    assert_eq!(
        checkout["subscription_data[metadata][github_org_id]"],
        org_id
    );
    assert_eq!(
        checkout["subscription_data[metadata][github_org_login]"],
        "acme"
    );
    assert_eq!(checkout["client_reference_id"], org_id);
    assert_eq!(
        checkout["success_url"],
        h.apex("/account?org=acme&checkout=success")
    );
    assert_eq!(checkout["cancel_url"], h.apex("/account?org=acme"));

    let completed = stripe.complete_checkout();
    send_event(&h, "checkout.session.completed", completed).await;

    let session_doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    let tenant = &session_doc["orgs"][0]["tenant"];
    assert_eq!(tenant["status"], "trialing");
    assert!(tenant["trial_ends_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(tenant["current_period_end"], Value::Null);
    let response = h.get("/index/config.json", Some(&reader)).await;
    assert_eq!(response.status(), 200);
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;

    // One subscription at a time.
    let response = h
        .api_post("/api/orgs/acme/checkout", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::already_subscribed");

    let response = h
        .api_post("/api/orgs/acme/portal", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert!(
        body["url"]
            .as_str()
            .unwrap()
            .starts_with("https://billing.stripe.com/")
    );
    let portal = stripe.portals().pop().unwrap();
    assert_eq!(portal["customer"], format!("cus_{}", h.org.id));
    assert_eq!(portal["return_url"], h.apex("/account?org=acme"));
}

#[tokio::test]
async fn billing_needs_an_admin_and_a_subscription_for_the_portal() {
    let (h, _stripe) = start().await;
    let member = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&member, &h.org, "member");
    let member = h.sign_in(&member).await;
    for path in ["/api/orgs/acme/checkout", "/api/orgs/acme/portal"] {
        let response = h.api_post(path, &member).send().await.unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(error_code(response).await, "account::admin_required");
    }
    let (admin, _, _) = setup(&h).await;
    let response = h
        .api_post("/api/orgs/acme/portal", &admin)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::no_subscription");
}

#[tokio::test]
async fn webhooks_must_be_signed_and_fresh() {
    let (h, stripe) = start().await;
    let (_, _, reader) = setup(&h).await;
    let id = stripe.add_subscription(h.org.id, "acme", "active");
    let object = json!({ "id": id, "object": "subscription" });

    let (body, _) = stripe::webhook("customer.subscription.created", object.clone(), now());
    let forged = stripe::sign_webhook("whsec_other", now(), &body);
    let stale = stripe::sign_webhook(stripe::WEBHOOK_SECRET, now() - 600, &body);
    let future = stripe::sign_webhook(stripe::WEBHOOK_SECRET, now() + 600, &body);
    for signature in [None, Some(forged.as_str()), Some(&stale), Some(&future)] {
        let response = deliver(&h, &body, signature).await;
        assert_eq!(response.status(), 400, "{signature:?}");
        assert_eq!(error_code(response).await, "billing::webhook_invalid");
    }
    // A valid signature over other bytes.
    let (other, signature) = stripe::webhook("customer.subscription.created", object, now());
    let response = deliver(&h, &body.replace("created", "deleted"), Some(&signature)).await;
    assert_eq!(response.status(), 400);
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        402
    );

    let response = deliver(&h, &other, Some(&signature)).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );

    // Events we do not use are acknowledged.
    send_event(&h, "invoice.paid", json!({ "id": "in_1" })).await;
}

#[tokio::test]
async fn a_lapsed_subscription_stops_publishing_then_reads() {
    let (h, stripe) = start().await;
    let (_, repo, reader) = setup(&h).await;
    let id = stripe.add_subscription(h.org.id, "acme", "active");
    h.state.billing.load().await.unwrap();
    let v1 = Crate::new("story_engine", "0.1.0", "acme/story-engine");
    h.publish_from_ci("acme/story-engine", repo, &v1).await;

    // Past due: Stripe is retrying the payment, so everything keeps working.
    stripe.update_subscription(&id, json!({ "status": "past_due" }));
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    let v2 = Crate::new("story_engine", "0.2.0", "acme/story-engine");
    h.publish_from_ci("acme/story-engine", repo, &v2).await;

    // Cancelled yesterday: publishing stops at once, reads continue.
    stripe.update_subscription(
        &id,
        json!({ "status": "canceled", "ended_at": now() - DAY }),
    );
    send_event(&h, "customer.subscription.deleted", json!({ "id": id })).await;
    let v3 = Crate::new("story_engine", "0.3.0", "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &v3);
    let response = h.publish(&v3, &token).await;
    assert_eq!(response.status(), 402);
    let detail = error_detail(response).await;
    assert!(detail.contains(&h.apex("/account")), "{detail}");
    assert!(detail.contains("acme"), "{detail}");
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    assert_eq!(
        h.get("/index/st/or/story_engine", Some(&reader))
            .await
            .status(),
        200
    );
    let download = h
        .get("/api/v1/crates/story_engine/0.1.0/download", Some(&reader))
        .await;
    assert_eq!(download.status(), 302);

    // Fifteen days after it ended, reads stop too.
    stripe.update_subscription(&id, json!({ "ended_at": now() - 15 * DAY }));
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    for path in [
        "/index/config.json",
        "/index/st/or/story_engine",
        "/api/v1/crates/story_engine/0.1.0/download",
    ] {
        let response = h.get(path, Some(&reader)).await;
        assert_eq!(response.status(), 402, "{path}");
        assert_eq!(error_code(response).await, "billing::subscription_inactive");
    }
}

#[tokio::test]
async fn start_up_keeps_the_latest_subscription_per_organisation() {
    let (h, stripe) = start().await;
    let (session, _, reader) = setup(&h).await;
    stripe.add_subscription(h.org.id, "acme", "canceled");
    let current = stripe.add_subscription(h.org.id, "acme", "active");
    // Enough other customers for several pages.
    for org in 0..150 {
        stripe.add_subscription(10_000 + org, &format!("org{org}"), "active");
    }
    h.state.billing.load().await.unwrap();
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    assert_eq!(h.state.billing.subscription(h.org.id).unwrap().id, current);
    let session_doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    let tenant = &session_doc["orgs"][0]["tenant"];
    assert_eq!(tenant["status"], "active");
    assert!(tenant["current_period_end"].as_str().is_some());

    // A returning organisation keeps its customer and gets no second trial.
    stripe.update_subscription(&current, json!({ "status": "canceled", "ended_at": now() }));
    h.state.billing.load().await.unwrap();
    let response = h
        .api_post("/api/orgs/acme/checkout", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let checkout = stripe.checkouts().pop().unwrap();
    assert_eq!(checkout["customer"], format!("cus_{}", h.org.id));
    assert!(!checkout.contains_key("subscription_data[trial_period_days]"));
}
