//! Billing with Stripe (docs/website-api.md): $100 per organisation per month, with a 14-day trial.
//!
//! Stripe is the source of truth and there is no database. Each subscription's metadata names its GitHub
//! organisation; subscriptions are listed at start-up and kept current by Stripe's webhooks, with a periodic reload
//! as the backstop. Without Stripe configured (tests, local development), every tenant is active.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use apollo_errors::Error;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use bytes::Bytes;
use hmac::{Hmac, KeyInit, Mac};
use miette::Diagnostic;
use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use sha2::Sha256;

use crate::{AppState, config::StripeConfig, error::ApiError, github::now_secs};

/// Reads keep working this long after a subscription's last paid period ends.
pub const READ_GRACE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
pub const TRIAL_DAYS: u32 = 14;
/// How often subscriptions are listed again, in case a webhook was missed.
pub const REFRESH: Duration = Duration::from_secs(10 * 60);
/// How far a webhook's timestamp may be from our clock, against replays (Stripe's own libraries use 5 minutes).
const WEBHOOK_TOLERANCE_SECS: u64 = 5 * 60;
const ORG_ID_KEY: &str = "github_org_id";

#[derive(Debug, Error, Diagnostic)]
pub enum BillingError {
    #[error("request to Stripe failed: {source}")]
    #[diagnostic(code(billing::http))]
    Http {
        #[from]
        source: reqwest::Error,
    },
    #[error("Stripe answered {status}: {body}")]
    #[diagnostic(code(billing::status))]
    Status {
        status: reqwest::StatusCode,
        body: String,
    },
}

/// A Stripe subscription: only the fields we use.
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub customer: String,
    /// `trialing`, `active`, `past_due`, `canceled`, `unpaid`, `incomplete`, `incomplete_expired` or `paused`.
    pub status: String,
    pub created: u64,
    #[serde(default)]
    pub trial_end: Option<u64>,
    /// On the subscription in older Stripe API versions, on its items in newer ones.
    #[serde(default)]
    current_period_end: Option<u64>,
    #[serde(default)]
    items: Items,
    #[serde(default)]
    pub ended_at: Option<u64>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Items {
    data: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
struct Item {
    #[serde(default)]
    current_period_end: Option<u64>,
}

impl Subscription {
    fn org_id(&self) -> Option<u64> {
        self.metadata.get(ORG_ID_KEY)?.parse().ok()
    }

    /// `past_due` keeps working while Stripe retries the payment.
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "trialing" | "active" | "past_due")
    }

    pub fn current_period_end(&self) -> Option<u64> {
        self.current_period_end.or_else(|| {
            self.items
                .data
                .iter()
                .filter_map(|i| i.current_period_end)
                .max()
        })
    }

    /// When the service stopped being paid for, from which the read grace period runs.
    fn paid_until(&self) -> Option<u64> {
        self.ended_at
            .or_else(|| self.current_period_end())
            .or(self.trial_end)
    }
}

/// What a tenant may do, given its subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Active,
    /// Lapsed recently: reads only.
    Grace,
    Lapsed,
}

fn standing(subscription: Option<&Subscription>, now: u64) -> Standing {
    let Some(subscription) = subscription else {
        return Standing::Lapsed;
    };
    if subscription.is_active() {
        return Standing::Active;
    }
    match subscription.paid_until() {
        Some(until) if now < until + READ_GRACE.as_secs() => Standing::Grace,
        _ => Standing::Lapsed,
    }
}

/// Keeps the newest subscription per organisation: an organisation that cancelled and subscribed again has two.
fn keep_latest(by_org: &mut HashMap<u64, Subscription>, subscription: Subscription) {
    let Some(org_id) = subscription.org_id() else {
        return;
    };
    let newer = by_org
        .get(&org_id)
        .is_none_or(|s| s.id == subscription.id || s.created <= subscription.created);
    if newer {
        by_org.insert(org_id, subscription);
    }
}

/// Checks a `Stripe-Signature` header: `t={timestamp},v1={hex HMAC-SHA256 of "{timestamp}.{body}"},…`.
fn signature_is_valid(secret: &[u8], header: &str, body: &[u8], now: u64) -> bool {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", t)) => timestamp = Some(t),
            Some(("v1", signature)) => signatures.extend(hex::decode(signature).ok()),
            _ => {}
        }
    }
    let Some(timestamp) = timestamp else {
        return false;
    };
    let fresh = timestamp
        .parse::<u64>()
        .is_ok_and(|t| t.abs_diff(now) <= WEBHOOK_TOLERANCE_SECS);
    fresh
        && signatures.iter().any(|signature| {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
            mac.update(timestamp.as_bytes());
            mac.update(b".");
            mac.update(body);
            // Constant-time comparison.
            mac.verify_slice(signature).is_ok()
        })
}

