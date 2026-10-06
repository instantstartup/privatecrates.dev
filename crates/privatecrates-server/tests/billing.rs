//! Plans, Stripe billing and their enforcement (docs/website-api.md).

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use common::{Crate, Harness, Options, READER_WEBHOOK_SECRET, error_code, error_detail};
use privatecrates_common::TERMS_VERSION;
use privatecrates_server::AppState;
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
        preview: false,
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

/// Signs in an admin of `acme`, its only member so far, and returns a crate repository and a reader's token.
async fn setup(h: &Harness) -> (String, u64, String) {
    let admin = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&admin, &h.org, "admin");
    let session = h.sign_in(&admin).await;
    let repo = h.repo("story-engine");
    let reader = h.fake.add_user("bob", "ghu_", &[(repo, true)]);
    (session, repo, reader)
}

/// Like [`setup`], with `acme` grown to 12 members: over the free limit.
async fn setup_large(h: &Harness) -> (String, u64, String) {
    let setup = setup(h).await;
    h.fake.add_org_members(&h.org, 11);
    setup
}

/// A GitHub `organization` webhook for a member of `acme` joining or leaving.
async fn membership_changed(h: &Harness, action: &str, delivery: &str) {
    let payload = json!({
        "action": action,
        "membership": { "user": { "id": 424242, "login": "carol" }, "state": "active", "role": "member" },
        "organization": { "id": h.org.id, "login": "acme" },
        "installation": { "id": h.org.reader_installation },
    });
    let response = h
        .github_webhook(READER_WEBHOOK_SECRET, "organization", delivery, &payload)
        .await;
    assert_eq!(response.status(), 204);
}

async fn session_org(h: &Harness, session: &str) -> Value {
    let doc: Value = h
        .api_get("/api/session", session)
        .await
        .json()
        .await
        .unwrap();
    doc["orgs"][0].clone()
}

async fn plan_step(h: &Harness, session: &str) -> Value {
    let doc: Value = h
        .api_get("/api/orgs/acme/onboarding", session)
        .await
        .json()
        .await
        .unwrap();
    let step = doc["steps"][4].clone();
    assert_eq!(step["id"], "plan");
    step
}

const BILLING_EMAIL: &str = "billing@acme.example";

/// `POST /api/orgs/acme/{action}` with the body the website sends: the billing email to start the trial or set it,
/// `{}` otherwise.
async fn post(h: &Harness, session: &str, action: &str) -> reqwest::Response {
    let body = match action {
        "trial" | "billing-email" => json!({ "billing_email": BILLING_EMAIL }),
        _ => json!({}),
    };
    h.api_post(&format!("/api/orgs/acme/{action}"), session)
        .body(body.to_string())
        .send()
        .await
        .unwrap()
}

async fn publish(h: &Harness, repo: u64, version: &str) -> reqwest::Response {
    let krate = Crate::new("story_engine", version, "acme/story-engine");
    let token = h.publish_token("acme/story-engine", repo, "release.yml", &krate);
    h.publish(&krate, &token).await
}

/// Publishes, asserting success, and returns Cargo's `warnings.other`.
async fn publish_warnings(h: &Harness, repo: u64, version: &str) -> Vec<Value> {
    let response = publish(h, repo, version).await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    body["warnings"]["other"].as_array().unwrap().clone()
}

#[tokio::test]
async fn a_free_organisation_needs_no_subscription() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup(&h).await;
    h.fake.add_org_members(&h.org, 4);

    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    assert!(publish_warnings(&h, repo, "0.1.0").await.is_empty());

    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 5);
    assert_eq!(org["free_member_limit"], 5);
    assert_eq!(org["plan"], "free");
    assert_eq!(org["trial_ends_at"], Value::Null);
    assert_eq!(org["has_payment_method"], false);
    assert_eq!(org["current_period_end"], Value::Null);
    assert_eq!(org["trial_available"], false);
    assert_eq!(org["tenant"]["status"], Value::Null);
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "done");
    assert_eq!(step["detail"], "Free: 5 of 5 members");

    // Nothing to pay for.
    for action in ["trial", "checkout"] {
        let response = post(&h, &session, action).await;
        assert_eq!(response.status(), 409, "{action}");
        assert_eq!(error_code(response).await, "billing::free_plan");
    }
    assert!(stripe.customers().is_empty());
}

