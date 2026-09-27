//! PrivateCrates: a hosted private Cargo registry that is a thin wrapper over GitHub. See SPEC.md.

pub mod account;
pub mod auth;
pub mod billing;
pub mod compliance;
pub mod config;
pub mod crate_file;
pub mod crates_io;
pub mod error;
pub mod github;
pub mod members;
pub mod metrics;
pub mod oidc;
pub mod publish;
pub mod routes;
pub mod search;
pub mod session;
pub mod status;
pub mod tenant;
pub mod webhooks;
pub mod website;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use apollo_errors::Error;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use miette::Diagnostic;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::{
    auth::PermissionCache,
    billing::{Billing, BillingError, OrgPlan, Standing},
    config::{Config, HostKind},
    crates_io::CratesIo,
    error::ApiError,
    github::{GitHub, GitHubError},
    oidc::{Oidc, RegistryTokens},
    session::Sealer,
    tenant::{BlobCache, Tenant, Tenants},
    website::Website,
};

#[derive(Debug, Error, Diagnostic)]
pub enum StartError {
    #[error("{source}")]
    #[diagnostic(code(start::github))]
    GitHub {
        #[from]
        source: GitHubError,
    },
    #[error("{source}")]
    #[diagnostic(code(start::billing))]
    Billing {
        #[from]
        source: BillingError,
    },
}

pub struct AppState {
    pub config: Config,
    pub gh: GitHub,
    pub tenants: Tenants,
    pub blobs: BlobCache,
    pub permissions: PermissionCache,
    pub oidc: Oidc,
    pub registry_tokens: RegistryTokens,
    pub crates_io: CratesIo,
    /// (storage repository ID, release tag) → `.crate` asset ID. Releases are immutable, so this never goes stale.
    pub asset_ids: moka::future::Cache<(u64, String), u64>,
    /// (storage repository ID, release tag) → signed download URL.
    pub download_urls: moka::future::Cache<(u64, String), String>,
    pub publish_limiter: PublishLimiter,
    pub search: search::Search,
    /// GitHub webhook deliveries already handled (SPEC §7).
    pub webhook_deliveries: moka::future::Cache<String, ()>,
    pub sealer: Sealer,
    pub billing: Billing,
    pub members: members::Members,
    pub website: Website,
    pub compliance: compliance::Compliance,
    /// When the server started, in Unix seconds.
    pub started_at: u64,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self, StartError> {
        Ok(Self {
            gh: GitHub::new(&config)?,
            tenants: Tenants::default(),
            blobs: tenant::blob_cache(),
            permissions: PermissionCache::new(config.permission_ttl),
            oidc: Oidc::new(config.oidc_issuer.clone(), config.oidc_jwks_url.clone()),
            registry_tokens: RegistryTokens::new(&config.registry_token_secret),
            crates_io: CratesIo::new(config.crates_io_api.clone()),
            asset_ids: moka::future::Cache::builder()
                .max_capacity(1_000_000)
                .build(),
            download_urls: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(routes::DOWNLOAD_URL_TTL)
                .build(),
            publish_limiter: PublishLimiter::new(config.publish_rate_per_minute),
            search: search::Search::default(),
            webhook_deliveries: webhooks::deliveries(),
            sealer: Sealer::new(&config.session_secret),
            billing: Billing::new(&config)?,
            members: members::Members::default(),
            website: Website::new(config.website_dir.as_deref()),
            compliance: compliance::Compliance::default(),
            started_at: github::now_secs(),
            config,
        })
    }

    /// Discovers tenants and loads their storage repositories.
    pub async fn discover(&self) -> Result<(), GitHubError> {
        self.tenants.discover(&self.gh, &self.blobs).await
    }

    /// An organisation's member count, or `None` when unknown (see [`members::Members::count`]).
    pub async fn members(&self, org_id: u64, org_login: &str) -> Option<u64> {
        let installation = self.tenants.by_org(org_id).map(|t| t.reader_installation);
        self.members
            .count(&self.gh, org_id, org_login, installation)
            .await
    }

