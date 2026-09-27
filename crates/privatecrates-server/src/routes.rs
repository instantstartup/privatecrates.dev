//! The Cargo registry protocol surface (SPEC §4).

use std::{sync::Arc, time::Duration};

use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::{HeaderMap, StatusCode, Uri, header, request::Parts},
    response::{Html, IntoResponse, Response},
};
use bytes::Bytes;
use privatecrates_common::{
    audience,
    index::name_from_path,
    storage::{crate_asset_name, release_tag},
};

use crate::{
    AppState,
    auth::{Caller, Credential, Resolver},
    billing::Standing,
    error::ApiError,
    publish,
    tenant::{Tenant, index_path},
};

/// A request's host, lowercased: the `Host` header, or the authority of an HTTP/2 request.
pub fn request_host(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| uri.authority().map(|a| a.as_str()))
        .map(str::to_ascii_lowercase)
}

/// The tenant a request is for, from its `Host` header. A tenant whose subscription lapsed more than the grace
/// period ago is refused here, for every request; publishing is refused as soon as it lapses (see `publish`).
pub struct TenantHost(pub Arc<Tenant>);

impl FromRequestParts<Arc<AppState>> for TenantHost {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let host = request_host(&parts.headers, &parts.uri).ok_or(ApiError::NotFound)?;
        let tenant = state
            .config
            .slug_for_host(&host)
            .and_then(|slug| state.tenants.get(slug))
            .ok_or(ApiError::NotFound)?;
        if state.standing(&tenant).await == Standing::Lapsed {
            return Err(subscription_inactive(state, &tenant));
        }
        Ok(TenantHost(tenant))
    }
}

fn subscription_inactive(state: &AppState, tenant: &Tenant) -> ApiError {
    ApiError::SubscriptionInactive {
        org: tenant.org_login.clone(),
        account_url: state.config.account_url(),
    }
}

pub(crate) fn resolver<'a>(state: &'a AppState, tenant: &'a Tenant) -> Resolver<'a> {
    Resolver {
        gh: &state.gh,
        cache: &state.permissions,
        registry_tokens: &state.registry_tokens,
        tenant,
    }
}

fn login_url(state: &AppState, tenant: &Tenant) -> String {
    format!("{}/login", state.config.tenant_base_url(&tenant.slug))
}

fn credential(
    state: &AppState,
    tenant: &Tenant,
    headers: &HeaderMap,
) -> Result<Credential, ApiError> {
    Credential::from_headers(headers)
        .ok_or_else(|| ApiError::token_required(login_url(state, tenant)))
}

pub(crate) async fn caller(
    state: &AppState,
    tenant: &Tenant,
    headers: &HeaderMap,
) -> Result<Caller, ApiError> {
    let credential = credential(state, tenant, headers)?;
    resolver(state, tenant).caller(credential).await
}

/// What the credential provider needs to sign a developer in: the reader App's client ID, for GitHub's device flow.
/// Not secret; it identifies the App.
pub async fn auth_info(
    State(state): State<Arc<AppState>>,
    TenantHost(_tenant): TenantHost,
) -> Json<serde_json::Value> {
    github_client(&state)
}

/// The same on the apex host, where `cargo privatecrates login` signs in for the account API.
pub async fn apex_auth_info(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    github_client(&state)
}

fn github_client(state: &AppState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "github_client_id": state.config.reader_client_id,
        "github_url": state.config.github_web.as_str().trim_end_matches('/'),
    }))
}

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn config_json(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = caller(&state, &tenant, &headers).await?;
    if !resolver(&state, &tenant).can_use_registry(&caller).await? {
        return Err(ApiError::NoAccess {
            org: tenant.org_login.clone(),
        });
    }
    let base = state.config.tenant_base_url(&tenant.slug);
    Ok(Json(serde_json::json!({
        "dl": format!("{base}/api/v1/crates/{{crate}}/{{version}}/download"),
        "api": base,
        "auth-required": true,
    })))
}

pub async fn index_file(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &tenant, &headers).await?;
    let name = name_from_path(&path).ok_or(ApiError::NotFound)?;
    let owner = tenant.owner(name).ok_or(ApiError::NotFound)?;
    if !resolver(&state, &tenant)
        .can_read(&caller, owner.repository_id)
        .await?
    {
        return Err(ApiError::NotFound);
    }
    let (sha, content) = tenant
        .file(&state.gh, &state.blobs, &index_path(name))
        .await?
        .ok_or(ApiError::NotFound)?;
    let etag = format!("\"{sha}\"");
    let cache_control = (header::CACHE_CONTROL, "private, no-cache");
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|v| v.as_bytes() == etag.as_bytes())
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag), cache_control_owned(cache_control)],
        )
            .into_response());
    }
    Ok((
        [
            (header::ETAG, etag),
            cache_control_owned(cache_control),
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".into()),
        ],
        content,
    )
        .into_response())
}