#[tokio::test]
async fn over_the_limit_without_a_subscription_is_inactive() {
    let (h, _stripe) = start().await;
    let (session, repo, reader) = setup_large(&h).await;

    let response = h.get("/index/config.json", Some(&reader)).await;
    assert_eq!(response.status(), 402);
    assert_eq!(error_code(response).await, "billing::subscription_inactive");
    assert_eq!(publish(&h, repo, "0.1.0").await.status(), 402);

    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 12);
    assert_eq!(org["plan"], "inactive");
    assert_eq!(org["trial_available"], true);
    assert_eq!(org["tenant"]["status"], Value::Null);
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "todo");
    assert_eq!(
        step["detail"],
        "12 members: start your 3-month free trial, no card needed. After it, $70 a month."
    );
    // $10 for each of the 7 members past the free 5.
    assert_eq!(org["monthly_price_usd"], 70);

    // The trial comes first; Checkout is for organisations that already had one.
    let response = post(&h, &session, "checkout").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::trial_available");
}

#[tokio::test]
async fn the_trial_starts_without_a_card() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup_large(&h).await;

    let response = post(&h, &session, "trial").await;
    assert_eq!(response.status(), 200);
    let doc: Value = response.json().await.unwrap();
    assert_eq!(doc["org"]["login"], "acme");
    assert_eq!(doc["steps"][4]["id"], "plan");
    assert_eq!(doc["steps"][4]["status"], "done");

    let org_id = h.org.id.to_string();
    let [customer]: [Value; 1] = stripe.customers().try_into().unwrap();
    assert_eq!(customer["metadata"]["github_org_id"], org_id);
    assert_eq!(customer["metadata"]["github_org_login"], "acme");
    // Stripe's reminder before the trial ends goes to the billing email.
    assert_eq!(customer["email"], BILLING_EMAIL);
    let [request]: [_; 1] = stripe.subscription_requests().try_into().unwrap();
    assert_eq!(request["customer"], customer["id"].as_str().unwrap());
    assert_eq!(request["items[0][price]"], stripe::PRICE_ID);
    assert_eq!(request["items[0][quantity]"], "7");
    assert_eq!(request["trial_period_days"], "90");
    assert_eq!(
        request["payment_settings[save_default_payment_method]"],
        "on_subscription"
    );
    assert_eq!(
        request["trial_settings[end_behavior][missing_payment_method]"],
        "cancel"
    );
    assert_eq!(request["metadata[github_org_id]"], org_id);
    assert_eq!(request["metadata[github_org_login]"], "acme");
    let [subscription]: [Value; 1] = stripe.subscriptions(h.org.id).try_into().unwrap();
    let trial_end = subscription["trial_end"].as_u64().unwrap();
    assert!(trial_end.abs_diff(now() + 90 * DAY) < 60);

    // Recorded at once, without waiting for Stripe's webhook.
    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "trial");
    assert!(org["trial_ends_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(org["has_payment_method"], false);
    assert_eq!(org["billing_email_missing"], false);
    assert_eq!(org["current_period_end"], Value::Null);
    assert_eq!(org["trial_available"], false);
    assert_eq!(org["tenant"]["status"], "trialing");
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    // Ninety days to go: no reminder yet.
    assert!(publish_warnings(&h, repo, "0.1.0").await.is_empty());

    let response = post(&h, &session, "trial").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::already_subscribed");
    let response = post(&h, &session, "checkout").await;
    assert_eq!(error_code(response).await, "billing::already_subscribed");

    // A card is added in the billing portal, and the trial converts.
    let response = post(&h, &session, "portal").await;
    assert_eq!(response.status(), 200);
    let portal = stripe.portals().pop().unwrap();
    assert_eq!(portal["customer"], customer["id"].as_str().unwrap());
    let id = subscription["id"].as_str().unwrap();
    stripe.add_card(id);
    h.state.billing.load().await.unwrap();
    assert_eq!(session_org(&h, &session).await["has_payment_method"], true);
    stripe.end_trial(id);
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "paid");
    assert!(org["current_period_end"].as_str().is_some());
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
}

