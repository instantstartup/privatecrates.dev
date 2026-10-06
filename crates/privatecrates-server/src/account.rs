//! The website's account API (docs/website-api.md): who is signed in, their organisations, and setting up and
//! paying for an organisation's registry.
//!
//! Every GitHub call about the user is made with their own token, so GitHub decides which organisations they see
//! and whether they are an admin. Our Apps' tokens are used only to check installations and to write the
//! settings file. An admin accepts the terms before a registry is created (docs/preview.md §2); that is recorded in
//! our own database.

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
use privatecrates_common::TERMS_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::task::JoinSet;

use crate::{
    AppState,
    billing::{self, OrgPlan, Plan, Subscription, trial_length},
    compliance,
    error::ApiError,
    github::{
        Account, AppKind, Conditional, FileWrite, GitHubError, Membership, Organization, Repo, User,
    },
    records::{self, Acceptance, Via},
    session::{self, Session, clear_session_cookie},
    tenant::{SETTINGS_PATH, Tenant, is_reserved, slug_is_valid},
};

pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/github/login", get(session::login))
        .route("/auth/github/callback", get(session::callback))
        .route("/auth/logout", post(session::logout))
        .route("/auth/logout-everywhere", post(session::logout_everywhere))
        .route("/api/session", get(session_info))
        .route("/api/orgs/{org}/onboarding", get(onboarding))
        .route("/api/orgs/{org}/settings", post(settings))
        .route("/api/orgs/{org}/terms", post(terms))
        .route("/api/orgs/{org}/trial", post(trial))
        .route("/api/orgs/{org}/billing-email", post(set_billing_email))
        .route("/api/orgs/{org}/checkout", post(checkout))
        .route("/api/orgs/{org}/portal", post(portal))
        .route("/api/errors", get(errors))
        .merge(compliance::routes())
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

pub(crate) fn rfc3339(secs: Option<u64>) -> Option<String> {
    let secs = i64::try_from(secs?).ok()?;
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

/// The organisation's registry and its raw subscription status, or `null` when it is not set up.
fn tenant_json(state: &AppState, org_id: u64, plan: &OrgPlan) -> Value {
    let Some(tenant) = state.tenants.by_org(org_id) else {
        return Value::Null;
    };
    let subscription = plan.subscription.as_ref();
    json!({
        "slug": tenant.slug,
        "registry_url": state.config.tenant_base_url(&tenant.slug),
        "status": subscription.map(|s| &s.status),
        "trial_ends_at": rfc3339(subscription.and_then(Subscription::trial_ends_at)),
        "current_period_end": rfc3339(subscription.and_then(Subscription::current_period_end)),
    })
}

/// An organisation in `GET /api/session`: who it is, the user's role, its plan and its registry.
fn org_json(
    state: &AppState,
    org: &Organization,
    membership: &Membership,
    plan: &OrgPlan,
    terms_accepted: bool,
) -> Value {
    let subscription = plan.subscription.as_ref();
    json!({
        "id": org.id,
        "login": org.login,
        "avatar_url": org.avatar_url,
        "role": role(membership),
        "members": plan.members,
        "free_member_limit": state.billing.free_member_limit(),
        "plan": plan.plan,
        // What the organisation pays (or would pay) a month at its member count: 0 when free.
        "monthly_price_usd": if membership.personal { 0 } else { state.billing.monthly_price_usd(plan.members) },
        "trial_ends_at": rfc3339(subscription.and_then(Subscription::trial_ends_at)),
        // The trial is a grace period: the organisation grew past the free limit again after an earlier one ended.
        "grace_period": plan.plan == Plan::Trial && subscription.is_some_and(Subscription::is_grace),
        "has_payment_method": subscription.is_some_and(Subscription::has_payment_method),
        "billing_email_missing": subscription.is_some_and(Subscription::billing_email_missing),
        "current_period_end": rfc3339(subscription.and_then(Subscription::current_period_end)),
        "trial_available": plan.trial_available,
        "tenant": tenant_json(state, org.id, plan),
        "terms_accepted": terms_accepted,
        "personal": membership.personal,
    })
}

/// The terms an organisation admin accepts now, and where to read them.
fn terms_json(state: &AppState) -> Value {
    json!({ "version": TERMS_VERSION, "url": state.config.terms_url() })
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
    let preview = state.billing.preview();
    let terms = terms_json(&state);
    let signed_out = Json(json!({
        "user": null, "orgs": [], "install_url": install_url, "preview": preview,
        "terms": terms,
    }));
    let Some(session) = Session::from_headers(&state, &headers)? else {
        return Ok(signed_out.into_response());
    };
    let user = match state.gh.user(&session.token).await {
        Ok(user) => user,
        // An expired or revoked bearer token: the tool signs in again.
        Err(GitHubError::Unauthorized) if session.bearer => return Err(ApiError::SignInRequired),
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
            let membership = match state.gh.org_membership(&token, &org.login).await {
                Ok(Some(membership)) if membership.state == "active" => membership,
                Ok(_) => return Ok(None),
                Err(e) => return Err(e),
            };
            let plan = state.plan(org.id, &org.login, membership.personal).await;
            let terms_accepted = state.terms.accepted(org.id).await;
            Ok(Some((
                org.login.clone(),
                org_json(&state, &org, &membership, &plan, terms_accepted),
            )))
        });
    }
    let mut orgs = Vec::new();
    while let Some(joined) = lookups.join_next().await {
        orgs.extend(joined.map_err(|e| ApiError::internal(e.to_string()))??);
    }
    orgs.sort_by_key(|(login, _)| login.to_ascii_lowercase());
    // Their own account comes first, once it has installed the reader App or has a registry.
    let installed = state.tenants.by_org(user.id).is_some()
        || state
            .gh
            .user_installation(AppKind::Reader, &user.login)
            .await?
            .is_some();
    if installed {
        let membership = personal_membership(user.clone());
        let plan = state.billing.personal_plan();
        let terms_accepted = state.terms.accepted(user.id).await;
        let account = org_json(
            &state,
            &membership.organization,
            &membership,
            &plan,
            terms_accepted,
        );
        orgs.insert(0, (user.login.clone(), account));
    }
    Ok(Json(json!({
        "user": { "login": user.login, "avatar_url": user.avatar_url, "name": user.name },
        "orgs": orgs.into_iter().map(|(_, org)| org).collect::<Vec<_>>(),
        "install_url": install_url,
        "preview": preview,
        "terms": terms,
    }))
    .into_response())
}

