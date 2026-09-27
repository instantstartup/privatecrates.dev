//! Every error the registry returns, each with a stable code and HTTP status.
//!
//! Responses use Cargo's format, `{"errors":[{"detail":"…","code":"…"}]}`, with the message as the detail: Cargo
//! shows only that, so each message says what happened and what to do. The website uses the code.

use apollo_errors::Error;
use axum::{
    Json,
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use miette::Diagnostic;

use crate::{
    billing::BillingError, github::GitHubError, oidc::OidcError, records::RecordsError,
    tenant::TenantError,
};

#[derive(Debug, Error, Diagnostic)]
pub enum ApiError {
    // --- Authentication and access ---
    #[error("this registry needs a token; see {login_url}")]
    #[diagnostic(code(auth::token_required))]
    #[http_status(401)]
    TokenRequired {
        #[extension]
        login_url: String,
        /// Tells Cargo where to send the user (RFC 3139).
        #[http_header("WWW-Authenticate")]
        challenge: HeaderValue,
    },

    #[error("GitHub rejected the token; it may have expired. Run `cargo login` for this registry")]
    #[diagnostic(code(auth::token_rejected))]
    #[http_status(401)]
    TokenRejected,

    #[error("the registry token is invalid or has expired")]
    #[diagnostic(code(auth::registry_token_invalid))]
    #[http_status(401)]
    RegistryTokenInvalid,

    #[error(
        "GitHub Actions OIDC tokens can only be used to publish; exchange one for a registry token at \
         /api/v1/oidc/exchange to read"
    )]
    #[diagnostic(code(auth::oidc_token_for_publishing))]
    #[http_status(401)]
    OidcTokenForPublishing,

    #[error("the OIDC token is invalid: {reason}")]
    #[diagnostic(code(auth::oidc_token_invalid))]
    #[http_status(401)]
    OidcTokenInvalid { reason: String },

    #[error("could not fetch GitHub's OIDC signing keys; please try again")]
    #[diagnostic(code(auth::oidc_keys_unavailable))]
    #[http_status(503)]
    OidcKeysUnavailable { reason: String },

    #[error("send a GitHub Actions OIDC token in the Authorization header")]
    #[diagnostic(code(auth::oidc_token_required))]
    #[http_status(400)]
    OidcTokenRequired,

    #[error(
        "your GitHub token is not authorised for this organisation's SAML single sign-on. Authorise it at {url} \
         and try again"
    )]
    #[diagnostic(code(auth::sso_required))]
    #[http_status(403)]
    SsoRequired {
        #[extension]
        url: String,
    },

    /// Also what a name with no registry answers, so neither names the organisation (SPEC §6.7).
    #[error(
        "you do not have access to any repository in the GitHub organisation that uses this registry, or there is \
         no registry by this name"
    )]
    #[diagnostic(code(auth::no_access))]
    #[http_status(403)]
    NoAccess,

    #[error("the workflow's repository is not in the GitHub organisation that uses this registry")]
    #[diagnostic(code(auth::workflow_outside_organisation))]
    #[http_status(403)]
    WorkflowOutsideOrganisation,

    #[error("the OIDC token has no valid repository_id")]
    #[diagnostic(code(auth::oidc_repository_missing))]
    #[http_status(403)]
    OidcRepositoryMissing,

    #[error("{action} needs push access to {repository}")]
    #[diagnostic(code(auth::push_required))]
    #[http_status(403)]
    PushRequired {
        action: String,
        #[extension]
        repository: String,
    },

    /// Also used for anything the caller may not read, so that its existence is not revealed.
    #[error("not found")]
    #[diagnostic(code(registry::not_found))]
    #[http_status(404)]
    NotFound,

    // --- Publishing: the request ---
    #[error("the publish request body is truncated")]
    #[diagnostic(code(publish::body_truncated))]
    #[http_status(400)]
    BodyTruncated,

    #[error("the publish metadata is invalid: {reason}")]
    #[diagnostic(code(publish::metadata_invalid))]
    #[http_status(400)]
    MetadataInvalid { reason: String },

    #[error("the .crate file is {size} bytes; the limit is {limit}")]
    #[diagnostic(code(publish::crate_too_large))]
    #[http_status(413)]
    CrateTooLarge {
        #[extension]
        size: usize,
        #[extension]
        limit: usize,
    },

    #[error("{reason}")]
    #[diagnostic(code(publish::name_invalid))]
    #[http_status(400)]
    NameInvalid { reason: String },

    #[error("`{version}` is not a valid semver version: {reason}")]
    #[diagnostic(code(publish::version_invalid))]
    #[http_status(400)]
    VersionInvalid { version: String, reason: String },

    #[error("{reason}")]
    #[diagnostic(code(publish::crate_file_invalid))]
    #[http_status(400)]
    CrateFileInvalid { reason: String },

    #[error("too many publishes with this token; please wait a minute")]
    #[diagnostic(code(publish::rate_limited))]
    #[http_status(429)]
    PublishRateLimited,

    // --- Publishing: who may publish ---
    #[error(
        "{name} is published from GitHub Actions only, so every version can be traced to a commit. To publish it: \
         in its repository run `cargo privatecrates init --registry {slug}`, merge the workflow it adds, then push \
         the tag {name}-v{version} (in a workspace) or v{version}. An organisation admin can allow publishing from \
         developers' machines instead, with allow_manual_publish = true in privatecrates.toml in the storage \
         repository. See {base_url}/login#publish"
    )]
    #[diagnostic(code(publish::ci_only))]
    #[http_status(403)]
    CiOnly {
        name: String,
        version: String,
        slug: String,
        base_url: String,
    },

    #[error(
        "{name} {version} was not packaged from a clean git checkout, so it cannot be traced to a commit. Commit \
         your changes and run cargo publish again, without --allow-dirty"
    )]
    #[diagnostic(code(publish::not_clean))]
    #[http_status(403)]
    ManualPublishNotClean { name: String, version: String },

    #[error(
        "the first publish of {name} needs `package.repository` in its Cargo.toml to name its GitHub repository in \
         {org} (found: {declared})"
    )]
    #[diagnostic(code(publish::repository_required))]
    #[http_status(403)]
    ManualPublishRepository {
        name: String,
        org: String,
        declared: String,
    },

    #[error(
        "{reason}. A publish token must be requested for exactly this crate, version and checksum (audience \
         {audience}); use the cargo-credential-privatecrates credential provider"
    )]
    #[diagnostic(code(publish::token_not_bound))]
    #[http_status(403)]
    PublishTokenNotBound {
        reason: String,
        #[extension]
        audience: String,
    },

    #[error("the publishing workflow must be defined in the crate's own repository")]
    #[diagnostic(code(publish::workflow_elsewhere))]
    #[http_status(403)]
    WorkflowElsewhere,

    #[error("{name} is owned by {owner}; it cannot be published from {repository}")]
    #[diagnostic(code(publish::not_owning_repository))]
    #[http_status(403)]
    NotOwningRepository {
        name: String,
        owner: String,
        repository: String,
    },

    #[error(
        "the workflow {workflow} is not allowed to publish {name}; allowed: {allowed}. An administrator can \
         change this in {owners_file} in the storage repository"
    )]
    #[diagnostic(code(publish::workflow_not_allowed))]
    #[http_status(403)]
    WorkflowNotAllowed {
        workflow: String,
        name: String,
        allowed: String,
        owners_file: String,
    },

    #[error("{name} can only be published from a job in the `{environment}` GitHub environment")]
    #[diagnostic(code(publish::environment_required))]
    #[http_status(403)]
    EnvironmentRequired { name: String, environment: String },

    #[error(
        "{name} cannot be published from a workflow triggered by `{event}`: only push, release and \
         workflow_dispatch are allowed, because each needs Write access to the repository. Other events, such as \
         pull_request_target, issue_comment, pull_request, workflow_run, schedule, merge_group and \
         repository_dispatch, can run a workflow for people without it. Trigger the publish workflow by pushing a \
         tag, publishing a release or running it by hand"
    )]
    #[diagnostic(code(publish::trigger_not_allowed))]
    #[http_status(403)]
    TriggerNotAllowed { name: String, event: String },

    #[error(
        "{actor} started this workflow, but publishing {name} needs Write access to {repository} (permission to \
         create releases), and {actor} does not have it. {advice}"
    )]
    #[diagnostic(code(publish::actor_cannot_release))]
    #[http_status(403)]
    ActorCannotRelease {
        actor: String,
        name: String,
        #[extension]
        repository: String,
        advice: String,
    },

    #[error(
        "the first publish of {name} must come from the repository in its Cargo.toml `package.repository` \
         ({declared}), but this workflow is in {repository}"
    )]
    #[diagnostic(code(publish::first_publish_repository))]
    #[http_status(403)]
    FirstPublishRepository {
        name: String,
        declared: String,
        repository: String,
    },

    #[error(
        "registry tokens from the OIDC exchange are read-only; publishing uses an OIDC token bound to the crate"
    )]
    #[diagnostic(code(publish::registry_token_read_only))]
    #[http_status(403)]
    RegistryTokenReadOnly,

    // --- Publishing: what is published ---
    #[error(
        "{name} {version} is already published; versions cannot be replaced, even when yanked. Publish a new \
         version"
    )]
    #[diagnostic(code(publish::version_exists))]
    #[http_status(409)]
    VersionExists { name: String, version: String },

    #[error(
        "a crate named {name} exists on crates.io. A dependency that forgets `registry = \"{slug}\"` would get \
         that crate instead of yours. Choose another name, or set name_clash = \"warn\" in privatecrates.toml"
    )]
    #[diagnostic(code(publish::name_on_crates_io))]
    #[http_status(403)]
    NameOnCratesIo { name: String, slug: String },

    #[error("could not check whether this name exists on crates.io; please try again")]
    #[diagnostic(code(publish::crates_io_unavailable))]
    #[http_status(503)]
    CratesIoUnavailable,

    #[error("the dependency {name} is not in this registry")]
    #[diagnostic(code(publish::dependency_not_found))]
    #[http_status(400)]
    DependencyNotFound { name: String },

    #[error(
        "the dependency {name} is from another registry ({registry}); only this registry and crates.io are \
         allowed"
    )]
    #[diagnostic(code(publish::dependency_registry))]
    #[http_status(400)]
    DependencyRegistry { name: String, registry: String },

    // --- Publishing: storage ---
    #[error("{name} was created by another publish at the same moment; try again")]
    #[diagnostic(code(publish::created_concurrently))]
    #[http_status(409)]
    CreatedConcurrently { name: String },

    #[error("{name} {version} was published concurrently")]
    #[diagnostic(code(publish::published_concurrently))]
    #[http_status(409)]
    PublishedConcurrently { name: String, version: String },

    #[error(
        "{name} {version} already has an immutable release with different contents, left by an earlier publish \
         that did not finish. That version can never be used; publish a new version"
    )]
    #[diagnostic(code(publish::release_conflict))]
    #[http_status(409)]
    ReleaseConflict { name: String, version: String },

    #[error(
        "GitHub's checksum of the uploaded .crate does not match; nothing was published. Please try again"
    )]
    #[diagnostic(code(publish::digest_mismatch))]
    #[http_status(502)]
    DigestMismatch,

    #[error(
        "immutable releases are not enabled on {repository}, so published crates could be altered. An \
         organisation administrator must enable them (repository Settings → General → Releases); nothing was \
         published"
    )]
    #[diagnostic(code(publish::immutable_releases_disabled))]
    #[http_status(424)]
    ImmutableReleasesDisabled {
        #[extension]
        repository: String,
    },

    #[error("the index kept changing; please try again")]
    #[diagnostic(code(registry::index_contention))]
    #[http_status(409)]
    IndexContention,

    // --- Webhooks ---
    #[error("webhooks are not configured on this server: WEBHOOK_SECRET is not set")]
    #[diagnostic(code(webhook::not_configured))]
    #[http_status(503)]
    WebhooksNotConfigured,

    #[error("the webhook's X-Hub-Signature-256 is missing or does not match")]
    #[diagnostic(code(webhook::signature_invalid))]
    #[http_status(401)]
    WebhookSignatureInvalid,

    #[error("the webhook payload is invalid: {reason}")]
    #[diagnostic(code(webhook::payload_invalid))]
    #[http_status(400)]
    WebhookPayloadInvalid { reason: String },
    // --- Billing ---
    #[error(
        "the PrivateCrates subscription for the {org} organisation is not active. An organisation admin can renew \
         it at {account_url}"
    )]
    #[diagnostic(code(billing::subscription_inactive))]
    #[http_status(402)]
    SubscriptionInactive {
        org: String,
        #[extension]
        account_url: String,
    },

    #[error("billing is not set up on this server")]
    #[diagnostic(code(billing::not_configured))]
    #[http_status(503)]
    BillingNotConfigured,

    #[error("PrivateCrates is free during the preview; there is nothing to pay")]
    #[diagnostic(code(billing::preview))]
    #[http_status(409)]
    BillingPreview,

    #[error("{org} already has a subscription; manage it from the billing portal")]
    #[diagnostic(code(billing::already_subscribed))]
    #[http_status(409)]
    AlreadySubscribed { org: String },

    #[error("{org} has already had its free trial; subscribe with a card instead")]
    #[diagnostic(code(billing::trial_used))]
    #[http_status(409)]
    TrialUsed { org: String },

    #[error("{org} has at most {limit} members, so PrivateCrates is free for it; nothing to pay")]
    #[diagnostic(code(billing::free_plan))]
    #[http_status(409)]
    FreePlan {
        org: String,
        #[extension]
        limit: u64,
    },

    #[error("{org} can start a free trial with no card; start that instead")]
    #[diagnostic(code(billing::trial_available))]
    #[http_status(409)]
    TrialAvailable { org: String },

    #[error(
        "starting the trial needs a valid billing email (`billing_email`), such as billing@example.com, so that the \
         reminder before the trial ends reaches someone"
    )]
    #[diagnostic(code(billing::email_invalid))]
    #[http_status(400)]
    BillingEmailInvalid,

    #[error("{org} has no subscription yet; start one first")]
    #[diagnostic(code(billing::no_subscription))]
    #[http_status(409)]
    NoSubscription { org: String },

    #[error("a request to Stripe failed; please try again")]
    #[diagnostic(code(billing::stripe_failed))]
    #[http_status(502)]
    Stripe {
        #[source]
        source: BillingError,
    },

    #[error("the Stripe webhook is invalid: {reason}")]
    #[diagnostic(code(billing::webhook_invalid))]
    #[http_status(400)]
    StripeWebhookInvalid { reason: String },

    // --- The website's account API ---
    #[error("sign in with GitHub first")]
    #[diagnostic(code(account::sign_in_required))]
    #[http_status(401)]
    SignInRequired,

    #[error(
        "the account API accepts only a PrivateCrates sign-in token (`ghu_…`, from `cargo privatecrates login`); \
         other GitHub tokens are refused here, as they are usually far broader than it needs"
    )]
    #[diagnostic(code(account::token_not_accepted))]
    #[http_status(401)]
    AccountTokenNotAccepted,

    #[error("this request must come from the PrivateCrates website")]
    #[diagnostic(code(account::cross_site_request))]
    #[http_status(403)]
    CrossSiteRequest,

    #[error(
        "this sign-in link has expired or was started in another browser; please sign in again"
    )]
    #[diagnostic(code(account::sign_in_state_invalid))]
    #[http_status(400)]
    SignInStateInvalid,

    #[error("GitHub did not complete the sign-in; please try again")]
    #[diagnostic(code(account::sign_in_failed))]
    #[http_status(400)]
    SignInFailed,

    #[error("you are not a member of a GitHub organisation called {org}")]
    #[diagnostic(code(account::org_not_found))]
    #[http_status(404)]
    OrgNotFound { org: String },

    #[error("only admins of the {org} organisation can do this; ask one of them")]
    #[diagnostic(code(account::admin_required))]
    #[http_status(403)]
    AdminRequired { org: String },

    #[error(
        "a registry name must be 1 to 63 lowercase letters, digits or hyphens, and cannot start or end with a \
         hyphen"
    )]
    #[diagnostic(code(account::slug_invalid))]
    #[http_status(400)]
    SlugInvalid,

    #[error("the registry name {slug} is reserved; choose another")]
    #[diagnostic(code(account::slug_reserved))]
    #[http_status(400)]
    SlugReserved { slug: String },

    #[error("the registry name {slug} is taken; choose another")]
    #[diagnostic(code(account::slug_taken))]
    #[http_status(409)]
    SlugTaken { slug: String },

    #[error(
        "privatecrates.toml already exists in {repository}. Settings are changed with a pull request to that \
         file, not here"
    )]
    #[diagnostic(code(account::settings_exist))]
    #[http_status(409)]
    SettingsExist {
        #[extension]
        repository: String,
    },

    #[error(
        "creating a registry needs an organisation admin to accept the PrivateCrates terms: read them at \
         {terms_url}, then send `accept_terms` with the current version, {version}"
    )]
    #[diagnostic(code(account::terms_not_accepted))]
    #[http_status(400)]
    TermsNotAccepted {
        #[extension]
        version: &'static str,
        #[extension]
        terms_url: String,
    },

    #[error("{org} has no registry yet; its terms are accepted when an admin sets it up")]
    #[diagnostic(code(account::not_set_up))]
    #[http_status(409)]
    NotSetUp { org: String },

    #[error(
        "the storage App must be installed on exactly one repository of {org}, the storage repository, before \
         the registry can be set up"
    )]
    #[diagnostic(code(account::storage_not_ready))]
    #[http_status(409)]
    StorageNotReady { org: String },

    // --- The compliance dashboard ---
    #[error(
        "{org} has no registry yet, so there is nothing to report; set one up at {account_url}"
    )]
    #[diagnostic(code(compliance::not_set_up))]
    #[http_status(404)]
    ComplianceNotSetUp {
        org: String,
        #[extension]
        account_url: String,
    },

    // --- GitHub and internal ---
    #[error("GitHub's rate limit was reached; please try again in a few minutes")]
    #[diagnostic(code(github::rate_limited))]
    #[http_status(503)]
    GitHubRateLimited,

    #[error("a request to GitHub failed; please try again")]
    #[diagnostic(code(github::request_failed))]
    #[http_status(502)]
    GitHub {
        #[source]
        source: GitHubError,
    },

    #[error("our records are unavailable, so nothing was changed; please try again")]
    #[diagnostic(code(records::unavailable))]
    #[http_status(503)]
    Records {
        #[source]
        source: RecordsError,
    },

    #[error("internal error; please try again")]
    #[diagnostic(code(registry::internal))]
    Internal { reason: String },
}