#[tokio::test]
async fn the_trial_needs_a_billing_email() {
    let (h, stripe) = start().await;
    let (session, _, _) = setup_large(&h).await;
    for body in [
        json!({}),
        json!({ "billing_email": null }),
        json!({ "billing_email": "" }),
        json!({ "billing_email": "billing" }),
        json!({ "billing_email": "billing@acme" }),
        json!({ "billing_email": "a@acme.example, b@acme.example" }),
        json!({ "billing_email": 42 }),
    ] {
        let response = h
            .api_post("/api/orgs/acme/trial", &session)
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{body}");
        assert_eq!(
            error_code(response).await,
            "billing::email_invalid",
            "{body}"
        );
    }
    assert!(stripe.customers().is_empty());

    // Surrounding spaces, as a form might add, are not part of the address.
    let response = h
        .api_post("/api/orgs/acme/trial", &session)
        .body(json!({ "billing_email": " billing@acme.example " }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let [customer]: [Value; 1] = stripe.customers().try_into().unwrap();
    assert_eq!(customer["email"], "billing@acme.example");
}

#[tokio::test]
async fn a_trial_started_automatically_asks_for_a_billing_email() {
    let (h, stripe) = start().await;
    let (session, _, _) = setup_large(&h).await;
    // Not yet subscribed: nothing to set it on.
    let response = post(&h, &session, "billing-email").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::no_subscription");
    assert_eq!(
        session_org(&h, &session).await["billing_email_missing"],
        false
    );

    let tenant = h.state.tenants.by_org(h.org.id).unwrap();
    h.state.start_trial_if_grown(&tenant).await;
    let [customer]: [Value; 1] = stripe.customers().try_into().unwrap();
    assert_eq!(customer["email"], Value::Null);
    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "trial");
    assert_eq!(org["billing_email_missing"], true);

    // Only an admin sets it, and only to an address.
    let member = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&member, &h.org, "member");
    let member = h.sign_in(&member).await;
    let response = post(&h, &member, "billing-email").await;
    assert_eq!(response.status(), 403);
    assert_eq!(error_code(response).await, "account::admin_required");
    let response = h
        .api_post("/api/orgs/acme/billing-email", &session)
        .body(json!({ "billing_email": "billing" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(error_code(response).await, "billing::email_invalid");

    let response = post(&h, &session, "billing-email").await;
    assert_eq!(response.status(), 200);
    let doc: Value = response.json().await.unwrap();
    assert_eq!(doc["org"]["login"], "acme");
    let [customer]: [Value; 1] = stripe.customers().try_into().unwrap();
    assert_eq!(customer["email"], BILLING_EMAIL);
    // Shown at once, without waiting for the next reload.
    assert_eq!(
        session_org(&h, &session).await["billing_email_missing"],
        false
    );
}

#[tokio::test]
async fn a_double_click_starts_one_trial() {
    let (h, stripe) = start().await;
    let (session, _, _) = setup_large(&h).await;
    let (first, second) = tokio::join!(post(&h, &session, "trial"), post(&h, &session, "trial"));
    let mut statuses = [first.status().as_u16(), second.status().as_u16()];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(stripe.subscriptions(h.org.id).len(), 1);

    // Another instance that has not seen the subscription yet repeats the same idempotent requests.
    let other = AppState::new(h.state.config.clone()).unwrap();
    other
        .billing
        .start_trial(h.org.id, "acme", Some(12), Some(BILLING_EMAIL))
        .await
        .unwrap();
    assert_eq!(stripe.subscriptions(h.org.id).len(), 1);
    assert_eq!(stripe.customers().len(), 1);
}

#[tokio::test]
async fn a_trial_that_ends_without_a_card_is_cancelled() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup_large(&h).await;
    assert_eq!(post(&h, &session, "trial").await.status(), 200);
    publish_warnings(&h, repo, "0.1.0").await;
    let id = stripe.subscriptions(h.org.id)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    stripe.end_trial(&id);
    send_event(&h, "customer.subscription.deleted", json!({ "id": id })).await;
    assert_eq!(stripe.subscriptions(h.org.id)[0]["status"], "canceled");

    // Publishing stops at once; reads continue for the grace period.
    let response = publish(&h, repo, "0.2.0").await;
    assert_eq!(response.status(), 402);
    let detail = error_detail(response).await;
    assert!(detail.contains(&h.apex("/account")), "{detail}");
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

    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "inactive");
    assert_eq!(org["trial_available"], false);
    assert_eq!(org["tenant"]["status"], "canceled");
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "todo");
    assert_eq!(
        step["detail"],
        "12 members: the subscription is canceled. Subscribe with a card to keep using the registry."
    );

    // One trial per organisation: now it subscribes through Checkout, with a card and no trial.
    let response = post(&h, &session, "trial").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::trial_used");
    let response = post(&h, &session, "checkout").await;
    assert_eq!(response.status(), 200);
    let checkout = stripe.checkouts().pop().unwrap();
    assert_eq!(
        checkout["customer"],
        stripe.customers()[0]["id"].as_str().unwrap()
    );
    assert!(!checkout.contains_key("subscription_data[trial_period_days]"));

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
async fn publishes_remind_of_a_trial_ending_without_a_card() {
    let (h, stripe) = start().await;
    let (session, repo, _) = setup_large(&h).await;
    assert_eq!(post(&h, &session, "trial").await.status(), 200);
    let id = stripe.subscriptions(h.org.id)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let ends = now() + 10 * DAY;
    stripe.update_subscription(&id, json!({ "trial_end": ends }));
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;

    let date = time::OffsetDateTime::from_unix_timestamp(ends as i64)
        .unwrap()
        .date();
    assert_eq!(
        publish_warnings(&h, repo, "0.1.0").await,
        [json!(format!(
            "the PrivateCrates free trial for acme ends on {date}; add a card at {}",
            h.apex("/account")
        ))]
    );

    // With a card, the trial converts by itself: no reminder.
    stripe.add_card(&id);
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    assert!(publish_warnings(&h, repo, "0.2.0").await.is_empty());
}

#[tokio::test]
async fn growing_past_the_limit_starts_the_trial() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup(&h).await;
    h.fake.add_org_members(&h.org, 4);
    // Counted: five members.
    assert_eq!(session_org(&h, &session).await["plan"], "free");

    h.fake.add_org_members(&h.org, 1);
    membership_changed(&h, "member_added", "d-1").await;
    let [subscription]: [Value; 1] = stripe.subscriptions(h.org.id).try_into().unwrap();
    assert_eq!(subscription["status"], "trialing");
    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 6);
    assert_eq!(org["plan"], "trial");
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    publish_warnings(&h, repo, "0.1.0").await;

    assert_eq!(subscription["items"]["data"][0]["quantity"], 1);
    assert_eq!(org["monthly_price_usd"], 10);

    // Only the first time: more members start nothing more, and are billed from the next invoice.
    h.fake.add_org_members(&h.org, 1);
    membership_changed(&h, "member_added", "d-2").await;
    let [subscription]: [Value; 1] = stripe.subscriptions(h.org.id).try_into().unwrap();
    assert_eq!(subscription["items"]["data"][0]["quantity"], 2);
    let update = stripe.subscription_requests().pop().unwrap();
    assert_eq!(update["proration_behavior"], "none");
    assert_eq!(session_org(&h, &session).await["monthly_price_usd"], 20);
}

