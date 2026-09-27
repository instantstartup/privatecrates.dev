//! The website's account API (docs/website-api.md): who is signed in, their organisations, and setting up and
//! paying for an organisation's registry.
//!
//! Every GitHub call about the user is made with their own token, so GitHub decides which organisations they see
//! and whether they are an admin. Our Apps' tokens are used only to check installations and to write the
//! settings file.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::task::JoinSet;

use crate::{
    AppState,
    billing::{Standing, TRIAL_DAYS},
    error::ApiError,
    github::{AppKind, Conditional, FileWrite, GitHubError, Membership, Organization, Repo},
    session::{self, Session, clear_session_cookie},
    tenant::{SETTINGS_PATH, Tenant, is_reserved, slug_is_valid},
};

pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/github/login", get(session::login))
        .route("/auth/github/callback", get(session::callback))
        .route("/auth/logout", post(session::logout))
        .route("/api/session", get(session_info))
        .route("/api/orgs/{org}/onboarding", get(onboarding))
        .route("/api/orgs/{org}/settings", post(settings))
        .route("/api/orgs/{org}/checkout", post(checkout))
        .route("/api/orgs/{org}/portal", post(portal))
        .route("/api/errors", get(errors))
        .layer(middleware::from_fn(no_store))
        .layer(middleware::from_fn_with_state(state, session::same_origin))
}

/// Answers depend on who is signed in, so no cache may keep them.
async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn rfc3339(secs: Option<u64>) -> Option<String> {
    let secs = i64::try_from(secs?).ok()?;
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

/// The organisation's registry and its subscription, or `null` when it is not set up.
fn tenant_json(state: &AppState, org_id: u64) -> Value {
    let Some(tenant) = state.tenants.by_org(org_id) else {
        return Value::Null;
    };
    let (status, trial_ends_at, current_period_end) = if !state.billing.enabled() {
        (Some("active".to_owned()), None, None)
    } else {
        match state.billing.subscription(org_id) {
            Some(s) if s.status == "trialing" => (Some(s.status.clone()), s.trial_end, None),
            Some(s) => (Some(s.status.clone()), None, s.current_period_end()),
            None => (None, None, None),
        }
    };
    json!({
        "slug": tenant.slug,
        "registry_url": state.config.tenant_base_url(&tenant.slug),
        "status": status,
        "trial_ends_at": rfc3339(trial_ends_at),
        "current_period_end": rfc3339(current_period_end),
    })
}

fn role(membership: &Membership) -> &'static str {
    if membership.is_admin() {
        "admin"
    } else {
        "member"
    }
}

/// `GET /api/session`: the signed-in user and their organisations.
async fn session_info(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // GitHub App user tokens only see organisations that have installed the App, so a new organisation starts by
    // installing it; `install_url` is how the website offers that.
    let install_url = install_url(&state.config, &state.config.reader_app_slug);
    let signed_out = Json(json!({ "user": null, "orgs": [], "install_url": install_url }));
    let Some(session) = Session::from_headers(&state, &headers) else {
        return Ok(signed_out.into_response());
    };
    let user = match state.gh.user(&session.token).await {
        Ok(user) => user,
        // Revoked on GitHub: the session is over.
        Err(GitHubError::Unauthorized) => {
            return Ok(([(header::SET_COOKIE, clear_session_cookie())], signed_out).into_response());
        }
        Err(e) => return Err(e.into()),
    };
    let mut lookups = JoinSet::new();
    for org in state.gh.user_orgs(&session.token).await? {
        let state = state.clone();
        let token = session.token.clone();
        lookups.spawn(async move {
            let membership = state.gh.org_membership(&token, &org.login).await;
            (org, membership)
        });
    }
    let mut orgs = Vec::new();
    while let Some(joined) = lookups.join_next().await {
        let (org, membership) = joined.map_err(|e| ApiError::internal(e.to_string()))?;
        let Some(membership) = membership?.filter(|m| m.state == "active") else {
            continue;
        };
        orgs.push((
            org.login.clone(),
            json!({
                "id": org.id,
                "login": org.login,
                "avatar_url": org.avatar_url,
                "role": role(&membership),
                "tenant": tenant_json(&state, org.id),
            }),
        ));
    }
    orgs.sort_by_key(|(login, _)| login.to_ascii_lowercase());
    Ok(Json(json!({
        "user": { "login": user.login, "avatar_url": user.avatar_url, "name": user.name },
        "orgs": orgs.into_iter().map(|(_, org)| org).collect::<Vec<_>>(),
        "install_url": install_url,
    }))
    .into_response())
}

/// The signed-in user's active membership of `org`.
async fn member(state: &AppState, session: &Session, org: &str) -> Result<Membership, ApiError> {
    let not_found = || ApiError::OrgNotFound {
        org: org.to_owned(),
    };
    // A GitHub login is letters, digits and hyphens; anything else must not reach a GitHub API path.
    let valid = !org.is_empty()
        && org.len() <= 39
        && org.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return Err(not_found());
    }
    match state.gh.org_membership(&session.token, org).await {
        Ok(Some(membership)) if membership.state == "active" => Ok(membership),
        Ok(_) => Err(not_found()),
        Err(GitHubError::Unauthorized) => Err(ApiError::SignInRequired),
        Err(e) => Err(e.into()),
    }
}

