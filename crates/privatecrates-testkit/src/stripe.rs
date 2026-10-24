//! An in-process fake of the few Stripe endpoints PrivateCrates uses, for tests: customers and subscriptions
//! created directly (with idempotency keys), Checkout and customer portal sessions, and signed webhook deliveries.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
};

use axum::{
    Form, Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
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
/// The API version every request must pin.
pub const API_VERSION: &str = "2026-08-26.dahlia";

const DAY: u64 = 24 * 60 * 60;

/// What Stripe keeps under an idempotency key: the request's parameters, and the status and body it got.
type Stored = (HashMap<String, String>, StatusCode, Value);

#[derive(Default)]
struct World {
    next_id: u64,
    /// How many requests the fake received, of any kind.
    requests: usize,
    /// How many of the next requests fail as an outage would, with a 500, before Stripe handles them.
    failing: usize,
    /// When set, only requests to this path count towards `failing`.
    failing_path: Option<String>,
    /// Customers created while search lags, which search does not find yet.
    unindexed: Vec<String>,
    search_lags: bool,
    customers: Vec<Value>,
    /// In creation order; `customer` is the customer's ID.
    subscriptions: Vec<Value>,
    /// The form parameters of each subscription created through the API.
    subscription_requests: Vec<HashMap<String, String>>,
    checkouts: Vec<HashMap<String, String>>,
    portals: Vec<HashMap<String, String>>,
    /// (path, Idempotency-Key) → the request's parameters and the response it got, a failure as much as a success.
    idempotent: HashMap<(String, String), Stored>,
}

impl World {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn customer(&self, id: &str) -> Option<&Value> {
        self.customers.iter().find(|c| c["id"] == id)
    }

    fn subscription_mut(&mut self, id: &str) -> &mut Value {
        self.subscriptions
            .iter_mut()
            .find(|s| s["id"] == id)
            .expect("known subscription")
    }

    /// A subscription as the API returns it, with its customer expanded if asked.
    fn render(&self, subscription: &Value, expand_customer: bool) -> Value {
        let mut subscription = subscription.clone();
        if expand_customer
            && let Some(customer) = self.customer(subscription["customer"].as_str().unwrap_or(""))
        {
            subscription["customer"] = customer.clone();
        }
        subscription
    }
}

fn customer_json(id: &str, metadata: Value) -> Value {
    json!({
        "id": id,
        "object": "customer",
        "email": null,
        "metadata": metadata,
        "invoice_settings": { "default_payment_method": null },
        "default_source": null,
    })
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

    /// Adds a subscription for an organisation, with a period ending in 30 days; returns its ID. The organisation's
    /// customer is `cus_{org_id}`.
    pub fn add_subscription(&self, org_id: u64, org_login: &str, status: &str) -> String {
        let mut w = self.world();
        let n = w.id();
        let id = format!("sub_{n}");
        let customer = format!("cus_{org_id}");
        let metadata =
            json!({ "github_org_id": org_id.to_string(), "github_org_login": org_login });
        if w.customer(&customer).is_none() {
            w.customers.push(customer_json(&customer, metadata.clone()));
        }
        let now = now();
        w.subscriptions.push(json!({
            "id": id,
            "object": "subscription",
            "customer": customer,
            "status": status,
            // Later subscriptions are newer, even within one second.
            "created": now + n,
            "trial_end": (status == "trialing").then_some(now + 14 * DAY),
            "ended_at": null,
            "default_payment_method": null,
            "items": { "data": [{ "id": format!("si_{n}"), "quantity": 1, "current_period_end": now + 30 * DAY }] },
            "metadata": metadata,
        }));
        id
    }

    /// Changes a subscription's fields, as Stripe would on a payment, a cancellation or a period's end.
    pub fn update_subscription(&self, id: &str, fields: Value) {
        let mut w = self.world();
        let subscription = w.subscription_mut(id);
        for (key, value) in fields.as_object().expect("an object") {
            subscription[key] = value.clone();
        }
    }

    /// Adds a card as the customer portal does: the customer's default payment method.
    pub fn add_card(&self, subscription: &str) {
        let mut w = self.world();
        let n = w.id();
        let customer = w.subscription_mut(subscription)["customer"].clone();
        let customer = w
            .customers
            .iter_mut()
            .find(|c| c["id"] == customer)
            .expect("known customer");
        customer["invoice_settings"]["default_payment_method"] = json!(format!("pm_{n}"));
    }

    /// Ends a subscription's trial now, as Stripe does at `trial_end`: with a payment method it becomes active;
    /// without one, a subscription created with `missing_payment_method=cancel` is cancelled.
    pub fn end_trial(&self, id: &str) {
        let w = self.world();
        let subscription = w
            .subscriptions
            .iter()
            .find(|s| s["id"] == id)
            .expect("known subscription");
        let customer = w
            .customer(subscription["customer"].as_str().unwrap_or(""))
            .expect("known customer");
        let has_payment_method = !subscription["default_payment_method"].is_null()
            || !customer["invoice_settings"]["default_payment_method"].is_null()
            || !customer["default_source"].is_null();
        let now = now();
        let fields = if has_payment_method {
            json!({ "status": "active", "trial_end": now, "items": { "data": [{ "current_period_end": now + 30 * DAY }] } })
        } else {
            json!({ "status": "canceled", "trial_end": now, "ended_at": now, "canceled_at": now })
        };
        drop(w);
        self.update_subscription(id, fields);
    }

    /// Every subscription of an organisation, oldest first.
    pub fn subscriptions(&self, org_id: u64) -> Vec<Value> {
        let org_id = org_id.to_string();
        self.world()
            .subscriptions
            .iter()
            .filter(|s| s["metadata"]["github_org_id"] == org_id.as_str())
            .cloned()
            .collect()
    }

    /// Makes the next `n` requests fail with a 500, as during a Stripe outage.
    pub fn fail_next(&self, n: usize) {
        let mut w = self.world();
        w.failing = n;
        w.failing_path = None;
    }

    /// Makes the next `n` requests to `path` fail with a 500 before Stripe handles them, so that nothing is stored
    /// under their idempotency keys; other requests go through.
    pub fn fail_next_on(&self, path: &str, n: usize) {
        let mut w = self.world();
        w.failing = n;
        w.failing_path = Some(path.to_owned());
    }

    /// Makes customer search miss the customers created from now on until [`Self::search_catches_up`], as Stripe's
    /// search does for a while after a write.
    pub fn search_lags(&self) {
        self.world().search_lags = true;
    }

    pub fn search_catches_up(&self) {
        let mut w = self.world();
        w.search_lags = false;
        w.unindexed.clear();
    }

    /// Forgets every idempotency key, as Stripe does after 24 hours.
    pub fn forget_idempotency_keys(&self) {
        self.world().idempotent.clear();
    }

    /// How many requests reached the fake.
    pub fn requests(&self) -> usize {
        self.world().requests
    }

    pub fn customers(&self) -> Vec<Value> {
        self.world().customers.clone()
    }

    /// The form parameters of every subscription created through the API.
    pub fn subscription_requests(&self) -> Vec<HashMap<String, String>> {
        self.world().subscription_requests.clone()
    }

    /// The form parameters of every Checkout session created.
    pub fn checkouts(&self) -> Vec<HashMap<String, String>> {
        self.world().checkouts.clone()
    }

    pub fn portals(&self) -> Vec<HashMap<String, String>> {
        self.world().portals.clone()
    }

    /// Completes the latest Checkout session as the customer would, card included: creates the active
    /// subscription with the session's subscription metadata. Returns the webhook event's object for
    /// `checkout.session.completed`.
    pub fn complete_checkout(&self) -> Value {
        let checkout = self.checkouts().pop().expect("a checkout session");
        let org_id: u64 = checkout["subscription_data[metadata][github_org_id]"]
            .parse()
            .expect("org ID metadata");
        let id = self.add_subscription(
            org_id,
            &checkout["subscription_data[metadata][github_org_login]"],
            "active",
        );
        let n = self.world().id();
        self.update_subscription(&id, json!({ "default_payment_method": format!("pm_{n}") }));
        let quantity: u64 = checkout["line_items[0][quantity]"]
            .parse()
            .expect("a quantity");
        let mut w = self.world();
        w.subscription_mut(&id)["items"]["data"][0]["quantity"] = json!(quantity);
        drop(w);
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
        .route("/v1/customers", post(create_customer))
        .route("/v1/customers/search", get(search_customers))
        .route("/v1/customers/{id}", post(update_customer))
        .route(
            "/v1/subscriptions",
            get(list_subscriptions).post(create_subscription),
        )
        .route(
            "/v1/subscriptions/{id}",
            get(subscription).post(update_subscription_quantity),
        )
        .route("/v1/checkout/sessions", post(create_checkout))
        .route("/v1/billing_portal/sessions", post(create_portal))
        .layer(middleware::from_fn_with_state(fake.clone(), count))
        .with_state(fake)
}

async fn count(State(fake): State<FakeStripe>, request: Request, next: Next) -> Response {
    let failing = {
        let mut w = fake.world();
        w.requests += 1;
        let counts = w
            .failing_path
            .as_deref()
            .is_none_or(|path| path == request.uri().path());
        let failing = counts && w.failing > 0;
        if counts {
            w.failing = w.failing.saturating_sub(1);
        }
        failing
    };
    if failing {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the fake is failing, as asked",
        );
    }
    next.run(request).await
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": { "message": message } }))).into_response()
}

fn authorised(headers: &HeaderMap) -> Result<(), Response> {
    let expected = format!("Bearer {SECRET_KEY}");
    if !headers
        .get(header::AUTHORIZATION)
        .is_some_and(|v| v.as_bytes() == expected.as_bytes())
    {
        return Err(error(StatusCode::UNAUTHORIZED, "Invalid API Key provided"));
    }
    if !headers
        .get("stripe-version")
        .is_some_and(|v| v.as_bytes() == API_VERSION.as_bytes())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "the fake expects the pinned Stripe-Version",
        ));
    }
    Ok(())
}