#[tokio::test]
async fn the_price_follows_the_members_and_stops_at_100() {
    let (h, stripe) = start().await;
    let (session, _, _) = setup_large(&h).await;
    assert_eq!(post(&h, &session, "trial").await.status(), 200);
    let quantity = || stripe.subscriptions(h.org.id)[0]["items"]["data"][0]["quantity"].clone();
    assert_eq!(quantity(), 7);

    // GitHub sends one webhook per member. 15 members pay for 10: $100 a month, the cap.
    for n in 0..3 {
        h.fake.add_org_members(&h.org, 1);
        membership_changed(&h, "member_added", &format!("add-{n}")).await;
    }
    assert_eq!(quantity(), 10);
    assert_eq!(session_org(&h, &session).await["monthly_price_usd"], 100);
    // More members change nothing, and nothing is sent to Stripe.
    let sent = stripe.subscription_requests().len();
    h.fake.add_org_members(&h.org, 1);
    membership_changed(&h, "member_added", "add-3").await;
    assert_eq!(quantity(), 10);
    assert_eq!(stripe.subscription_requests().len(), sent);

    // Back to 5 members: free, and the subscription costs nothing until it grows again.
    for n in 0..11 {
        h.fake.remove_org_members(&h.org, 1);
        membership_changed(&h, "member_removed", &format!("remove-{n}")).await;
    }
    assert_eq!(quantity(), 0);
    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "free");
    assert_eq!(org["monthly_price_usd"], 0);
}