async fn admin(state: &AppState, session: &Session, org: &str) -> Result<Membership, ApiError> {
    let membership = member(state, session, org).await?;
    if !membership.is_admin() {
        return Err(ApiError::AdminRequired {
            org: membership.organization.login,
        });
    }
    Ok(membership)
}

/// The storage App's installation token for an organisation and the repositories it is installed on, if it is
/// installed there.
async fn storage(state: &AppState, org: &str) -> Result<Option<(String, Vec<Repo>)>, ApiError> {
    let Some(installation) = state.gh.org_installation(AppKind::Storage, org).await? else {
        return Ok(None);
    };
    let token = state
        .gh
        .installation_token(AppKind::Storage, installation.id)
        .await?;
    let repos = state.gh.installation_repositories(&token).await?;
    Ok(Some((token, repos)))
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Status {
    Done,
    Todo,
    Blocked,
}

#[derive(Serialize)]
struct Step {
    id: &'static str,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_url: Option<String>,
}

impl Step {
    /// A step that is done, or to do with a detail saying what to do.
    fn new(id: &'static str, done: bool, todo: impl Into<String>) -> Self {
        Self {
            id,
            status: if done { Status::Done } else { Status::Todo },
            detail: (!done).then(|| todo.into()),
            action_url: None,
        }
    }

    fn action(self, url: String) -> Self {
        Self {
            action_url: Some(url),
            ..self
        }
    }

    fn blocked(self, detail: String) -> Self {
        Self {
            status: Status::Blocked,
            detail: Some(detail),
            ..self
        }
    }
}

/// Whether the storage repository has a settings file.
async fn has_settings(state: &AppState, token: &str, repo: &Repo) -> Result<bool, ApiError> {
    let tree = match state
        .gh
        .tree(token, &repo.full_name, &repo.default_branch, None)
        .await
    {
        Ok(Conditional::Modified { value, .. }) => value,
        Ok(Conditional::NotModified) => return Ok(false),
        // An empty repository has no branch yet.
        Err(GitHubError::NotFound) => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    Ok(tree.tree.iter().any(|e| e.path == SETTINGS_PATH))
}

/// Where an organisation installs one of our Apps.
fn install_url(config: &crate::config::Config, app: &str) -> String {
    let github = config.github_web.as_str().trim_end_matches('/');
    format!("{github}/apps/{app}/installations/new")
}

/// The organisation's set-up checklist.
async fn onboarding_doc(state: &AppState, membership: &Membership) -> Result<Value, ApiError> {
    let org = &membership.organization;
    let config = &state.config;
    let github = config.github_web.as_str().trim_end_matches('/');
    let install = |app: &str| install_url(config, app);
    let mut tenant = state.tenants.by_org(org.id);
    let (reader, storage_repos, settings_file) = match &tenant {
        Some(_) => (true, Some(1), true),
        None => {
            let reader = state
                .gh
                .org_installation(AppKind::Reader, &org.login)
                .await?
                .is_some();
            let storage = storage(state, &org.login).await?;
            let settings_file = match &storage {
                Some((token, repos)) if repos.len() == 1 => {
                    has_settings(state, token, &repos[0]).await?
                }
                _ => false,
            };
            (reader, storage.map(|(_, repos)| repos.len()), settings_file)
        }
    };
    // Both Apps are installed and the settings exist, but we have not noticed yet (a missed webhook).
    if tenant.is_none() && reader && settings_file {
        state.discover().await?;
        tenant = state.tenants.by_org(org.id);
    }
    let storage_ready = storage_repos == Some(1);
    let storage_detail = match storage_repos {
        Some(n) if n > 1 => format!(
            "The storage App is installed on {n} repositories. Install it on the storage repository only."
        ),
        _ => "Install the storage App on the storage repository only.".into(),
    };
    let settings = match &tenant {
        Some(_) => Step::new("settings", true, ""),
        None if !(reader && storage_ready) => {
            Step::new("settings", false, "").blocked("Install both Apps first.".into())
        }
        None if settings_file => Step::new(
            "settings",
            false,
            "privatecrates.toml in the storage repository is invalid, or its registry name is taken. Fix it with \
             a pull request.",
        ),
        None => Step::new("settings", false, "Choose your registry name."),
    };
    let subscription = state.billing.subscription(org.id);
    let subscribed = state.billing.standing(org.id) == Standing::Active;
    let subscription_todo = match &subscription {
        Some(s) => format!(
            "The subscription is {}. Subscribe again to keep using the registry.",
            s.status.replace('_', " ")
        ),
        None => format!("Start a {TRIAL_DAYS}-day free trial."),
    };
    let mut steps = vec![
        Step::new("reader_app", reader, "Install the reader App on the organisation.")
            .action(install(&config.reader_app_slug)),
        Step::new(
            "storage_repo",
            storage_ready,
            format!(
                "Create a private repository, e.g. {}/crates-store, or use an existing empty one: it must hold nothing \
                 but PrivateCrates' index and releases. Enable immutable releases in Settings → General → Releases. \
                 You choose it when you install the storage App.",
                org.login
            ),
        )
        .action(format!("{github}/organizations/{}/repositories/new", org.login)),
        Step::new("storage_app", storage_ready, storage_detail)
            .action(install(&config.storage_app_slug)),
        settings,
        Step::new("subscription", subscribed, subscription_todo),
    ];
    if !membership.is_admin() {
        let ask = format!("Only admins of {} can do this; ask one of them.", org.login);
        steps = steps
            .into_iter()
            .map(|step| match step.status {
                Status::Done => step,
                _ => step.blocked(ask.clone()),
            })
            .collect();
    }
    Ok(json!({
        "org": { "id": org.id, "login": org.login },
        "steps": steps,
        "suggested_slug": suggested_slug(state, org, tenant.as_deref()),
    }))
}

/// The organisation's own name, lowercased (a GitHub login is always a valid slug), unless it is reserved or
/// taken.
fn suggested_slug(state: &AppState, org: &Organization, tenant: Option<&Tenant>) -> String {
    if let Some(tenant) = tenant {
        return tenant.slug.clone();
    }
    let slug = org.login.to_ascii_lowercase();
    if is_reserved(&slug) || state.tenants.get(&slug).is_some() {
        format!("{slug}-crates")
    } else {
        slug
    }
}

async fn onboarding(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let membership = member(&state, &session, &org).await?;
    Ok(Json(onboarding_doc(&state, &membership).await?))
}

#[derive(Deserialize)]
struct SettingsRequest {
    slug: String,
}

/// Sets the organisation's registry up: the storage App creates `privatecrates.toml`, and only creates it. Any
/// later change is the organisation's own pull request.
async fn settings(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    let org = &membership.organization;
    let slug = serde_json::from_slice::<SettingsRequest>(&body)
        .map_err(|_| ApiError::SlugInvalid)?
        .slug;
    if !slug_is_valid(&slug) {
        return Err(ApiError::SlugInvalid);
    }
    if is_reserved(&slug) {
        return Err(ApiError::SlugReserved { slug });
    }
    if let Some(tenant) = state.tenants.by_org(org.id) {
        return Err(ApiError::SettingsExist {
            repository: tenant.storage_repo.clone(),
        });
    }
    if state.tenants.get(&slug).is_some() {
        return Err(ApiError::SlugTaken { slug });
    }
    let storage_not_ready = || ApiError::StorageNotReady {
        org: org.login.clone(),
    };
    let (token, repos) = storage(&state, &org.login)
        .await?
        .ok_or_else(storage_not_ready)?;
    let [repo]: [Repo; 1] = repos.try_into().map_err(|_| storage_not_ready())?;
    let registry = state.config.tenant_base_url(&slug);
    let content = format!(
        "# PrivateCrates settings: {}/docs/setup. Change them with a pull request.\nslug = \"{slug}\"\n",
        state.config.apex_url()
    );
    let message = format!(
        "Create privatecrates.toml\n\nSet up the registry {registry}, requested by {}.\n",
        membership.user.login
    );
    let write = FileWrite {
        path: SETTINGS_PATH,
        content: content.as_bytes(),
        // Create only: GitHub refuses the write if the file already exists.
        sha: None,
        message: &message,
    };
    match state
        .gh
        .put_file(&token, &repo.full_name, &repo.default_branch, write)
        .await
    {
        Ok(_) => {}
        Err(GitHubError::Conflict) => {
            return Err(ApiError::SettingsExist {
                repository: repo.full_name,
            });
        }
        Err(e) => return Err(e.into()),
    }
    tracing::info!(org = %org.login, %slug, by = %membership.user.login, "registry set up");
    state.discover().await?;
    Ok(Json(onboarding_doc(&state, &membership).await?))
}

fn account_url(state: &AppState, org: &Organization) -> String {
    format!("{}?org={}", state.config.account_url(), org.login)
}

async fn checkout(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    let org = &membership.organization;
    let back = account_url(&state, org);
    let url = state
        .billing
        .checkout(
            org.id,
            &org.login,
            &format!("{back}&checkout=success"),
            &back,
        )
        .await?;
    Ok(Json(json!({ "url": url })))
}

async fn portal(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    let org = &membership.organization;
    let url = state
        .billing
        .portal(org.id, &org.login, &account_url(&state, org))
        .await?;
    Ok(Json(json!({ "url": url })))
}

/// Every error the registry and the account API can return, for the documentation's error reference.
async fn errors() -> Json<Vec<Value>> {
    Json(
        apollo_errors::error_catalog()
            .into_iter()
            .filter(|e| e.type_name == "ApiError")
            .flat_map(|e| e.variants)
            .map(|v| {
                json!({ "code": v.code.default, "message": v.message, "http_status": v.http_status.as_u16() })
            })
            .collect(),
    )
}