struct Stripe {
    http: reqwest::Client,
    config: StripeConfig,
}

impl Stripe {
    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        form: &[(&str, &str)],
    ) -> Result<T, BillingError> {
        let url = self
            .config
            .api
            .join(path.trim_start_matches('/'))
            .expect("API paths are valid URL paths");
        let mut request = self
            .http
            .request(method.clone(), url)
            .bearer_auth(&self.config.secret_key);
        request = if method == Method::GET {
            request.query(form)
        } else {
            request.form(form)
        };
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(BillingError::Status {
                status,
                body: body.chars().take(500).collect(),
            });
        }
        Ok(response.json().await?)
    }

    async fn subscription(&self, id: &str) -> Result<Subscription, BillingError> {
        // Subscription IDs come from signed webhooks, but keep them from reshaping the URL all the same.
        let id: String = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        self.call(Method::GET, &format!("/v1/subscriptions/{id}"), &[])
            .await
    }
}

#[derive(Deserialize)]
struct List<T> {
    data: Vec<T>,
    has_more: bool,
}

#[derive(Deserialize)]
struct Url {
    url: String,
}

pub struct Billing {
    stripe: Option<Stripe>,
    /// GitHub organisation ID → its latest subscription.
    subscriptions: RwLock<HashMap<u64, Subscription>>,
}

impl Billing {
    pub fn new(config: Option<StripeConfig>) -> Result<Self, BillingError> {
        let stripe = match config {
            Some(config) => Some(Stripe {
                http: reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()?,
                config,
            }),
            None => {
                tracing::warn!("Stripe is not configured, so every tenant is treated as active");
                None
            }
        };
        Ok(Self {
            stripe,
            subscriptions: RwLock::default(),
        })
    }

    pub fn enabled(&self) -> bool {
        self.stripe.is_some()
    }

    /// Lists every subscription from Stripe and keeps the latest per organisation.
    pub async fn load(&self) -> Result<(), BillingError> {
        let Some(stripe) = &self.stripe else {
            return Ok(());
        };
        let mut by_org = HashMap::new();
        let mut after: Option<String> = None;
        loop {
            let mut query = vec![("status", "all"), ("limit", "100")];
            if let Some(after) = &after {
                query.push(("starting_after", after));
            }
            let page: List<Subscription> = stripe
                .call(Method::GET, "/v1/subscriptions", &query)
                .await?;
            after = page.data.last().map(|s| s.id.clone());
            for subscription in page.data {
                keep_latest(&mut by_org, subscription);
            }
            if !page.has_more || after.is_none() {
                break;
            }
        }
        *self.subscriptions.write().expect("billing lock") = by_org;
        Ok(())
    }

    pub fn subscription(&self, org_id: u64) -> Option<Subscription> {
        self.subscriptions
            .read()
            .expect("billing lock")
            .get(&org_id)
            .cloned()
    }

    pub fn standing(&self, org_id: u64) -> Standing {
        if !self.enabled() {
            return Standing::Active;
        }
        standing(self.subscription(org_id).as_ref(), now_secs())
    }

    fn record(&self, subscription: Subscription) {
        tracing::info!(org_id = ?subscription.org_id(), subscription = %subscription.id, status = %subscription.status, "subscription updated");
        keep_latest(
            &mut self.subscriptions.write().expect("billing lock"),
            subscription,
        );
    }

    fn stripe(&self) -> Result<&Stripe, ApiError> {
        self.stripe.as_ref().ok_or(ApiError::BillingNotConfigured)
    }