/// The signed-in user's active membership of `org`.
pub(crate) async fn member(
    state: &AppState,
    session: &Session,
    org: &str,
) -> Result<Membership, ApiError> {
    let not_found = || ApiError::OrgNotFound {
        org: org.to_owned(),
    };
    if !is_github_login(org) {
        return Err(not_found());
    }
    match state.gh.org_membership(&session.token, org).await {
        Ok(Some(membership)) if membership.state == "active" => Ok(membership),
        Ok(Some(_)) => Err(not_found()),
        // Not an organisation of theirs: it may be their own account.
        Ok(None) => match state.gh.user(&session.token).await {
            Ok(user) if user.login.eq_ignore_ascii_case(org) => Ok(personal_membership(user)),
            Ok(_) => Err(not_found()),
            Err(GitHubError::Unauthorized) => Err(ApiError::SignInRequired),
            Err(e) => Err(e.into()),
        },
        Err(GitHubError::Unauthorized) => Err(ApiError::SignInRequired),
        Err(e) => Err(e.into()),
    }
}

/// A person's own account, as if it were an organisation they alone administer.
fn personal_membership(user: User) -> Membership {
    Membership {
        state: "active".into(),
        role: "admin".into(),
        organization: Organization {
            id: user.id,
            login: user.login.clone(),
            avatar_url: user.avatar_url,
        },
        user: Account {
            login: user.login,
            id: user.id,
            kind: Some("User".into()),
        },
        personal: true,
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

/// The storage App's installation token for an organisation or personal account, and the repositories it is
/// installed on, if it is installed there.
async fn storage(
    state: &AppState,
    org: &str,
    personal: bool,
) -> Result<Option<(String, Vec<Repo>)>, ApiError> {
    let Some(installation) = state
        .gh
        .account_installation(AppKind::Storage, org, personal)
        .await?
    else {
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
    let (reader, storage_repos, settings_file, suggested_repo) = match &tenant {
        Some(_) => (true, Some(1), true, None),
        None => {
            let reader = state
                .gh
                .account_installation(AppKind::Reader, &org.login, membership.personal)
                .await?;
            let storage = storage(state, &org.login, membership.personal).await?;
            let settings_file = match &storage {
                Some((token, repos)) if repos.len() == 1 => {
                    has_settings(state, token, &repos[0]).await?
                }
                _ => false,
            };
            // Until the storage App is installed, look for the repository the pre-filled link creates: the reader
            // App can see it, so the checklist moves on by itself and the install link can pre-select it.
            let suggested_repo = match (&reader, &storage) {
                (Some(installation), None) => {
                    let token = state
                        .gh
                        .installation_token(AppKind::Reader, installation.id)
                        .await?;
                    state
                        .gh
                        .repository(&token, &format!("{}/{STORAGE_REPO_NAME}", org.login))
                        .await?
                }
                _ => None,
            };
            (
                reader.is_some(),
                storage.map(|(_, repos)| repos.len()),
                settings_file,
                suggested_repo,
            )
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
    let plan = state.plan(org.id, &org.login, membership.personal).await;
    let mut steps = vec![
        Step::new("reader_app", reader, "Install the reader App on the organisation.")
            .action(install(&config.reader_app_slug)),
        storage_repo_step(config, org, storage_ready, suggested_repo.as_ref()),
        Step::new("storage_app", storage_ready, storage_detail).action(match &suggested_repo {
            // Pre-selects the organisation and the storage repository on GitHub's install page.
            Some(repo) => format!(
                "{github}/apps/{}/installations/new/permissions?suggested_target_id={}&repository_ids[]={}",
                config.storage_app_slug, org.id, repo.id
            ),
            None => install(&config.storage_app_slug),
        }),
        settings,
        plan_step(state, &plan, membership.personal),
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
    let mut terms = terms_json(state);
    terms["accepted"] = state.terms.accepted(org.id).await.into();
    let verifier = match &tenant {
        Some(tenant) => verifier_json(state, tenant).await?,
        None => Value::Null,
    };
    Ok(json!({
        "org": { "id": org.id, "login": org.login },
        "personal": membership.personal,
        "steps": steps,
        "suggested_slug": suggested_slug(state, org, tenant.as_deref()),
        "terms": terms,
        "verifier": verifier,
    }))
}

/// The verifier workflow, recommended once the registry exists (SPEC §10.3): whether the storage repository has one,
/// and where to add it. An admin adds it with their own account: GitHub's new-file page, filled in, which they commit
/// themselves (our storage App cannot write workflows, and should not maintain what checks it).
async fn verifier_json(state: &AppState, tenant: &Tenant) -> Result<Value, ApiError> {
    let installed = crate::compliance::has_verify_workflow(state, tenant).await?;
    let github = state.config.github_web.as_str().trim_end_matches('/');
    let content =
        privatecrates_common::verifier::workflow(&state.config.tenant_base_url(&tenant.slug));
    let mut add_url = url::Url::parse(&format!(
        "{github}/{}/new/{}",
        tenant.storage_repo, tenant.branch
    ))
    .expect("a GitHub URL");
    add_url
        .query_pairs_mut()
        .append_pair("filename", privatecrates_common::verifier::WORKFLOW_PATH)
        .append_pair("value", &content);
    Ok(json!({
        "installed": installed,
        "repository": tenant.storage_repo,
        "path": privatecrates_common::verifier::WORKFLOW_PATH,
        "add_url": add_url.as_str(),
        "workflow": content,
    }))
}

/// Paying, or not: done while the organisation is free or its subscription is active, trialing or past due.
fn plan_step(state: &AppState, plan: &OrgPlan, personal: bool) -> Step {
    let billing = &state.billing;
    let limit = billing.free_member_limit();
    let subscription = plan.subscription.as_ref();
    let date = |secs: Option<u64>| {
        secs.and_then(|secs| OffsetDateTime::from_unix_timestamp(i64::try_from(secs).ok()?).ok())
            .map_or_else(|| "its end".to_owned(), |t| t.date().to_string())
    };
    let (done, detail) = match plan.plan {
        Plan::Free if personal => (true, "Personal accounts are always free.".to_owned()),
        Plan::Free if billing.preview() => (true, "Free during the preview.".to_owned()),
        Plan::Free => (
            true,
            match plan.members {
                Some(n) => format!("Free: {n} of {limit} members"),
                None => format!("Free for organisations with up to {limit} members"),
            },
        ),
        Plan::Trial => {
            let ends = date(subscription.and_then(Subscription::trial_ends_at));
            let card = if subscription.is_some_and(Subscription::has_payment_method) {
                "then the card on file is charged"
            } else {
                "add a card in the billing portal to keep the registry afterwards"
            };
            (true, format!("Free trial until {ends}; {card}."))
        }
        Plan::Paid => (true, "Subscribed.".to_owned()),
        Plan::PastDue => (
            true,
            "The last payment failed and Stripe is retrying it; update the card in the billing portal.".to_owned(),
        ),
        Plan::Inactive => {
            let members = plan.members.unwrap_or_default();
            let action = if !billing.enabled() {
                "billing is not set up on this server.".to_owned()
            } else if plan.trial_available {
                format!(
                    "start your {} free trial, no card needed. After it, ${} a month.",
                    trial_length(billing.trial_days()),
                    billing.monthly_price_usd(plan.members)
                )
            } else {
                format!(
                    "the subscription is {}. Subscribe with a card to keep using the registry.",
                    subscription.map_or("over", |s| s.status.as_str()).replace('_', " ")
                )
            };
            (false, format!("{members} members: {action}"))
        }
    };
    Step {
        id: "plan",
        status: if done { Status::Done } else { Status::Todo },
        detail: Some(detail),
        action_url: None,
    }
}

/// The storage repository's conventional name, which the pre-filled create link uses.
const STORAGE_REPO_NAME: &str = "crates-store";

/// Creating the storage repository. Our Apps cannot create repositories (that needs administration rights, which
/// could also turn immutable releases off), so the step links to GitHub's create page, pre-filled.
fn storage_repo_step(
    config: &crate::config::Config,
    org: &Organization,
    storage_ready: bool,
    suggested: Option<&Repo>,
) -> Step {
    let github = config.github_web.as_str().trim_end_matches('/');
    let immutable = "Enable immutable releases in its Settings → General → Releases, so published versions can never \
                     change.";
    match suggested {
        Some(repo) if !storage_ready => Step::new(
            "storage_repo",
            true,
            format!("{} exists. {immutable}", repo.full_name),
        )
        .action(format!("{github}/{}/settings", repo.full_name)),
        _ => {
            let mut create = url::Url::parse(&format!("{github}/new")).expect("a valid URL");
            create
                .query_pairs_mut()
                .append_pair("owner", &org.login)
                .append_pair("name", STORAGE_REPO_NAME)
                .append_pair("visibility", "private")
                .append_pair(
                    "description",
                    "PrivateCrates registry storage: the index and crate releases",
                );
            Step::new(
                "storage_repo",
                storage_ready,
                format!(
                    "Create {}/{STORAGE_REPO_NAME} (the link fills in the form), or use an existing empty repository: \
                     it must hold nothing but PrivateCrates' index and releases. {immutable}",
                    org.login
                ),
            )
            .action(create.to_string())
        }
    }
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

/// `accept_terms` is any JSON value, so that a wrong one is refused as not accepting the terms.
#[derive(Deserialize)]
struct SettingsRequest {
    slug: String,
    accept_terms: Option<Value>,
    /// The admin's choice: may developers publish from their own machines? Off unless chosen.
    #[serde(default)]
    allow_manual_publish: bool,
}

#[derive(Deserialize)]
struct TermsRequest {
    accept_terms: Option<Value>,
}

/// Checks that the request accepts the current terms: the admin sends the version they were shown.
fn check_terms(state: &AppState, accept_terms: Option<&Value>) -> Result<(), ApiError> {
    if accept_terms.and_then(Value::as_str) == Some(TERMS_VERSION) {
        Ok(())
    } else {
        Err(ApiError::TermsNotAccepted {
            version: TERMS_VERSION,
            terms_url: state.config.terms_url(),
        })
    }
}

/// Records that the signed-in admin accepted the current terms on behalf of the organisation.
async fn record_terms(
    state: &AppState,
    session: &Session,
    membership: &Membership,
) -> Result<(), ApiError> {
    let org = &membership.organization;
    let statement = records::statement(TERMS_VERSION, &org.login);
    let acceptance = Acceptance {
        org_id: org.id,
        org_login: &org.login,
        user_id: membership.user.id,
        user_login: &membership.user.login,
        version: TERMS_VERSION,
        via: if session.bearer {
            Via::Cli
        } else {
            Via::Website
        },
        statement: &statement,
    };
    Ok(state.terms.accept(&acceptance).await?)
}

/// Sets the organisation's registry up: the admin's acceptance of the terms is recorded, then the storage App
/// creates `privatecrates.toml`, and only creates it. Any later change is the organisation's own pull request.
async fn settings(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    let org = &membership.organization;
    let request =
        serde_json::from_slice::<SettingsRequest>(&body).map_err(|_| ApiError::SlugInvalid)?;
    let slug = request.slug;
    if !slug_is_valid(&slug) {
        return Err(ApiError::SlugInvalid);
    }
    if is_reserved(&slug) {
        return Err(ApiError::SlugReserved { slug });
    }
    check_terms(&state, request.accept_terms.as_ref())?;
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
    let (token, repos) = storage(&state, &org.login, membership.personal)
        .await?
        .ok_or_else(storage_not_ready)?;
    let [repo]: [Repo; 1] = repos.try_into().map_err(|_| storage_not_ready())?;
    let registry = state.config.tenant_base_url(&slug);
    let apex = state.config.apex_url();
    let content = format!(
        "# PrivateCrates settings: {apex}/docs/setup. Change them with a pull request.\nslug = \"{slug}\"\n\n\
         # Whether crates may also be published from developers' machines, first versions included. Those versions\n\
         # have no provenance: {apex}/docs/publishing#laptop. When false, crates are published from GitHub Actions.\n\
         # This is the default for every repository.\n\
         allow_manual_publish = {}\n\n\
         # A repository's own setting overrides the default, by its name:\n\
         # [repositories.my-repository]\n\
         # allow_manual_publish = {}\n",
        request.allow_manual_publish, !request.allow_manual_publish
    );
    let message = format!(
        "Create privatecrates.toml\n\nSet up the registry {registry}, requested by {}.\n",
        membership.user.login
    );
    // Recorded first: without the acceptance on record, no registry is created.
    record_terms(&state, &session, &membership).await?;
    let write = FileWrite {
        path: SETTINGS_PATH,
        content: content.as_bytes(),
        // Create only: GitHub refuses the write if the file already exists.
        sha: None,
        message: &message,
    };
    match state
        .gh
        // The default branch: a new storage repository is empty, and this becomes its first commit.
        .put_file(&token, &repo.full_name, None, write)
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

/// Accepts the current terms for an organisation whose registry exists: set up before the terms, or before their
/// current version. Accepting again records nothing new.
async fn terms(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    let accept_terms = serde_json::from_slice::<TermsRequest>(&body)
        .ok()
        .and_then(|r| r.accept_terms);
    check_terms(&state, accept_terms.as_ref())?;
    let org = &membership.organization;
    if state.tenants.by_org(org.id).is_none() {
        return Err(ApiError::NotSetUp {
            org: org.login.clone(),
        });
    }
    record_terms(&state, &session, &membership).await?;
    Ok(Json(onboarding_doc(&state, &membership).await?))
}

/// A GitHub login is letters, digits and hyphens; anything else must not reach a GitHub API path.
fn is_github_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Billing is off during the preview: there is nothing to pay.
fn billing_on(state: &AppState) -> Result<(), ApiError> {
    if state.billing.preview() {
        Err(ApiError::BillingPreview)
    } else {
        Ok(())
    }
}

/// Personal accounts are always free: there is nothing to pay for.
fn not_personal(membership: &Membership) -> Result<(), ApiError> {
    if membership.personal {
        Err(ApiError::PersonalAccountFree {
            account: membership.organization.login.clone(),
        })
    } else {
        Ok(())
    }
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
    billing_on(&state)?;
    not_personal(&membership)?;
    let org = &membership.organization;
    let back = account_url(&state, org);
    let url = state
        .billing
        .checkout(
            org.id,
            &org.login,
            state.members(org.id, &org.login).await,
            &format!("{back}&checkout=success"),
            &back,
        )
        .await?;
    Ok(Json(json!({ "url": url })))
}

#[derive(Deserialize)]
struct BillingEmailRequest {
    billing_email: Option<String>,
}

/// The `{"billing_email": …}` of a request body, checked. It is kept by Stripe, and never logged.
fn billing_email(body: &[u8]) -> Result<String, ApiError> {
    serde_json::from_slice::<BillingEmailRequest>(body)
        .ok()
        .and_then(|r| r.billing_email)
        .map(|e| e.trim().to_owned())
        .filter(|e| billing::email_is_valid(e))
        .ok_or(ApiError::BillingEmailInvalid)
}

/// Starts the organisation's no-card trial. The billing email is required, so that Stripe's reminder before the
/// trial ends reaches someone.
async fn trial(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    billing_on(&state)?;
    not_personal(&membership)?;
    let org = &membership.organization;
    let email = billing_email(&body)?;
    state
        .billing
        .start_trial(
            org.id,
            &org.login,
            state.members(org.id, &org.login).await,
            Some(&email),
        )
        .await?;
    tracing::info!(org = %org.login, by = %membership.user.login, "trial requested");
    Ok(Json(onboarding_doc(&state, &membership).await?))
}

/// Sets the billing email of an organisation that has a subscription, such as a trial started automatically when it
/// grew past the free limit, which has none.
async fn set_billing_email(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    billing_on(&state)?;
    not_personal(&membership)?;
    let org = &membership.organization;
    let email = billing_email(&body)?;
    state
        .billing
        .set_billing_email(org.id, &org.login, &email)
        .await?;
    tracing::info!(org = %org.login, by = %membership.user.login, "billing email set");
    Ok(Json(onboarding_doc(&state, &membership).await?))
}

async fn portal(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let membership = admin(&state, &session, &org).await?;
    billing_on(&state)?;
    not_personal(&membership)?;
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