/// Runs a creation once per `Idempotency-Key`, as Stripe does: a repeat with the same parameters gets the first
/// response, a failure as much as a success, and one with other parameters is refused.
fn idempotent(
    fake: &FakeStripe,
    path: &str,
    headers: &HeaderMap,
    form: HashMap<String, String>,
    create: impl FnOnce(
        &mut World,
        &HashMap<String, String>,
    ) -> Result<Value, (StatusCode, &'static str)>,
) -> Response {
    if let Err(e) = authorised(headers) {
        return e;
    }
    let mut w = fake.world();
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|k| (path.to_owned(), k.to_owned()));
    if let Some((first, status, response)) = key.as_ref().and_then(|k| w.idempotent.get(k)) {
        return if *first == form {
            (*status, Json(response.clone())).into_response()
        } else {
            error(
                StatusCode::BAD_REQUEST,
                "Keys for idempotent requests can only be used with the same parameters they were first used with",
            )
        };
    }
    let (status, response) = match create(&mut w, &form) {
        Ok(response) => (StatusCode::OK, response),
        Err((status, message)) => (status, json!({ "error": { "message": message } })),
    };
    if let Some(key) = key {
        w.idempotent.insert(key, (form, status, response.clone()));
    }
    (status, Json(response)).into_response()
}