impl ApiError {
    pub fn token_required(login_url: String) -> Self {
        let challenge = HeaderValue::from_str(&format!("Cargo login_url=\"{login_url}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("Cargo"));
        Self::TokenRequired {
            login_url,
            challenge,
        }
    }

    pub fn internal(reason: impl Into<String>) -> Self {
        Self::Internal {
            reason: reason.into(),
        }
    }
}

impl From<GitHubError> for ApiError {
    fn from(e: GitHubError) -> Self {
        match e {
            GitHubError::Sso { url } => Self::SsoRequired { url },
            GitHubError::Unauthorized => Self::TokenRejected,
            GitHubError::RateLimited => Self::GitHubRateLimited,
            source => Self::GitHub { source },
        }
    }
}

impl From<TenantError> for ApiError {
    fn from(e: TenantError) -> Self {
        match e {
            TenantError::GitHub { source } => source.into(),
            other => Self::internal(other.to_string()),
        }
    }
}

impl From<BillingError> for ApiError {
    fn from(source: BillingError) -> Self {
        Self::Stripe { source }
    }
}

impl From<RecordsError> for ApiError {
    fn from(source: RecordsError) -> Self {
        Self::Records { source }
    }
}

impl From<OidcError> for ApiError {
    fn from(e: OidcError) -> Self {
        match e {
            OidcError::Invalid { reason } => Self::OidcTokenInvalid { reason },
            OidcError::Keys { reason } => Self::OidcKeysUnavailable { reason },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.http_status();
        let code = self.code().map(|c| c.to_string());
        if status.is_server_error() {
            tracing::error!(error = ?self, code = ?code, "request failed");
        }
        let body =
            Json(serde_json::json!({ "errors": [{ "detail": self.to_string(), "code": code }] }));
        let mut response = (status, body).into_response();
        response.headers_mut().extend(self.http_headers());
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{StatusCode, header};

    #[tokio::test]
    async fn responses_use_cargos_format() {
        let response =
            ApiError::token_required("https://acme.privatecrates.dev/login".into()).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[header::WWW_AUTHENTICATE],
            "Cargo login_url=\"https://acme.privatecrates.dev/login\""
        );
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["errors"][0]["detail"],
            "this registry needs a token; see https://acme.privatecrates.dev/login"
        );
        assert_eq!(body["errors"][0]["code"], "auth::token_required");
    }

    #[test]
    fn github_errors_map_to_specific_errors() {
        let sso: ApiError = GitHubError::Sso { url: "u".into() }.into();
        assert!(matches!(sso, ApiError::SsoRequired { .. }));
        assert_eq!(sso.http_status(), StatusCode::FORBIDDEN);
        let other: ApiError = GitHubError::NotFound.into();
        assert_eq!(other.http_status(), StatusCode::BAD_GATEWAY);
    }
}
