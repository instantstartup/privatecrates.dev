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
    name::CrateName,
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

/// The tenant a request is for, from its `Host` header. A name with no registry gets an empty stand-in, so that
/// nobody can learn which organisations use PrivateCrates by probing names (SPEC §6.7). Nothing about the
/// subscription is checked here, before the caller is known; see `caller` and `publish`.
pub struct TenantHost(pub Arc<Tenant>);

impl FromRequestParts<Arc<AppState>> for TenantHost {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let host = request_host(&parts.headers, &parts.uri).ok_or(ApiError::NotFound)?;
        let slug = state
            .config
            .slug_for_host(&host)
            .ok_or(ApiError::NotFound)?;
        let tenant = state
            .tenants
            .get(slug)
            .unwrap_or_else(|| Arc::new(Tenant::phantom(slug.to_owned())));
        Ok(TenantHost(tenant))
    }
}

/// A crate name and version from a URL, before either goes near a GitHub API path: anything else is not found, as an
/// unknown crate is.
fn well_formed(name: &str, version: &str) -> Result<(), ApiError> {
    if CrateName::parse(name).is_err() || semver::Version::parse(version).is_err() {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

pub(crate) fn subscription_inactive(state: &AppState, tenant: &Tenant) -> ApiError {
    ApiError::SubscriptionInactive {
        org: tenant.org_login.clone(),
        account_url: state.config.account_url(),
    }
}

/// Why a publish is refused while builds can still read: with the day reads stop too.
pub(crate) fn publishing_paused(state: &AppState, tenant: &Tenant) -> ApiError {
    let reads_until = state
        .billing
        .reads_until(tenant.org_id)
        .and_then(|t| time::OffsetDateTime::from_unix_timestamp(i64::try_from(t).ok()?).ok());
    match reads_until {
        Some(until) => ApiError::PublishingPaused {
            org: tenant.org_login.clone(),
            reads_until: until.date().to_string(),
            account_url: state.config.account_url(),
        },
        None => subscription_inactive(state, tenant),
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
    let resolver = resolver(state, tenant);
    let caller = resolver.caller(credential).await?;
    // Only someone who may use the registry learns that its subscription has lapsed.
    if state.standing(tenant).await == Standing::Lapsed {
        return Err(if resolver.can_use_registry(&caller).await? {
            subscription_inactive(state, tenant)
        } else {
            ApiError::NoAccess
        });
    }
    Ok(caller)
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
        return Err(ApiError::NoAccess);
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

/// The longest a signed download URL is kept, whatever it says about itself.
pub const DOWNLOAD_URL_TTL: Duration = Duration::from_secs(30 * 60);
/// How long a signed URL that does not say when it expires is kept (GitHub's last for several minutes).
const DOWNLOAD_URL_DEFAULT_SECS: u64 = 60;
/// A cached URL is handed out only while it has at least this long left, so Cargo can still follow it.
const DOWNLOAD_URL_MARGIN_SECS: u64 = 60;

/// When a signed download URL from GitHub stops working, from what it says about itself: an Azure-style `se` time,
/// an S3-style `X-Amz-Date` + `X-Amz-Expires`, or a `jwt` parameter's `exp`; the earliest if several.
fn link_expiry(url: &str) -> Option<u64> {
    use base64::Engine;
    let url = url::Url::parse(url).ok()?;
    let params: std::collections::HashMap<String, String> =
        url.query_pairs().into_owned().collect();
    let se = params.get("se").and_then(|t| {
        time::OffsetDateTime::parse(t, &time::format_description::well_known::Rfc3339).ok()
    });
    let amz = params
        .get("X-Amz-Date")
        .zip(params.get("X-Amz-Expires"))
        .and_then(|(date, secs)| {
            let format =
                time::macros::format_description!("[year][month][day]T[hour][minute][second]Z");
            let start = time::PrimitiveDateTime::parse(date, &format)
                .ok()?
                .assume_utc();
            Some(start + Duration::from_secs(secs.parse().ok()?))
        });
    let jwt = params.get("jwt").and_then(|jwt| {
        let payload = jwt.split('.').nth(1)?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .ok()?;
        let exp = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?["exp"].as_i64()?;
        time::OffsetDateTime::from_unix_timestamp(exp).ok()
    });
    [se, amz, jwt]
        .into_iter()
        .flatten()
        .min()
        .and_then(|t| u64::try_from(t.unix_timestamp()).ok())
}

pub async fn download(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &tenant, &headers).await?;
    well_formed(&name, &version)?;
    let owner = tenant.owner(&name).ok_or(ApiError::NotFound)?;
    if !resolver(&state, &tenant)
        .can_read(&caller, owner.repository_id)
        .await?
    {
        return Err(ApiError::NotFound);
    }
    let tag = release_tag(&name, &version);
    let key = (tenant.storage_repo_id, tag.clone());
    let now = crate::github::now_secs();
    if let Some((url, expires)) = state.download_urls.get(&key).await
        && expires > now + DOWNLOAD_URL_MARGIN_SECS
    {
        return Ok(found(&url));
    }
    let token = tenant.storage_token(&state.gh).await?;
    let asset_id = crate_asset_id(&state, &tenant, &token, &name, &version).await?;
    let url = state
        .gh
        .asset_download_url(&token, &tenant.storage_repo, asset_id)
        .await?;
    // Kept as long as GitHub says it works, so a busy CI costs one GitHub call per crate version per link's life
    // rather than per minute (SPEC §4.3).
    let expires =
        link_expiry(&url).unwrap_or(now + DOWNLOAD_URL_DEFAULT_SECS + DOWNLOAD_URL_MARGIN_SECS);
    state
        .download_urls
        .insert(key, (url.clone(), expires))
        .await;
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
    well_formed(&name, &version)?;
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
        return Err(ApiError::WorkflowOutsideOrganisation);
    }
    if state.standing(&tenant).await == Standing::Lapsed {
        return Err(subscription_inactive(&state, &tenant));
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

/// The /login page's own policy: plain HTML and inline styles, no scripts, no framing.
const LOGIN_PAGE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; \
                              frame-ancestors 'none'";

pub async fn login_page(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
) -> impl IntoResponse {
    let base = state.config.tenant_base_url(&tenant.slug);
    let apex = state.config.apex_url();
    // The same page for every name, registry or not, and without the organisation's name (SPEC §6.7).
    let slug = &tenant.slug;
    let install = privatecrates_common::install::ci_step("cargo-credential-privatecrates");
    let headers = [
        (header::CONTENT_SECURITY_POLICY, LOGIN_PAGE_CSP),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
    ];
    (
        headers,
        Html(format!(
            r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{slug} · PrivateCrates</title>
<style>body{{font:16px/1.5 system-ui,sans-serif;max-width:46rem;margin:2rem auto;padding:0 1rem}}pre{{background:#f4f4f4;padding:1rem;overflow-x:auto}}@media (prefers-color-scheme:dark){{body{{background:#111;color:#eee}}pre{{background:#222}}a{{color:#8cf}}}}</style>
</head><body>
<h1>Private registry: {slug}</h1>
<p>This is the <code>{slug}</code> registry. Access follows your GitHub permissions: you can use a crate if you can
read its repository on GitHub. There is no separate account.</p>

<h2 id="setup">Joining the team</h2>
<p>If a project already uses this registry, you need one thing: the credential provider.</p>
<pre>cargo binstall cargo-credential-privatecrates
# without cargo-binstall, it builds from source:
cargo install cargo-credential-privatecrates --locked</pre>
<p>Then build as usual. The first time, Cargo shows a code: approve it on GitHub, and you are signed in for every
project using this registry. To sign in before building (for example, before opening the project in an editor):</p>
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
published from; the registry does not say which, so private names stay private. Ask someone in your organisation for read
access to that repository. <code>cargo privatecrates doctor --crate NAME</code> checks your setup and sign-in.</li>
<li><strong>"no matching package" right after someone published.</strong> Retry after a minute, or run
<code>cargo update</code>.</li>
<li><strong>A publish fails with "changes that were not yet committed into git: Cargo.lock".</strong> The version was
bumped without committing the updated <code>Cargo.lock</code>. Commit it, and push a tag for the next version;
<code>cargo privatecrates doctor</code> checks this before you tag.</li>
<li><strong>Signed in as the wrong GitHub account.</strong> <code>cargo logout --registry {slug}</code>, then
<code>cargo login --registry {slug}</code>.</li>
<li><strong>Signed in, but every crate is "not found".</strong> Your GitHub account must be a member of the organisation
that uses this registry; if it is, sign out and in again, and on GitHub grant the PrivateCrates app access to that
organisation when asked.</li>
</ul>

<h2 id="ci">GitHub Actions</h2>
<p>Add <code>permissions: id-token: write</code> to the job and install the credential provider with the step in the
workflow below, which downloads the prebuilt binary and checks it. No secrets are needed.</p>

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
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v5
{install}      - run: cargo publish --registry {slug}</pre>
<p>The crate's <code>package.repository</code> must be the repository the workflow runs in.
<code>cargo privatecrates init</code> sets all of this up.</p>
<p>More: <a href="{apex}/docs/joining">joining a team</a>, <a href="{apex}/docs">all documentation</a>.</p>
</body></html>"#
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::link_expiry;

    #[test]
    fn signed_links_say_when_they_expire() {
        // 2026-10-06T12:05:00Z
        let at = 1_791_288_300;
        assert_eq!(
            link_expiry(
                "https://release-assets.githubusercontent.com/a?sp=r&se=2026-10-06T12%3A05%3A00Z&sig=x"
            ),
            Some(at)
        );
        assert_eq!(
            link_expiry(
                "https://objects.githubusercontent.com/a?X-Amz-Date=20261006T120000Z&X-Amz-Expires=300&X-Amz-Signature=x"
            ),
            Some(at)
        );
        // eyJleHAiOjE3OTEyODgzMDB9 is {"exp":1791288300}.
        assert_eq!(
            link_expiry(
                "https://release-assets.githubusercontent.com/a?jwt=eyJhbGciOiJIUzI1NiJ9.eyJleHAiOjE3OTEyODgzMDB9.sig"
            ),
            Some(at)
        );
        // The earliest, when it says more than once.
        assert_eq!(
            link_expiry(
                "https://x.example/a?se=2026-10-06T12%3A05%3A00Z&jwt=a.eyJleHAiOjE3OTEyODkwMDB9.s"
            ),
            Some(at)
        );
        assert_eq!(link_expiry("https://x.example/signed/1?sig=fake"), None);
    }
}