/// `metadata[key]=value` form fields as an object.
fn metadata(form: &HashMap<String, String>) -> Value {
    form.iter()
        .filter_map(|(k, v)| {
            let key = k.strip_prefix("metadata[")?.strip_suffix(']')?;
            Some((key.to_owned(), Value::String(v.clone())))
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

async fn create_customer(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    idempotent(&fake, "/v1/customers", &headers, form, |w, form| {
        let id = format!("cus_new{}", w.id());
        let mut customer = customer_json(&id, metadata(form));
        customer["name"] = json!(form.get("name"));
        customer["email"] = json!(form.get("email"));
        if w.search_lags {
            w.unindexed.push(id);
        }
        w.customers.push(customer.clone());
        Ok(customer)
    })
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
}

/// Customer search, for the one query PrivateCrates makes: `metadata['key']:'value'`.
async fn search_customers(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Query(search): Query<SearchQuery>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let Some((key, value)) = search
        .query
        .strip_prefix("metadata['")
        .and_then(|q| q.split_once("']:'"))
        .and_then(|(key, value)| Some((key, value.strip_suffix('\'')?)))
    else {
        return error(
            StatusCode::BAD_REQUEST,
            "the fake searches by one metadata key",
        );
    };
    let w = fake.world();
    let data: Vec<Value> = w
        .customers
        .iter()
        .filter(|c| {
            c["metadata"][key] == value && !w.unindexed.iter().any(|id| c["id"] == id.as_str())
        })
        .cloned()
        .collect();
    Json(json!({ "object": "search_result", "data": data, "has_more": false })).into_response()
}

/// Updates a customer: only its email, which is all PrivateCrates changes.
async fn update_customer(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let mut w = fake.world();
    let Some(customer) = w.customers.iter_mut().find(|c| c["id"] == id.as_str()) else {
        return error(StatusCode::NOT_FOUND, "No such customer");
    };
    if let Some(email) = form.get("email") {
        customer["email"] = json!(email);
    }
    // Stripe merges metadata keys into what is there.
    if let Value::Object(update) = metadata(&form) {
        for (key, value) in update {
            customer["metadata"][key] = value;
        }
    }
    Json(customer.clone()).into_response()
}

async fn create_subscription(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    idempotent(&fake, "/v1/subscriptions", &headers, form, |w, form| {
        let customer = form.get("customer").map(String::as_str).unwrap_or_default();
        if w.customer(customer).is_none() {
            return Err((StatusCode::BAD_REQUEST, "No such customer"));
        }
        if form.get("items[0][price]").map(String::as_str) != Some(PRICE_ID) {
            return Err((StatusCode::BAD_REQUEST, "No such price"));
        }
        let trial_days: Option<u64> = form.get("trial_period_days").and_then(|d| d.parse().ok());
        let n = w.id();
        let now = now();
        let trial_end = trial_days.map(|days| now + days * DAY);
        let subscription = json!({
            "id": format!("sub_{n}"),
            "object": "subscription",
            "customer": customer,
            // A subscription without a trial would need a payment method first, which the fake does not model.
            "status": if trial_end.is_some() { "trialing" } else { "incomplete" },
            "created": now + n,
            "trial_end": trial_end,
            "ended_at": null,
            "default_payment_method": null,
            "items": { "data": [{
                "id": format!("si_{n}"),
                "quantity": form.get("items[0][quantity]").and_then(|q| q.parse::<u64>().ok()).unwrap_or(1),
                "current_period_end": trial_end.unwrap_or(now),
            }] },
            "metadata": metadata(form),
        });
        w.subscriptions.push(subscription.clone());
        w.subscription_requests.push(form.clone());
        let expand = form.get("expand[]").is_some_and(|e| e == "customer");
        Ok(w.render(&subscription, expand))
    })
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    limit: Option<usize>,
    starting_after: Option<String>,
    #[serde(rename = "expand[]")]
    expand: Option<String>,
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
    let expand = query.expand.as_deref() == Some("data.customer");
    let data: Vec<Value> = newest_first
        .iter()
        .skip(start)
        .take(limit)
        .map(|s| w.render(s, expand))
        .collect();
    Json(json!({
        "object": "list",
        "data": data,
        "has_more": start + limit < newest_first.len(),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct RetrieveQuery {
    #[serde(rename = "expand[]")]
    expand: Option<String>,
}

async fn subscription(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<RetrieveQuery>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let w = fake.world();
    let expand = query.expand.as_deref() == Some("customer");
    match w.subscriptions.iter().find(|s| s["id"] == id) {
        Some(s) => Json(w.render(s, expand)).into_response(),
        None => error(StatusCode::NOT_FOUND, "No such subscription"),
    }
}

/// `POST /v1/subscriptions/{id}`: changes the quantity of the subscription's item, the one update we make.
async fn update_subscription_quantity(
    State(fake): State<FakeStripe>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if let Err(e) = authorised(&headers) {
        return e;
    }
    let mut w = fake.world();
    let Some(index) = w.subscriptions.iter().position(|s| s["id"] == id) else {
        return error(StatusCode::NOT_FOUND, "No such subscription");
    };
    let item = &w.subscriptions[index]["items"]["data"][0]["id"];
    if form.get("items[0][id]").map(String::as_str) != item.as_str() {
        return error(StatusCode::BAD_REQUEST, "No such subscription item");
    }
    let Some(quantity) = form
        .get("items[0][quantity]")
        .and_then(|q| q.parse::<u64>().ok())
    else {
        return error(StatusCode::BAD_REQUEST, "Invalid quantity");
    };
    w.subscriptions[index]["items"]["data"][0]["quantity"] = json!(quantity);
    w.subscription_requests.push(form.clone());
    let expand = form.get("expand[]").is_some_and(|e| e == "customer");
    let rendered = w.render(&w.subscriptions[index], expand);
    Json(rendered).into_response()
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
        .is_some_and(|c| w.customer(c).is_some());
    if !known {
        return error(StatusCode::BAD_REQUEST, "No such customer");
    }
    let id = format!("bps_{}", w.id());
    w.portals.push(form);
    Json(json!({ "id": id, "url": format!("https://billing.stripe.com/p/session/{id}") }))
        .into_response()
}
