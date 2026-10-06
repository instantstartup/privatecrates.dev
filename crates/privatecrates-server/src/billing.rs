//! Billing with Stripe (docs/website-api.md): free for organisations with few members; above that, $10 a month for
//! each member past the free limit, never more than $100 a month, after a no-card trial.
//!
//! The Stripe price is $10 per unit per month; a subscription's quantity is the number of members it pays for
//! ([`billed_members`]), kept in step with the member count. Changes apply from the next invoice, without proration.
//!
//! Stripe is the source of truth and there is no database. Each subscription's metadata names its GitHub
//! organisation; subscriptions are listed at start-up and kept current by Stripe's webhooks, with a periodic reload
//! as the backstop. Plans are computed without Stripe too, but without it every tenant is active.
//!
//! During the preview (docs/preview.md) billing is off whatever Stripe configuration is present: every organisation
//! is free, and Stripe is never called.

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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;

use crate::{
    AppState,
    config::{Config, StripeConfig},
    error::ApiError,
    github::now_secs,
    metrics::{Metrics, SendRecorded},
};

/// Reads keep working this long after a subscription's last paid period ends.
pub const READ_GRACE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// How often subscriptions are listed again, in case a webhook was missed.
pub const REFRESH: Duration = Duration::from_secs(10 * 60);
/// How far a webhook's timestamp may be from our clock, against replays (Stripe's own libraries use 5 minutes).
const WEBHOOK_TOLERANCE_SECS: u64 = 5 * 60;
/// The most members an organisation pays for: past this, the price stays at its cap ($100 a month).
pub const MAX_BILLED_MEMBERS: u64 = 10;
/// The price of each member past the free limit, per month: the Stripe price (`scripts/stripe-setup.sh`).
pub const MEMBER_PRICE_USD: u64 = 10;
/// The Stripe API version of our requests, and of the webhook endpoint (`scripts/stripe-setup.sh`).
const API_VERSION: &str = "2026-08-26.dahlia";
const ORG_ID_KEY: &str = "github_org_id";
/// On a customer: the ID of its last subscription that had ended when the organisation was seen back at the free
/// limit. Growing past the limit again then earns a grace period, rather than stopping the registry at once.
const FREE_AFTER_KEY: &str = "free_after";
/// On a subscription: a grace period, not the organisation's one free trial.
const GRACE_KEY: &str = "grace";
/// How long an organisation that grows past the free limit again, after an earlier trial or subscription ended,
/// keeps everything working without a card.
pub const GRACE_DAYS: u32 = 14;

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
    /// Requested expanded, for its default payment method.
    customer: Customer,
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
    default_payment_method: Option<serde_json::Value>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