    /// A Checkout session for a new subscription; returns the URL to send the admin to. The trial is for an
    /// organisation's first subscription only, and a returning organisation keeps its Stripe customer.
    pub async fn checkout(
        &self,
        org_id: u64,
        org_login: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<String, ApiError> {
        let stripe = self.stripe()?;
        let previous = self.subscription(org_id);
        if previous.as_ref().is_some_and(Subscription::is_active) {
            return Err(ApiError::AlreadySubscribed {
                org: org_login.to_owned(),
            });
        }
        let org_id = org_id.to_string();
        let trial_days = TRIAL_DAYS.to_string();
        let mut form = vec![
            ("mode", "subscription"),
            ("line_items[0][price]", stripe.config.price_id.as_str()),
            ("line_items[0][quantity]", "1"),
            ("client_reference_id", &org_id),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("metadata[github_org_id]", &org_id),
            ("metadata[github_org_login]", org_login),
            ("subscription_data[metadata][github_org_id]", &org_id),
            ("subscription_data[metadata][github_org_login]", org_login),
        ];
        match &previous {
            Some(previous) => form.push(("customer", &previous.customer)),
            None => form.push(("subscription_data[trial_period_days]", &trial_days)),
        }
        let session: Url = stripe
            .call(Method::POST, "/v1/checkout/sessions", &form)
            .await?;
        Ok(session.url)
    }

    /// A customer portal session, where an admin manages payment details or cancels.
    pub async fn portal(
        &self,
        org_id: u64,
        org_login: &str,
        return_url: &str,
    ) -> Result<String, ApiError> {
        let stripe = self.stripe()?;
        let subscription = self
            .subscription(org_id)
            .ok_or_else(|| ApiError::NoSubscription {
                org: org_login.to_owned(),
            })?;
        let session: Url = stripe
            .call(
                Method::POST,
                "/v1/billing_portal/sessions",
                &[
                    ("customer", &subscription.customer),
                    ("return_url", return_url),
                ],
            )
            .await?;
        Ok(session.url)
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/webhooks/stripe", post(webhook))
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    data: EventData,
}

#[derive(Deserialize)]
struct EventData {
    object: serde_json::Value,
}

/// Stripe's webhooks. Events can arrive out of order, so rather than trusting the object in the event, the
/// subscription it names is fetched as it is now.
async fn webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let billing = &state.billing;
    let Some(stripe) = &billing.stripe else {
        return Err(ApiError::NotFound);
    };
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !signature_is_valid(&stripe.config.webhook_secret, signature, &body, now_secs()) {
        return Err(ApiError::StripeWebhookInvalid {
            reason: "bad or stale signature".into(),
        });
    }
    let event: Event =
        serde_json::from_slice(&body).map_err(|e| ApiError::StripeWebhookInvalid {
            reason: e.to_string(),
        })?;
    let object = &event.data.object;
    let subscription = match event.kind.as_str() {
        "checkout.session.completed" => object["subscription"].as_str(),
        "customer.subscription.created"
        | "customer.subscription.updated"
        | "customer.subscription.deleted" => object["id"].as_str(),
        _ => None,
    };
    if let Some(id) = subscription {
        billing.record(stripe.subscription(id).await?);
    }
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(id: &str, status: &str, created: u64) -> Subscription {
        serde_json::from_value(serde_json::json!({
            "id": id, "customer": "cus_1", "status": status, "created": created,
            "items": { "data": [{ "current_period_end": 1000 }] },
            "metadata": { "github_org_id": "100", "github_org_login": "acme" },
        }))
        .unwrap()
    }

    fn sign(secret: &[u8], timestamp: u64, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(format!("{timestamp}.").as_bytes());
        mac.update(body);
        format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn webhook_signatures() {
        let body = b"{\"type\":\"x\"}";
        let now = 1_800_000_000;
        assert!(signature_is_valid(
            b"whsec",
            &sign(b"whsec", now, body),
            body,
            now
        ));
        // Several signatures during a secret rotation: any may match.
        let rotated = format!("{},v1=00ff", sign(b"whsec", now, body));
        assert!(signature_is_valid(b"whsec", &rotated, body, now));
        assert!(!signature_is_valid(
            b"other",
            &sign(b"whsec", now, body),
            body,
            now
        ));
        assert!(!signature_is_valid(
            b"whsec",
            &sign(b"whsec", now, body),
            b"{}",
            now
        ));
        assert!(!signature_is_valid(
            b"whsec",
            &sign(b"whsec", now - 301, body),
            body,
            now
        ));
        assert!(!signature_is_valid(b"whsec", "v1=00", body, now));
        assert!(!signature_is_valid(b"whsec", "", body, now));
    }

    #[test]
    fn standing_follows_status_and_grace() {
        let grace = READ_GRACE.as_secs();
        assert_eq!(standing(None, 0), Standing::Lapsed);
        for status in ["trialing", "active", "past_due"] {
            assert_eq!(
                standing(Some(&subscription("s", status, 1)), 1_000_000),
                Standing::Active
            );
        }
        let canceled = subscription("s", "canceled", 1);
        assert_eq!(canceled.current_period_end(), Some(1000));
        assert_eq!(standing(Some(&canceled), 1000 + grace - 1), Standing::Grace);
        assert_eq!(standing(Some(&canceled), 1000 + grace), Standing::Lapsed);
        let unpaid = subscription("s", "unpaid", 1);
        assert_eq!(standing(Some(&unpaid), 1001), Standing::Grace);
    }

    #[test]
    fn keeps_the_latest_subscription_per_org() {
        let mut by_org = HashMap::new();
        keep_latest(&mut by_org, subscription("new", "active", 20));
        keep_latest(&mut by_org, subscription("old", "canceled", 10));
        assert_eq!(by_org[&100].id, "new");
        keep_latest(&mut by_org, subscription("new", "canceled", 20));
        assert_eq!(by_org[&100].status, "canceled");
        let mut orphan = subscription("x", "active", 30);
        orphan.metadata.clear();
        keep_latest(&mut by_org, orphan);
        assert_eq!(by_org.len(), 1);
    }
}
