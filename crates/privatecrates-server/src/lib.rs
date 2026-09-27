//! PrivateCrates: a hosted private Cargo registry that is a thin wrapper over GitHub. See SPEC.md.

pub mod auth;
pub mod config;
pub mod crate_file;
pub mod crates_io;
pub mod error;
pub mod github;
pub mod oidc;
pub mod publish;
pub mod routes;
pub mod tenant;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderMap, header},
    routing::{get, post, put},
};
use sha2::{Digest, Sha256};

use crate::{
    auth::PermissionCache,
    config::Config,
    crates_io::CratesIo,
    github::{GitHub, GitHubError},
    oidc::{Oidc, RegistryTokens},
    tenant::{BlobCache, Tenants},
};

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
}

impl AppState {
    pub fn new(config: Config) -> Result<Self, GitHubError> {
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
            config,
        })
    }

    /// Discovers tenants and loads their storage repositories.
    pub async fn discover(&self) -> Result<(), GitHubError> {
        self.tenants.discover(&self.gh, &self.blobs).await
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

pub fn router(state: Arc<AppState>) -> Router {
    let publish_limit = state.config.max_crate_bytes + 10 * 1024 * 1024;
    Router::new()
        .route("/healthz", get(routes::healthz))
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
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

/// Keeps tenants and their storage snapshots fresh. Webhooks (SPEC §7) will make most of this unnecessary; the
/// timers remain as the backstop.
pub fn spawn_refresh(state: Arc<AppState>) {
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