fn cache_control_owned((name, value): (header::HeaderName, &str)) -> (header::HeaderName, String) {
    (name, value.to_owned())
}

/// Signed download URLs are cached this long. GitHub's signed URLs stay valid for several minutes, so this is well
/// within their lifetime, and a burst of CI jobs costs one GitHub call per crate (SPEC §4.3).
pub const DOWNLOAD_URL_TTL: Duration = Duration::from_secs(60);

pub async fn download(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &tenant, &headers).await?;
    let owner = tenant.owner(&name).ok_or(ApiError::NotFound)?;
    if !resolver(&state, &tenant)
        .can_read(&caller, owner.repository_id)
        .await?
    {
        return Err(ApiError::NotFound);
    }
    let tag = release_tag(&name, &version);
    let key = (tenant.storage_repo_id, tag.clone());
    if let Some(url) = state.download_urls.get(&key).await {
        return Ok(found(&url));
    }
    let token = tenant.storage_token(&state.gh).await?;
    let asset_id = crate_asset_id(&state, &tenant, &token, &name, &version).await?;
    let url = state
        .gh
        .asset_download_url(&token, &tenant.storage_repo, asset_id)
        .await?;
    state.download_urls.insert(key, url.clone()).await;
    Ok(found(&url))
}

/// The ID of a version's `.crate` release asset, given a storage App token.
pub(crate) async fn crate_asset_id(
    state: &AppState,
    tenant: &Tenant,
    token: &str,
    name: &str,
    version: &str,
) -> Result<u64, ApiError> {
    let tag = release_tag(name, version);
    let key = (tenant.storage_repo_id, tag.clone());
    if let Some(id) = state.asset_ids.get(&key).await {
        return Ok(id);
    }
    let release = state
        .gh
        .release_by_tag(token, &tenant.storage_repo, &tag)
        .await?
        .filter(|r| !r.draft)
        .ok_or(ApiError::NotFound)?;
    let asset_name = crate_asset_name(name, version);
    let id = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .map(|a| a.id)
        .ok_or(ApiError::NotFound)?;
    // Releases are immutable, so an asset ID never changes.
    state.asset_ids.insert(key, id).await;
    Ok(id)
}

fn found(url: &str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, url.to_owned())]).into_response()
}

pub async fn publish(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    if state.standing(&tenant).await != Standing::Active {
        return Err(subscription_inactive(&state, &tenant));
    }
    let credential = credential(&state, &tenant, &headers)?;
    if !state.publish_limiter.allow(&headers).await {
        return Err(ApiError::PublishRateLimited);
    }
    publish::publish(&state, &tenant, credential, body).await
}

pub async fn yank(
    state: State<Arc<AppState>>,
    tenant: TenantHost,
    path: Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    set_yanked(state, tenant, path, headers, true).await
}

pub async fn unyank(
    state: State<Arc<AppState>>,
    tenant: TenantHost,
    path: Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    set_yanked(state, tenant, path, headers, false).await
}

