//! An in-process fake of the few Stripe endpoints PrivateCrates uses, for tests: Checkout and customer portal
//! sessions, subscriptions, and signed webhook deliveries.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

use crate::now;

pub const SECRET_KEY: &str = "sk_test_fake";
pub const WEBHOOK_SECRET: &str = "whsec_fake";
pub const PRICE_ID: &str = "price_fake";

#[derive(Default)]
struct World {
    next_id: u64,
    /// In creation order.
    subscriptions: Vec<Value>,
    checkouts: Vec<HashMap<String, String>>,
    portals: Vec<HashMap<String, String>>,
}

impl World {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

#[derive(Clone)]
pub struct FakeStripe {
    world: Arc<Mutex<World>>,
    pub url: String,
}

impl FakeStripe {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("bind the fake Stripe");
        let url = format!("http://{}", listener.local_addr().expect("local address"));
        let fake = Self {
            world: Arc::default(),
            url,
        };
        let app = router(fake.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        fake
    }

    fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().expect("fake Stripe lock")
    }

    /// Adds a subscription for an organisation, with a period ending in 30 days; returns its ID.
    pub fn add_subscription(&self, org_id: u64, org_login: &str, status: &str) -> String {
        let mut w = self.world();
        let n = w.id();
        let id = format!("sub_{n}");
        let now = now();
        w.subscriptions.push(json!({
            "id": id,
            "object": "subscription",
            "customer": format!("cus_{org_id}"),
            "status": status,
            // Later subscriptions are newer, even within one second.
            "created": now + n,
            "trial_end": (status == "trialing").then_some(now + 14 * 24 * 3600),
            "ended_at": null,
            "items": { "data": [{ "current_period_end": now + 30 * 24 * 3600 }] },
            "metadata": { "github_org_id": org_id.to_string(), "github_org_login": org_login },
        }));
        id
    }

    /// Changes a subscription's fields, as Stripe would on a payment, a cancellation or a period's end.
    pub fn update_subscription(&self, id: &str, fields: Value) {
        let mut w = self.world();
        let subscription = w
            .subscriptions
            .iter_mut()
            .find(|s| s["id"] == id)
            .expect("known subscription");
        for (key, value) in fields.as_object().expect("an object") {
            subscription[key] = value.clone();
        }
    }

    /// The form parameters of every Checkout session created.
    pub fn checkouts(&self) -> Vec<HashMap<String, String>> {
        self.world().checkouts.clone()
    }

    pub fn portals(&self) -> Vec<HashMap<String, String>> {
        self.world().portals.clone()
    }

    /// Completes the latest Checkout session as the customer would: creates the trialing subscription with the
    /// session's subscription metadata. Returns the webhook event's object for `checkout.session.completed`.
    pub fn complete_checkout(&self) -> Value {
        let checkout = self.checkouts().pop().expect("a checkout session");
        let org_id: u64 = checkout["subscription_data[metadata][github_org_id]"]
            .parse()
            .expect("org ID metadata");
        let status = if checkout.contains_key("subscription_data[trial_period_days]") {
            "trialing"
        } else {
            "active"
        };
        let id = self.add_subscription(
            org_id,
            &checkout["subscription_data[metadata][github_org_login]"],
            status,
        );
        json!({ "id": "cs_test", "object": "checkout.session", "subscription": id })
    }
}

/// A webhook delivery: the event body and its `Stripe-Signature` header, signed at `timestamp`.
pub fn webhook(event_type: &str, object: Value, timestamp: u64) -> (String, String) {
    let body = json!({
        "id": format!("evt_{timestamp}"),
        "object": "event",
        "type": event_type,
        "data": { "object": object },
    })
    .to_string();
    let signature = sign_webhook(WEBHOOK_SECRET, timestamp, &body);
    (body, signature)
}

/// A `Stripe-Signature` header for `body`.
pub fn sign_webhook(secret: &str, timestamp: u64, body: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(format!("{timestamp}.{body}").as_bytes());
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

fn router(fake: FakeStripe) -> Router {
    Router::new()
        .route("/v1/subscriptions", get(list_subscriptions))
        .route("/v1/subscriptions/{id}", get(subscription))
        .route("/v1/checkout/sessions", post(create_checkout))
        .route("/v1/billing_portal/sessions", post(create_portal))
        .with_state(fake)
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

fn authorised(headers: &HeaderMap) -> Result<(), Response> {
    let expected = format!("Bearer {SECRET_KEY}");
    if headers
        .get(header::AUTHORIZATION)
        .is_some_and(|v| v.as_bytes() == expected.as_bytes())
    {
        Ok(())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "Invalid API Key provided"))
    }
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    limit: Option<usize>,
    starting_after: Option<String>,
}

/// Newest first, like Stripe. Without `status=all`, cancelled subscriptions are left out, like Stripe.
async fn list_subscriptions(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let w = fake.world();
    let all = query.status.as_deref() == Some("all");
    let newest_first: Vec<&Value> = w
        .subscriptions
        .iter()
        .rev()
        .filter(|s| all || s["status"] != "canceled")
        .collect();
    let start = match &query.starting_after {
        Some(after) => match newest_first.iter().position(|s| s["id"] == after.as_str()) {
            Some(i) => i + 1,
            None => return error(StatusCode::BAD_REQUEST, "No such subscription"),
        },
        None => 0,
    };
    let limit = query.limit.unwrap_or(10);
    let data: Vec<&Value> = newest_first
        .iter()
        .skip(start)
        .take(limit)
        .copied()
        .collect();
    Json(json!({
        "object": "list",
        "data": data,
        "has_more": start + limit < newest_first.len(),
    }))
    .into_response()
}

async fn subscription(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    match fake.world().subscriptions.iter().find(|s| s["id"] == id) {
        Some(s) => Json(s.clone()).into_response(),
        None => error(StatusCode::NOT_FOUND, "No such subscription"),
    }
}

async fn create_checkout(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let mut w = fake.world();
    let id = format!("cs_test_{}", w.id());
    w.checkouts.push(form);
    Json(json!({ "id": id, "url": format!("https://checkout.stripe.com/c/pay/{id}") }))
        .into_response()
}

async fn create_portal(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let mut w = fake.world();
    let known = form
        .get("customer")
        .is_some_and(|c| w.subscriptions.iter().any(|s| s["customer"] == c.as_str()));
    if !known {
        return error(StatusCode::BAD_REQUEST, "No such customer");
    }
    let id = format!("bps_{}", w.id());
    w.portals.push(form);
    Json(json!({ "id": id, "url": format!("https://billing.stripe.com/p/session/{id}") }))
        .into_response()
}