/// A subscription's customer: its ID, or the customer itself when expanded.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Customer {
    Id(String),
    Expanded {
        id: String,
        /// Where Stripe sends reminders and invoices.
        #[serde(default)]
        email: Option<String>,
        #[serde(default)]
        invoice_settings: InvoiceSettings,
        #[serde(default)]
        default_source: Option<serde_json::Value>,
        #[serde(default)]
        metadata: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
struct InvoiceSettings {
    #[serde(default)]
    default_payment_method: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Items {
    data: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
struct Item {
    #[serde(default)]
    id: String,
    #[serde(default)]
    quantity: Option<u64>,
    #[serde(default)]
    current_period_end: Option<u64>,
}

/// How many members an organisation pays for: those past the free limit, up to [`MAX_BILLED_MEMBERS`]. Zero for a
/// free organisation, whose subscription (if it has one) then costs nothing until it grows again.
pub fn billed_members(members: u64, free_member_limit: u64) -> u64 {
    members
        .saturating_sub(free_member_limit)
        .min(MAX_BILLED_MEMBERS)
}

impl Subscription {
    fn org_id(&self) -> Option<u64> {
        self.metadata.get(ORG_ID_KEY)?.parse().ok()
    }

    fn customer_id(&self) -> &str {
        match &self.customer {
            Customer::Id(id) | Customer::Expanded { id, .. } => id,
        }
    }

    /// `past_due` keeps working while Stripe retries the payment.
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "trialing" | "active" | "past_due")
    }

    /// Whether Stripe has a payment method to charge when a trial ends: it looks at the subscription's and then the
    /// customer's defaults.
    pub fn has_payment_method(&self) -> bool {
        self.default_payment_method.is_some()
            || matches!(
                &self.customer,
                Customer::Expanded { invoice_settings, default_source, .. }
                    if invoice_settings.default_payment_method.is_some() || default_source.is_some()
            )
    }

    /// Whether the customer has no email for Stripe's reminders and invoices, as a trial started automatically
    /// does not. Unknown, and so `false`, when the customer was not expanded.
    pub fn billing_email_missing(&self) -> bool {
        matches!(&self.customer, Customer::Expanded { email, .. } if email.as_deref().is_none_or(str::is_empty))
    }

    /// When the trial ends, while the subscription is in one.
    pub fn trial_ends_at(&self) -> Option<u64> {
        self.trial_end.filter(|_| self.status == "trialing")
    }

    /// When the current paid period ends, outside a trial.
    pub fn current_period_end(&self) -> Option<u64> {
        if self.status == "trialing" {
            return None;
        }
        self.current_period_end.or_else(|| {
            self.items
                .data
                .iter()
                .filter_map(|i| i.current_period_end)
                .max()
        })
    }

    fn customer_metadata(&self, key: &str) -> Option<&str> {
        match &self.customer {
            Customer::Expanded { metadata, .. } => metadata.get(key).map(String::as_str),
            Customer::Id(_) => None,
        }
    }

    /// Whether this is a grace period after growing past the free limit again, rather than the first trial.
    pub fn is_grace(&self) -> bool {
        self.metadata.get(GRACE_KEY).is_some_and(|v| v == "true")
    }

    /// Whether the organisation has been seen back at the free limit since this subscription ended.
    fn freed_since(&self) -> bool {
        self.customer_metadata(FREE_AFTER_KEY) == Some(self.id.as_str())
    }

    /// Until when builds can still read, once the subscription is over: the end of what was paid for (or of the
    /// trial), plus the read grace period. `None` while it is active.
    pub fn reads_until(&self) -> Option<u64> {
        if self.is_active() {
            return None;
        }
        self.paid_until().map(|until| until + READ_GRACE.as_secs())
    }

    /// The subscription's one item and the quantity it pays for.
    fn item(&self) -> Option<(&str, u64)> {
        let item = self.items.data.first()?;
        Some((item.id.as_str(), item.quantity.unwrap_or(1)))
    }

    /// When the service stopped being paid for, from which the read grace period runs.
    fn paid_until(&self) -> Option<u64> {
        self.ended_at
            .or_else(|| self.current_period_end())
            .or(self.trial_end)
    }
}

/// What a tenant may do, given its plan.
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

/// An organisation's plan, as the website shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    /// At or under the member limit.
    Free,
    Trial,
    Paid,
    /// Stripe is retrying a failed payment.
    PastDue,
    /// Over the limit, with a subscription that ended or never started.
    Inactive,
}

/// An organisation's plan and what it is based on.
#[derive(Debug, Clone)]
pub struct OrgPlan {
    pub plan: Plan,
    /// `None` when unknown, which counts as free.
    pub members: Option<u64>,
    pub subscription: Option<Subscription>,
    /// Whether the organisation can start its no-card trial.
    pub trial_available: bool,
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
    metrics: Metrics,
}