    pub async fn plan(&self, org_id: u64, org_login: &str) -> OrgPlan {
        self.billing
            .plan(org_id, self.members(org_id, org_login).await)
    }

    /// What a tenant may do: everything while it is free or subscribed.
    pub async fn standing(&self, tenant: &Tenant) -> Standing {
        let members = self.members(tenant.org_id, &tenant.org_login).await;
        self.billing.standing(tenant.org_id, members)
    }

    /// Starts the trial of a registered organisation that is over the member limit and has never had a
    /// subscription, so that growing past the limit never breaks its registry. Called from webhooks and the periodic
    /// refresh, never while serving a request: a failure is logged, and the next refresh tries again.
    pub async fn start_trial_if_grown(&self, tenant: &Tenant) {
        let plan = self.plan(tenant.org_id, &tenant.org_login).await;
        if !plan.trial_available {
            return;
        }
        tracing::info!(org = %tenant.org_login, members = ?plan.members, "over the free member limit; starting the trial");
        if let Err(e) = self
            .billing
            .start_trial(tenant.org_id, &tenant.org_login, plan.members, None)
            .await
        {
            tracing::warn!(org = %tenant.org_login, error = %e, "starting the trial automatically failed");
        }
    }

    /// Refreshes every tenant's storage snapshot; cheap when nothing changed.
    pub async fn refresh_storage(&self) {
        for tenant in self.tenants.all() {
            if let Err(e) = tenant.refresh(&self.gh, &self.blobs).await {
                tracing::warn!(tenant = %tenant.slug, error = %e, "storage refresh failed");
            }
        }
    }
}

/// A fixed-window limit on publishes per token (SPEC §10.2).
pub struct PublishLimiter {
    per_minute: u32,
    counts: moka::future::Cache<[u8; 32], Arc<AtomicU32>>,
}

impl PublishLimiter {
    fn new(per_minute: u32) -> Self {
        Self {
            per_minute,
            counts: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
        }
    }

    pub async fn allow(&self, headers: &HeaderMap) -> bool {
        let key: [u8; 32] = Sha256::digest(
            headers
                .get(header::AUTHORIZATION)
                .map(|v| v.as_bytes())
                .unwrap_or_default(),
        )
        .into();
        let count = self
            .counts
            .get_with(key, async { Arc::new(AtomicU32::new(0)) })
            .await;
        count.fetch_add(1, Ordering::Relaxed) < self.per_minute
    }
}

/// Routes each request by its `Host`: the apex serves the website and account API, `www.` redirects to the apex,
/// and `{slug}.` serves that tenant's registry. `/healthz` answers on any host.
pub fn router(state: Arc<AppState>) -> Router {
    let apex = apex_router(state.clone());
    let tenant = tenant_router(state.clone());
    let https = state.config.public_scheme == "https";
    Router::new()
        .route("/healthz", get(routes::healthz))
        .fallback(move |request: Request| {
            let (apex, tenant, state) = (apex.clone(), tenant.clone(), state.clone());
            async move {
                let host = routes::request_host(request.headers(), request.uri());
                let Ok(response) = match state.config.host_kind(host.as_deref().unwrap_or_default())
                {
                    HostKind::Apex => apex.oneshot(request).await,
                    HostKind::Tenant(_) => tenant.oneshot(request).await,
                    HostKind::Www => Ok(to_apex(&state, &request)),
                    HostKind::Unknown => Ok(ApiError::NotFound.into_response()),
                };
                response
            }
        })
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            transport_headers(https, request, next)
        }))
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Headers for every response on every host, registries and redirects included: HSTS when served over HTTPS, and
/// `nosniff`.
async fn transport_headers(https: bool, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if https {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    response
}

fn to_apex(state: &AppState, request: &Request) -> Response {
    let path = request.uri().path_and_query().map_or("/", |p| p.as_str());
    match HeaderValue::from_str(&format!("{}{path}", state.config.apex_url())) {
        Ok(location) => (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, location)],
        )
            .into_response(),
        Err(_) => ApiError::NotFound.into_response(),
    }
}