#[tokio::test]
async fn the_periodic_refresh_starts_the_trial_too() {
    let (h, stripe) = start().await;
    setup_large(&h).await;
    let tenant = h.state.tenants.by_org(h.org.id).unwrap();
    h.state.start_trial_if_grown(&tenant).await;
    assert_eq!(stripe.subscriptions(h.org.id)[0]["status"], "trialing");
    h.state.start_trial_if_grown(&tenant).await;
    assert_eq!(stripe.subscriptions(h.org.id).len(), 1);
}

#[tokio::test]
async fn shrinking_to_the_limit_makes_the_organisation_free() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup(&h).await;
    h.fake.add_org_members(&h.org, 5);
    // A trial long over.
    let id = stripe.add_subscription(h.org.id, "acme", "canceled");
    stripe.update_subscription(&id, json!({ "ended_at": now() - 60 * DAY }));
    h.state.billing.load().await.unwrap();
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        402
    );

    h.fake.remove_org_members(&h.org, 1);
    membership_changed(&h, "member_removed", "d-1").await;
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    publish_warnings(&h, repo, "0.1.0").await;
    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 5);
    assert_eq!(org["plan"], "free");
    // The old subscription is still reported, so the website can tell what happened.
    assert_eq!(org["tenant"]["status"], "canceled");
    // Nothing was started for it.
    assert_eq!(stripe.subscriptions(h.org.id).len(), 1);
}

#[tokio::test]
async fn a_returning_organisation_subscribes_through_checkout() {
    let (h, stripe) = start().await;
    let (session, repo, reader) = setup_large(&h).await;
    let previous = stripe.add_subscription(h.org.id, "acme", "canceled");
    stripe.update_subscription(&previous, json!({ "ended_at": now() - 60 * DAY }));
    h.state.billing.load().await.unwrap();

    let response = post(&h, &session, "checkout").await;
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
    // The members past the free 5: 12 members pay for 7.
    assert_eq!(checkout["line_items[0][quantity]"], "7");
    assert_eq!(checkout["customer"], format!("cus_{}", h.org.id));
    assert!(!checkout.contains_key("subscription_data[trial_period_days]"));
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

    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "paid");
    assert_eq!(org["has_payment_method"], true);
    let tenant = &org["tenant"];
    assert_eq!(tenant["status"], "active");
    assert_eq!(tenant["trial_ends_at"], Value::Null);
    assert!(
        tenant["current_period_end"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    publish_warnings(&h, repo, "0.1.0").await;

    let response = post(&h, &session, "checkout").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::already_subscribed");
}

#[tokio::test]
async fn billing_needs_an_admin_and_a_subscription_for_the_portal() {
    let (h, _stripe) = start().await;
    let member = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&member, &h.org, "member");
    let member = h.sign_in(&member).await;
    for action in ["trial", "checkout", "portal"] {
        let response = post(&h, &member, action).await;
        assert_eq!(response.status(), 403);
        assert_eq!(error_code(response).await, "account::admin_required");
    }
    let (admin, _, _) = setup(&h).await;
    let response = post(&h, &admin, "portal").await;
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "billing::no_subscription");
    // The website sends `{}` to sign out too.
    let response = h
        .api_post("/auth/logout", &admin)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
}

#[tokio::test]
async fn webhooks_must_be_signed_and_fresh() {
    let (h, stripe) = start().await;
    let (_, _, reader) = setup_large(&h).await;
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
    let (_, repo, reader) = setup_large(&h).await;
    let id = stripe.add_subscription(h.org.id, "acme", "active");
    h.state.billing.load().await.unwrap();
    publish_warnings(&h, repo, "0.1.0").await;

    // Past due: Stripe is retrying the payment, so everything keeps working.
    stripe.update_subscription(&id, json!({ "status": "past_due" }));
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    publish_warnings(&h, repo, "0.2.0").await;

    // Cancelled yesterday: publishing stops at once, reads continue.
    stripe.update_subscription(
        &id,
        json!({ "status": "canceled", "ended_at": now() - DAY }),
    );
    send_event(&h, "customer.subscription.deleted", json!({ "id": id })).await;
    let response = publish(&h, repo, "0.3.0").await;
    assert_eq!(response.status(), 402);
    let detail = error_detail(response).await;
    assert!(detail.contains(&h.apex("/account")), "{detail}");
    assert!(detail.contains("acme"), "{detail}");
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    let download = h
        .get("/api/v1/crates/story_engine/0.1.0/download", Some(&reader))
        .await;
    assert_eq!(download.status(), 302);

    // Fifteen days after it ended, reads stop too.
    stripe.update_subscription(&id, json!({ "ended_at": now() - 15 * DAY }));
    send_event(&h, "customer.subscription.updated", json!({ "id": id })).await;
    let response = h.get("/index/config.json", Some(&reader)).await;
    assert_eq!(response.status(), 402);
}