impl Stripe {
    /// A Stripe API call. A POST with an idempotency key has its effect once, however often it is repeated.
    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        form: &[(&str, &str)],
        idempotency_key: Option<&str>,
    ) -> Result<T, BillingError> {
        let url = self
            .config
            .api
            .join(path.trim_start_matches('/'))
            .expect("API paths are valid URL paths");
        let mut request = self
            .http
            .request(method.clone(), url)
            .bearer_auth(&self.config.secret_key)
            .header("Stripe-Version", API_VERSION);
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        request = if method == Method::GET {
            request.query(form)
        } else {
            request.form(form)
        };
        let response = request.send_recorded(&self.metrics).await?;
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
        self.call(
            Method::GET,
            &format!("/v1/subscriptions/{id}"),
            &[("expand[]", "customer")],
            None,
        )
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

#[derive(Deserialize)]
struct Created {
    id: String,
}

/// A basic check that a billing email is an address Stripe can send to: `local@domain.tld`, with no spaces or
/// characters that would make it several addresses. Stripe and the mail system do the rest.
pub fn email_is_valid(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    let labels: Vec<&str> = domain.split('.').collect();
    email.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && !email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "<>,;:\"()[]\\".contains(c))
        && !domain.contains('@')
        && labels.len() >= 2
        && labels
            .iter()
            .all(|l| !l.is_empty() && !l.starts_with('-') && !l.ends_with('-'))
}

/// "3-month" for 90 days, otherwise "45-day".
pub fn trial_length(days: u32) -> String {
    if days.is_multiple_of(30) {
        format!("{}-month", days / 30)
    } else {
        format!("{days}-day")
    }
}

pub struct Billing {
    stripe: Option<Stripe>,
    preview: bool,
    free_member_limit: u64,
    trial_days: u32,
    /// GitHub organisation ID → its latest subscription.
    subscriptions: RwLock<HashMap<u64, Subscription>>,
    /// Held while a trial starts, so that a double click starts one.
    starting_trial: tokio::sync::Mutex<()>,
}

impl Billing {
    pub fn new(config: &Config) -> Result<Self, BillingError> {
        let stripe = match &config.stripe {
            _ if config.preview => {
                log::info!("preview: PrivateCrates is free, and billing is off");
                None
            }
            Some(stripe) => Some(Stripe {
                http: reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()?,
                config: stripe.clone(),
                metrics: Metrics::default(),
            }),
            None => {
                log::warn!("Stripe is not configured, so every tenant is treated as active");
                None
            }
        };
        Ok(Self {
            stripe,
            preview: config.preview,
            free_member_limit: config.free_member_limit,
            trial_days: config.trial_days,
            subscriptions: RwLock::default(),
            starting_trial: tokio::sync::Mutex::default(),
        })
    }

    pub fn enabled(&self) -> bool {
        self.stripe.is_some()
    }

    /// Whether this is the preview, free for everyone.
    pub fn preview(&self) -> bool {
        self.preview
    }

    /// The outcomes and latencies of our recent calls to Stripe, when it is configured.
    pub fn metrics(&self) -> Option<&Metrics> {
        self.stripe.as_ref().map(|s| &s.metrics)
    }

    pub fn free_member_limit(&self) -> u64 {
        self.free_member_limit
    }

    pub fn trial_days(&self) -> u32 {
        self.trial_days
    }