/// The website, the account API and the webhooks, on the apex host.
fn apex_router(state: Arc<AppState>) -> Router {
    let mut router = Router::new();
    if !state.config.is_production() {
        // Only production may be indexed; the website's own robots.txt is for it.
        router = router.route("/robots.txt", get(website::disallow_robots));
    }
    router
        .route("/api/v1/auth", get(routes::apex_auth_info))
        .merge(account::routes(state.clone()))
        .merge(billing::routes())
        .merge(status::routes())
        .merge(webhooks::routes())
        .fallback(website::serve)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            website::security_headers,
        ))
        .with_state(state)
}

/// A tenant's registry, on `{slug}.{BASE_DOMAIN}`.
fn tenant_router(state: Arc<AppState>) -> Router {
    let publish_limit = state.config.max_crate_bytes + 10 * 1024 * 1024;
    Router::new()
        .route("/login", get(routes::login_page))
        .route("/index/config.json", get(routes::config_json))
        .route("/index/{*path}", get(routes::index_file))
        .route(
            "/api/v1/crates/new",
            put(routes::publish).layer(DefaultBodyLimit::max(publish_limit)),
        )
        .route(
            "/api/v1/crates/{name}/{version}/download",
            get(routes::download),
        )
        .route(
            "/api/v1/crates/{name}/{version}/yank",
            axum::routing::delete(routes::yank),
        )
        .route(
            "/api/v1/crates/{name}/{version}/unyank",
            put(routes::unyank),
        )
        .route("/api/v1/oidc/exchange", post(routes::oidc_exchange))
        .route("/api/v1/auth", get(routes::auth_info))
        .merge(search::routes())
        .with_state(state)
}

/// Keeps tenants, their storage snapshots, subscriptions and member counts fresh, and starts the trials of tenants
/// that grew past the free member limit. Webhooks (SPEC §7, and Stripe's) make most of this unnecessary; the timers
/// remain as the backstop, for instance for a webhook that reached an instance being replaced by a deploy.
pub fn spawn_refresh(state: Arc<AppState>) {
    let subscriptions = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(billing::REFRESH);
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = subscriptions.billing.load().await {
                // Without a current list, an organisation's earlier subscription could be missed.
                tracing::warn!(error = %e, "loading subscriptions failed");
                continue;
            }
            // Recounts each organisation's members once their count expires.
            for tenant in subscriptions.tenants.all() {
                subscriptions.start_trial_if_grown(&tenant).await;
            }
        }
    });
    let storage = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(storage.config.storage_refresh);
        tick.tick().await;
        loop {
            tick.tick().await;
            storage.refresh_storage().await;
        }
    });
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(state.config.tenant_refresh);
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = state.discover().await {
                tracing::warn!(error = %e, "tenant discovery failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn headers(https: bool) -> axum::http::HeaderMap {
        Router::new()
            .route("/", get(|| async { StatusCode::NOT_FOUND }))
            .layer(middleware::from_fn(move |request: Request, next: Next| {
                transport_headers(https, request, next)
            }))
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap()
            .headers()
            .clone()
    }

    #[tokio::test]
    async fn hsts_on_every_response_over_https() {
        let secure = headers(true).await;
        assert_eq!(
            secure[header::STRICT_TRANSPORT_SECURITY],
            "max-age=31536000; includeSubDomains"
        );
        assert_eq!(secure[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let plain = headers(false).await;
        assert!(plain.get(header::STRICT_TRANSPORT_SECURITY).is_none());
        assert_eq!(plain[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }
}