#[tokio::test]
async fn start_up_keeps_the_latest_subscription_per_organisation() {
    let (h, stripe) = start().await;
    let (session, _, reader) = setup_large(&h).await;
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
    let org = session_org(&h, &session).await;
    assert_eq!(org["plan"], "paid");
    assert_eq!(org["tenant"]["status"], "active");
    assert!(org["tenant"]["current_period_end"].as_str().is_some());
}

#[tokio::test]
async fn without_stripe_plans_are_still_computed() {
    let h = Harness::start_with(Options {
        preview: false,
        ..Options::default()
    })
    .await;
    let (session, repo, reader) = setup_large(&h).await;
    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 12);
    assert_eq!(org["plan"], "inactive");
    assert_eq!(org["trial_available"], false);
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "todo");
    assert_eq!(
        step["detail"],
        "12 members: billing is not set up on this server."
    );
    // Nothing is enforced without billing.
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    publish_warnings(&h, repo, "0.1.0").await;
    let response = post(&h, &session, "trial").await;
    assert_eq!(response.status(), 503);
    assert_eq!(error_code(response).await, "billing::not_configured");
}

#[tokio::test]
async fn without_stripe_a_small_organisation_is_free() {
    let h = Harness::start_with(Options {
        preview: false,
        ..Options::default()
    })
    .await;
    let (session, _, _) = setup(&h).await;
    let org = session_org(&h, &session).await;
    assert_eq!(org["members"], 1);
    assert_eq!(org["plan"], "free");
    assert_eq!(org["trial_available"], false);
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "done");
    assert_eq!(step["detail"], "Free: 1 of 5 members");
}

/// The preview (docs/preview.md §1): free for everyone, and Stripe is never called, even when it is configured.
#[tokio::test]
async fn the_preview_is_free_and_never_calls_stripe() {
    let h = Harness::start_with(Options {
        stripe: true,
        ..Options::default()
    })
    .await;
    let stripe = h.stripe.clone().unwrap();
    let (session, repo, reader) = setup_large(&h).await;
    // A lapsed subscription in Stripe changes nothing.
    stripe.add_subscription(h.org.id, "acme", "canceled");

    // Over the member limit, reads and publishes work, with no trial reminder.
    assert_eq!(
        h.get("/index/config.json", Some(&reader)).await.status(),
        200
    );
    assert!(publish_warnings(&h, repo, "0.1.0").await.is_empty());

    let doc: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(doc["preview"], true);
    assert_eq!(
        doc["terms"],
        json!({ "version": TERMS_VERSION, "url": h.apex("/legal/terms") })
    );
    let org = &doc["orgs"][0];
    assert_eq!(org["members"], 12);
    assert_eq!(org["plan"], "free");
    assert_eq!(org["trial_available"], false);
    assert_eq!(org["trial_ends_at"], Value::Null);
    assert_eq!(org["tenant"]["status"], Value::Null);
    let step = plan_step(&h, &session).await;
    assert_eq!(step["status"], "done");
    assert_eq!(step["detail"], "Free during the preview.");

    for action in ["trial", "checkout", "portal", "billing-email"] {
        let response = post(&h, &session, action).await;
        assert_eq!(response.status(), 409, "{action}");
        assert_eq!(error_code(response).await, "billing::preview", "{action}");
    }
    // Before the billing email is even checked.
    let response = h
        .api_post("/api/orgs/acme/trial", &session)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(error_code(response).await, "billing::preview");

    // Growing past the limit starts no trial, and the refresh lists nothing.
    membership_changed(&h, "member_added", "preview-1").await;
    h.state.billing.load().await.unwrap();
    for tenant in h.state.tenants.all() {
        h.state.start_trial_if_grown(&tenant).await;
    }
    // Stripe's webhooks are not served.
    let (body, signature) = stripe::webhook(
        "customer.subscription.updated",
        json!({ "id": "sub_1" }),
        now(),
    );
    assert_eq!(deliver(&h, &body, Some(&signature)).await.status(), 404);

    assert_eq!(stripe.requests(), 0);
}