    /// Lists every subscription from Stripe and keeps the latest per organisation.
    pub async fn load(&self) -> Result<(), BillingError> {
        let Some(stripe) = &self.stripe else {
            return Ok(());
        };
        let mut by_org = HashMap::new();
        let mut after: Option<String> = None;
        loop {
            let mut query = vec![
                ("status", "all"),
                ("limit", "100"),
                ("expand[]", "data.customer"),
            ];
            if let Some(after) = &after {
                query.push(("starting_after", after));
            }
            let page: List<Subscription> = stripe
                .call(Method::GET, "/v1/subscriptions", &query, None)
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

    /// An unknown member count counts as free: GitHub failing must not take registries down.
    fn is_free(&self, members: Option<u64>) -> bool {
        members.is_none_or(|n| n <= self.free_member_limit)
    }

    /// How many members the organisation pays for (see [`billed_members`]).
    fn billed(&self, members: Option<u64>) -> u64 {
        members.map_or(0, |n| billed_members(n, self.free_member_limit))
    }

    /// The monthly price in US dollars for an organisation with this many members.
    pub fn monthly_price_usd(&self, members: Option<u64>) -> u64 {
        self.billed(members) * MEMBER_PRICE_USD
    }

    /// Brings the subscription's quantity in line with the member count, when they differ: the new price applies
    /// from the next invoice. An unknown count changes nothing.
    pub async fn sync_quantity(
        &self,
        org_id: u64,
        org_login: &str,
        members: Option<u64>,
    ) -> Result<(), ApiError> {
        let Some(stripe) = &self.stripe else {
            return Ok(());
        };
        if members.is_none() {
            return Ok(());
        }
        let Some(subscription) = self.subscription(org_id).filter(Subscription::is_active) else {
            return Ok(());
        };
        let Some((item, quantity)) = subscription.item() else {
            return Ok(());
        };
        let billed = self.billed(members);
        if quantity == billed || item.is_empty() {
            return Ok(());
        }
        let id: String = subscription
            .id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let billed_text = billed.to_string();
        let updated: Subscription = stripe
            .call(
                Method::POST,
                &format!("/v1/subscriptions/{id}"),
                &[
                    ("items[0][id]", item),
                    ("items[0][quantity]", &billed_text),
                    ("proration_behavior", "none"),
                    ("expand[]", "customer"),
                ],
                None,
            )
            .await?;
        log::info!(org:% = org_login, from = quantity, to = billed; "billed members changed");
        self.record(updated);
        Ok(())
    }

    /// A personal account's plan: always free, whatever the limit or Stripe says.
    pub fn personal_plan(&self) -> OrgPlan {
        OrgPlan {
            plan: Plan::Free,
            members: Some(1),
            subscription: None,
            trial_available: false,
        }
    }

    /// The organisation's plan, given its member count.
    pub fn plan(&self, org_id: u64, members: Option<u64>) -> OrgPlan {
        if self.preview {
            return OrgPlan {
                plan: Plan::Free,
                members,
                subscription: None,
                trial_available: false,
            };
        }
        let subscription = self.subscription(org_id);
        let free = self.is_free(members);
        let plan = match subscription.as_ref().map(|s| s.status.as_str()) {
            _ if free => Plan::Free,
            Some("trialing") => Plan::Trial,
            Some("active") => Plan::Paid,
            Some("past_due") => Plan::PastDue,
            _ => Plan::Inactive,
        };
        OrgPlan {
            plan,
            members,
            trial_available: self.enabled() && !free && subscription.is_none(),
            subscription,
        }
    }

    pub fn standing(&self, org_id: u64, members: Option<u64>) -> Standing {
        if !self.enabled() || self.is_free(members) {
            return Standing::Active;
        }
        standing(self.subscription(org_id).as_ref(), now_secs())
    }

    fn record(&self, subscription: Subscription) {
        log::info!(org_id:? = subscription.org_id(), subscription:% = subscription.id, status:% = subscription.status; "subscription updated");
        keep_latest(
            &mut self.subscriptions.write().expect("billing lock"),
            subscription,
        );
    }

    fn stripe(&self) -> Result<&Stripe, ApiError> {
        self.stripe.as_ref().ok_or(ApiError::BillingNotConfigured)
    }

    /// Notes, on the Stripe customer, that the organisation is back at the free limit after its last subscription
    /// ended, so that growing past it again earns a grace period. Nothing to do if that is already noted, or if the
    /// subscription is still active (it then simply costs nothing).
    pub async fn note_free(&self, org_id: u64, members: Option<u64>) -> Result<(), ApiError> {
        let Some(stripe) = &self.stripe else {
            return Ok(());
        };
        if members.is_none() || !self.is_free(members) {
            return Ok(());
        }
        let Some(subscription) = self.subscription(org_id) else {
            return Ok(());
        };
        if subscription.is_active() || subscription.freed_since() {
            return Ok(());
        }
        let customer: String = subscription
            .customer_id()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let key = format!("metadata[{FREE_AFTER_KEY}]");
        stripe
            .call::<Created>(
                Method::POST,
                &format!("/v1/customers/{customer}"),
                &[(key.as_str(), subscription.id.as_str())],
                None,
            )
            .await?;
        self.record(stripe.subscription(&subscription.id).await?);
        Ok(())
    }

    /// Whether the organisation has grown past the free limit again since its last subscription ended, and so gets
    /// a grace period rather than losing its registry at once.
    pub fn grace_due(&self, org_id: u64, members: Option<u64>) -> bool {
        self.enabled()
            && !self.is_free(members)
            && self
                .subscription(org_id)
                .is_some_and(|s| !s.is_active() && s.freed_since())
    }

    /// Starts a [`GRACE_DAYS`] grace period: a no-card trialing subscription, like the first trial, for an
    /// organisation that has grown past the free limit again ([`Self::grace_due`]). Without a card by its end, it
    /// ends as a trial does.
    pub async fn start_grace(
        &self,
        org_id: u64,
        org_login: &str,
        members: Option<u64>,
    ) -> Result<(), ApiError> {
        let stripe = self.stripe()?;
        let _starting = self.starting_trial.lock().await;
        if !self.grace_due(org_id, members) {
            return Ok(());
        }
        let previous = self.subscription(org_id).expect("checked by grace_due");
        // One grace period per ended subscription, whoever asks and however often.
        let key = format!("privatecrates-grace-{}", previous.id);
        let org_id = org_id.to_string();
        let quantity = self.billed(members).to_string();
        let days = GRACE_DAYS.to_string();
        let grace_key = format!("metadata[{GRACE_KEY}]");
        let subscription: Subscription = stripe
            .call(
                Method::POST,
                "/v1/subscriptions",
                &[
                    ("customer", previous.customer_id()),
                    ("items[0][price]", &stripe.config.price_id),
                    ("items[0][quantity]", &quantity),
                    ("trial_period_days", &days),
                    (
                        "payment_settings[save_default_payment_method]",
                        "on_subscription",
                    ),
                    (
                        "trial_settings[end_behavior][missing_payment_method]",
                        "cancel",
                    ),
                    ("metadata[github_org_id]", &org_id),
                    ("metadata[github_org_login]", org_login),
                    (grace_key.as_str(), "true"),
                    ("expand[]", "customer"),
                ],
                Some(&key),
            )
            .await?;
        log::info!(org:% = org_login, subscription:% = subscription.id; "grace period started");
        self.record(subscription);
        Ok(())
    }

    /// Until when builds can still read a lapsed organisation's registry (see [`Subscription::reads_until`]).
    pub fn reads_until(&self, org_id: u64) -> Option<u64> {
        self.subscription(org_id)?.reads_until()
    }

    /// Starts the organisation's no-card trial: a Stripe customer and a trialing subscription that cancels itself
    /// if no card was added by its end. Each organisation gets one trial. The billing email, set on the customer,
    /// receives Stripe's reminder before the trial ends; a trial started automatically has none.
    pub async fn start_trial(
        &self,
        org_id: u64,
        org_login: &str,
        members: Option<u64>,
        billing_email: Option<&str>,
    ) -> Result<(), ApiError> {
        let stripe = self.stripe()?;
        let _starting = self.starting_trial.lock().await;
        let org = || org_login.to_owned();
        match self.subscription(org_id) {
            Some(s) if s.is_active() => return Err(ApiError::AlreadySubscribed { org: org() }),
            Some(_) => return Err(ApiError::TrialUsed { org: org() }),
            None => {}
        }
        if self.is_free(members) {
            return Err(ApiError::FreePlan {
                org: org(),
                limit: self.free_member_limit,
            });
        }
        // Keyed by organisation, so a repeat (another instance, a retry) creates nothing more; Stripe keeps keys
        // for 24 hours, and after that the subscription is known.
        let key = format!("privatecrates-trial-{org_id}");
        let org_id = org_id.to_string();
        let mut customer = vec![
            ("name", org_login),
            ("metadata[github_org_id]", &org_id),
            ("metadata[github_org_login]", org_login),
        ];
        customer.extend(billing_email.map(|email| ("email", email)));
        let customer: Created = stripe
            .call(
                Method::POST,
                "/v1/customers",
                &customer,
                Some(&format!("{key}-customer")),
            )
            .await?;
        let trial_days = self.trial_days.to_string();
        let quantity = self.billed(members).to_string();
        let subscription: Subscription = stripe
            .call(
                Method::POST,
                "/v1/subscriptions",
                &[
                    ("customer", &customer.id),
                    ("items[0][price]", &stripe.config.price_id),
                    ("items[0][quantity]", &quantity),
                    ("trial_period_days", &trial_days),
                    (
                        "payment_settings[save_default_payment_method]",
                        "on_subscription",
                    ),
                    (
                        "trial_settings[end_behavior][missing_payment_method]",
                        "cancel",
                    ),
                    ("metadata[github_org_id]", &org_id),
                    ("metadata[github_org_login]", org_login),
                    ("expand[]", "customer"),
                ],
                Some(&key),
            )
            .await?;
        log::info!(org:% = org_login, subscription:% = subscription.id; "trial started");
        self.record(subscription);
        Ok(())
    }

    /// A Checkout session for an organisation that cannot have a trial: a card is required and billing starts at
    /// once. Returns the URL to send the admin to. A returning organisation keeps its Stripe customer.
    pub async fn checkout(
        &self,
        org_id: u64,
        org_login: &str,
        members: Option<u64>,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<String, ApiError> {
        let stripe = self.stripe()?;
        let org = || org_login.to_owned();
        let previous = match self.subscription(org_id) {
            Some(s) if s.is_active() => return Err(ApiError::AlreadySubscribed { org: org() }),
            _ if self.is_free(members) => {
                return Err(ApiError::FreePlan {
                    org: org(),
                    limit: self.free_member_limit,
                });
            }
            None => return Err(ApiError::TrialAvailable { org: org() }),
            Some(previous) => previous,
        };
        let org_id = org_id.to_string();
        let quantity = self.billed(members).to_string();
        let form = [
            ("mode", "subscription"),
            ("line_items[0][price]", stripe.config.price_id.as_str()),
            ("line_items[0][quantity]", &quantity),
            ("client_reference_id", &org_id),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("metadata[github_org_id]", &org_id),
            ("metadata[github_org_login]", org_login),
            ("subscription_data[metadata][github_org_id]", &org_id),
            ("subscription_data[metadata][github_org_login]", org_login),
            ("customer", previous.customer_id()),
        ];
        let session: Url = stripe
            .call(Method::POST, "/v1/checkout/sessions", &form, None)
            .await?;
        Ok(session.url)
    }

    /// Sets the email of the organisation's Stripe customer, where Stripe sends the reminder before a trial ends and
    /// invoices.
    pub async fn set_billing_email(
        &self,
        org_id: u64,
        org_login: &str,
        email: &str,
    ) -> Result<(), ApiError> {
        let stripe = self.stripe()?;
        let subscription = self
            .subscription(org_id)
            .ok_or_else(|| ApiError::NoSubscription {
                org: org_login.to_owned(),
            })?;
        let customer = subscription.customer_id();
        // Customer IDs come from Stripe, but keep them from reshaping the URL all the same.
        let customer: String = customer
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        stripe
            .call::<Created>(
                Method::POST,
                &format!("/v1/customers/{customer}"),
                &[("email", email)],
                None,
            )
            .await?;
        // Fetched again, so the session shows the email at once.
        self.record(stripe.subscription(&subscription.id).await?);
        Ok(())
    }

    /// A customer portal session, where an admin adds a card, manages payment details or cancels.
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
                    ("customer", subscription.customer_id()),
                    ("return_url", return_url),
                ],
                None,
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

    fn billing(stripe: bool) -> Billing {
        billing_with(stripe, false)
    }

    fn billing_with(stripe: bool, preview: bool) -> Billing {
        let mut config = crate::config::tests::config("privatecrates.dev");
        config.preview = preview;
        config.stripe = stripe.then(|| StripeConfig {
            api: "https://api.stripe.com".parse().unwrap(),
            secret_key: "sk_test".into(),
            webhook_secret: b"whsec".to_vec(),
            price_id: "price".into(),
        });
        Billing::new(&config).unwrap()
    }

    #[test]
    fn the_price_ramps_to_its_cap() {
        let price = |members| billed_members(members, 5) * MEMBER_PRICE_USD;
        assert_eq!(
            [0, 5, 6, 10, 15, 16, 500].map(price),
            [0, 0, 10, 50, 100, 100, 100]
        );
    }

    #[test]
    fn plans_follow_members_then_the_subscription() {
        let billed = billing(true);
        let plan = |members| billed.plan(100, members);
        for members in [None, Some(0), Some(5)] {
            let free = plan(members);
            assert_eq!(free.plan, Plan::Free);
            assert!(!free.trial_available);
            assert_eq!(billed.standing(100, members), Standing::Active);
        }
        let over = plan(Some(6));
        assert_eq!(over.plan, Plan::Inactive);
        assert!(over.trial_available);
        assert_eq!(billed.standing(100, Some(6)), Standing::Lapsed);
        for (status, expected) in [
            ("trialing", Plan::Trial),
            ("active", Plan::Paid),
            ("past_due", Plan::PastDue),
            ("canceled", Plan::Inactive),
            ("unpaid", Plan::Inactive),
        ] {
            billed.record(subscription("s", status, 1));
            let over = plan(Some(6));
            assert_eq!(over.plan, expected, "{status}");
            assert!(!over.trial_available, "{status}");
            // A subscription does not stop a small organisation being free.
            assert_eq!(plan(Some(5)).plan, Plan::Free);
        }

        // Without Stripe, plans are still computed but nothing is enforced.
        let unbilled = billing(false);
        assert_eq!(unbilled.plan(100, Some(6)).plan, Plan::Inactive);
        assert!(!unbilled.plan(100, Some(6)).trial_available);
        assert_eq!(unbilled.standing(100, Some(6)), Standing::Active);
    }

    #[test]
    fn the_preview_is_free_whatever_stripe_says() {
        let preview = billing_with(true, true);
        assert!(preview.preview());
        assert!(!preview.enabled());
        assert!(preview.metrics().is_none());
        let over = preview.plan(100, Some(50));
        assert_eq!(over.plan, Plan::Free);
        assert_eq!(over.members, Some(50));
        assert!(!over.trial_available);
        assert_eq!(preview.standing(100, Some(50)), Standing::Active);
    }

    #[test]
    fn billing_emails() {
        for valid in [
            "billing@example.com",
            "a.b+c@mail.example.co.uk",
            "x@xn--bcher-kva.example",
        ] {
            assert!(email_is_valid(valid), "{valid}");
        }
        for invalid in [
            "",
            "billing",
            "@example.com",
            "billing@",
            "billing@localhost",
            "billing@example..com",
            "billing@-example.com",
            "a@b@example.com",
            "billing @example.com",
            "a@example.com,b@example.com",
            "Billing <billing@example.com>",
            "billing@example.com\n",
        ] {
            assert!(!email_is_valid(invalid), "{invalid:?}");
        }
        assert!(!email_is_valid(&format!("{}@example.com", "a".repeat(65))));
    }

    #[test]
    fn trial_lengths_read_naturally() {
        assert_eq!(trial_length(90), "3-month");
        assert_eq!(trial_length(30), "1-month");
        assert_eq!(trial_length(14), "14-day");
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