async fn set_yanked(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
    yanked: bool,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = caller(&state, &tenant, &headers).await?;
    let resolver = resolver(&state, &tenant);
    let owner = tenant.owner(&name).ok_or(ApiError::NotFound)?;
    if !resolver.can_read(&caller, owner.repository_id).await? {
        return Err(ApiError::NotFound);
    }
    if !resolver.can_push(&caller, owner.repository_id).await? {
        return Err(ApiError::PushRequired {
            action: "yanking".into(),
            repository: owner.repository,
        });
    }
    let login = resolver.login(&caller).await?;
    publish::set_yanked(&state, &tenant, &name, &version, yanked, &login).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Exchanges a GitHub Actions OIDC token for a one-hour, read-only registry token (SPEC §6.4).
pub async fn oidc_exchange(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Credential::Oidc(token) = credential(&state, &tenant, &headers)? else {
        return Err(ApiError::OidcTokenRequired);
    };
    let base = state.config.tenant_base_url(&tenant.slug);
    let claims = state
        .oidc
        .validate(&token, &audience::read(&base))
        .await
        .map_err(ApiError::from)?;
    if claims.owner_id() != Some(tenant.org_id) {
        return Err(ApiError::WorkflowOutsideOrganisation {
            org: tenant.org_login.clone(),
        });
    }
    let repository_id = claims
        .repository_id()
        .ok_or(ApiError::OidcRepositoryMissing)?;
    let (token, expires_at) =
        state
            .registry_tokens
            .issue(&tenant.slug, tenant.org_id, repository_id);
    Ok(Json(
        serde_json::json!({ "token": token, "expires_at": expires_at }),
    ))
}

pub async fn login_page(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
) -> Html<String> {
    let base = state.config.tenant_base_url(&tenant.slug);
    let apex = state.config.apex_url();
    let slug = &tenant.slug;
    let org = &tenant.org_login;
    Html(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{slug} · PrivateCrates</title>
<style>body{{font:16px/1.5 system-ui,sans-serif;max-width:46rem;margin:2rem auto;padding:0 1rem}}pre{{background:#f4f4f4;padding:1rem;overflow-x:auto}}@media (prefers-color-scheme:dark){{body{{background:#111;color:#eee}}pre{{background:#222}}a{{color:#8cf}}}}</style>
</head><body>
<h1>Private crates for {org}</h1>
<p>This is the <code>{slug}</code> registry, for the <strong>{org}</strong> organisation on GitHub. Access follows your
GitHub permissions: you can use a crate if you can read its repository on GitHub. There is no separate account.</p>

<h2 id="setup">Joining the team</h2>
<p>If a project already uses this registry, you need one thing: the credential provider.</p>
<pre>cargo install cargo-credential-privatecrates --locked</pre>
<p>Then build as usual. The first time, Cargo shows a code: approve it on GitHub, and you are signed in for every
project using {org}'s registry. To sign in before building (for example, before opening the project in an editor):</p>
<pre>cargo login --registry {slug}</pre>
<p>A project that does not use the registry yet needs it in <code>.cargo/config.toml</code>:</p>
<pre>[registries.{slug}]
index = "sparse+{base}/index/"
credential-provider = ["cargo-credential-privatecrates"]</pre>
<p>and dependencies that name it, in <code>Cargo.toml</code>:</p>
<pre>my_crate = {{ version = "1", registry = "{slug}" }}</pre>

<h2 id="editors">Editors and background builds</h2>
<p>Editors such as rust-analyzer run Cargo without a terminal, where there is nowhere to show a sign-in code. If you
are not signed in, the build stops at once with <em>not signed in … run <code>cargo login --registry {slug}</code> in a
terminal</em>. Run that once, then reload the editor.</p>

<h2 id="troubleshooting">When something does not work</h2>
<ul>
<li><strong>A crate is "not found".</strong> Either it does not exist, or you cannot read the repository it is
published from; the registry does not say which, so private names stay private. Ask someone in {org} for read access
to that repository. <code>cargo privatecrates doctor --crate NAME</code> checks your setup and sign-in.</li>
<li><strong>"no matching package" right after someone published.</strong> Retry after a minute, or run
<code>cargo update</code>.</li>
<li><strong>Signed in as the wrong GitHub account.</strong> <code>cargo logout --registry {slug}</code>, then
<code>cargo login --registry {slug}</code>.</li>
<li><strong>Signed in, but every crate is "not found".</strong> Your GitHub account must be a member of {org}; if
it is, sign out and in again, and on GitHub grant the PrivateCrates app access to {org} when asked.</li>
</ul>

<h2 id="ci">GitHub Actions</h2>
<p>Add <code>permissions: id-token: write</code> to the job and install the credential provider. No secrets are needed.</p>

<h2 id="publish">Publishing</h2>
<p>Crates are published from GitHub Actions, so every version has verifiable provenance. Only people who can create
releases in the crate's repository (write access or above) can trigger a publish.</p>
<pre>on:
  push:
    tags: ["v*"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - run: cargo install cargo-credential-privatecrates --locked
      - run: cargo publish --registry {slug}</pre>
<p>The crate's <code>package.repository</code> must be the repository the workflow runs in.
<code>cargo privatecrates init</code> sets all of this up.</p>
<p>More: <a href="{apex}/docs/joining">joining a team</a>, <a href="{apex}/docs">all documentation</a>.</p>
</body></html>"#
    ))
}
