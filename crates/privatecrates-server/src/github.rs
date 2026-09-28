//! The few GitHub REST endpoints PrivateCrates uses.
//!
//! Two kinds of calls: with an installation token of one of our Apps (storage reads and writes, tenant discovery),
//! and with a caller's own token (permission lookups on their behalf, SPEC §6.3). Every call's outcome and latency is
//! recorded for `GET /api/status`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apollo_errors::Error;
use base64::Engine;
use bytes::Bytes;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use miette::Diagnostic;
use reqwest::{Method, RequestBuilder, Response, StatusCode, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

use crate::{
    config::{AppConfig, Config},
    metrics::{Metrics, SendRecorded},
};

const USER_AGENT: &str = "privatecrates (https://privatecrates.dev)";
const PER_PAGE: usize = 100;

#[derive(Debug, Error, Diagnostic)]
pub enum GitHubError {
    #[error("the token is not authorised for SAML single sign-on")]
    #[diagnostic(code(github::sso_required))]
    Sso { url: String },
    #[error("GitHub rejected the token")]
    #[diagnostic(code(github::unauthorized))]
    Unauthorized,
    #[error("not found")]
    #[diagnostic(code(github::not_found))]
    NotFound,
    /// A write lost an optimistic-concurrency race.
    #[error("conflict")]
    #[diagnostic(code(github::conflict))]
    Conflict,
    #[error("rate limited")]
    #[diagnostic(code(github::rate_limited))]
    RateLimited,
    #[error("GitHub answered {status}: {body}")]
    #[diagnostic(code(github::status))]
    Status { status: StatusCode, body: String },
    #[error("request failed: {source}")]
    #[diagnostic(code(github::http))]
    Http {
        #[from]
        source: reqwest::Error,
    },
    #[error("unexpected response: {reason}")]
    #[diagnostic(code(github::unexpected))]
    Unexpected { reason: String },
    #[error("could not sign the App JWT: {source}")]
    #[diagnostic(code(github::app_jwt))]
    Jwt {
        #[from]
        source: jsonwebtoken::errors::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppKind {
    Reader,
    Storage,
}

struct App {
    id: u64,
    key: EncodingKey,
}

impl App {
    fn new(config: &AppConfig) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            id: config.id,
            key: EncodingKey::from_rsa_pem(&config.private_key_pem)?,
        })
    }

    fn jwt(&self) -> Result<String, jsonwebtoken::errors::Error> {
        #[derive(Serialize)]
        struct Claims {
            iat: u64,
            exp: u64,
            iss: String,
        }
        let now = now_secs();
        jsonwebtoken::encode(
            &Header::new(Algorithm::RS256),
            &Claims {
                iat: now - 60,
                exp: now + 9 * 60,
                iss: self.id.to_string(),
            },
            &self.key,
        )
    }
}

pub struct GitHub {
    http: reqwest::Client,
    no_redirect: reqwest::Client,
    api: Url,
    /// `https://github.com`, where the web sign-in flow exchanges its codes.
    web: Url,
    reader: App,
    storage: App,
    installation_tokens: moka::future::Cache<(AppKind, u64), String>,
    metrics: Metrics,
}

// Response types: only the fields we use.

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub login: String,
    pub id: u64,
    /// `Organization`, or `User` for a personal account.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

impl Account {
    pub fn is_personal(&self) -> bool {
        self.kind.as_deref() == Some("User")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Installation {
    pub id: u64,
    pub account: Account,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub pull: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub admin: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: Account,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub permissions: Option<Permissions>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Organization {
    pub id: u64,
    pub login: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// The signed-in user's membership of an organisation.
#[derive(Debug, Clone, Deserialize)]
pub struct Membership {
    /// `active` or `pending` (invited).
    pub state: String,
    /// `admin` or `member`.
    pub role: String,
    pub organization: Organization,
    pub user: Account,
    /// A person's own account, which they administer alone: GitHub has no membership for it.
    #[serde(skip)]
    pub personal: bool,
}

impl Membership {
    pub fn is_admin(&self) -> bool {
        self.state == "active" && self.role == "admin"
    }
}

/// A user's permission on a repository.
#[derive(Debug, Clone, Deserialize)]
pub struct CollaboratorPermission {
    /// The base role: `admin`, `write`, `read` or `none`. GitHub maps `maintain` to `write`, `triage` to `read`,
    /// and a custom role to the role it is based on.
    pub permission: String,
    #[serde(default)]
    pub user: Option<Account>,
}

impl CollaboratorPermission {
    /// Whether the permission includes creating releases: Write, Maintain or Admin.
    pub fn can_release(&self) -> bool {
        matches!(self.permission.as_str(), "write" | "admin")
    }
}

/// A user access token from the web sign-in flow.
pub struct UserToken {
    pub access_token: String,
    /// Seconds until it expires; GitHub App user tokens last 8 hours.
    pub expires_in: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tree {
    pub tree: Vec<TreeEntry>,
    #[serde(default)]
    pub truncated: bool,
}

/// A commit, as the list-commits API gives it.
#[derive(Debug, Clone, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub commit: CommitDetail,
    /// The GitHub account the commit's author email belongs to, if any.
    #[serde(default)]
    pub author: Option<CommitAccount>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitDetail {
    pub message: String,
    #[serde(default)]
    pub author: Option<Signature>,
    #[serde(default)]
    pub committer: Option<Signature>,
    #[serde(default)]
    pub verification: Option<Verification>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitAccount {
    pub login: String,
}

/// A commit's author or committer, as git records them.
#[derive(Debug, Clone, Deserialize)]
pub struct Signature {
    pub name: String,
    pub date: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verification {
    pub verified: bool,
}

/// One file write: the file's current blob sha (`None` to create it), its new content, and the commit message.
pub struct FileWrite<'a> {
    pub path: &'a str,
    pub content: &'a [u8],
    pub sha: Option<&'a str>,
    pub message: &'a str,
}

pub enum Conditional<T> {
    NotModified,
    Modified { value: T, etag: Option<String> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    /// `sha256:<hex>`, computed by GitHub at upload.
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub id: u64,
    pub tag_name: String,
    pub draft: bool,
    #[serde(default)]
    pub immutable: bool,
    pub upload_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl GitHub {
    pub fn new(config: &Config) -> Result<Self, GitHubError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()?;
        let no_redirect = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            no_redirect,
            api: config.github_api.clone(),
            web: config.github_web.clone(),
            reader: App::new(&config.reader_app)?,
            storage: App::new(&config.storage_app)?,
            installation_tokens: moka::future::Cache::builder()
                .max_capacity(100_000)
                // Installation tokens last an hour; refresh well before that.
                .time_to_live(Duration::from_secs(45 * 60))
                .build(),
            metrics: Metrics::default(),
        })
    }

    /// The outcomes and latencies of our recent calls to GitHub.
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    fn url(&self, path: &str) -> Url {
        self.api
            .join(path.trim_start_matches('/'))
            .expect("API paths are valid URL paths")
    }

    fn request(&self, method: Method, path: &str, token: &str) -> RequestBuilder {
        with_headers(self.http.request(method, self.url(path)), token)
    }

    fn app(&self, kind: AppKind) -> &App {
        match kind {
            AppKind::Reader => &self.reader,
            AppKind::Storage => &self.storage,
        }
    }

    // --- App and installation authentication ---

    pub async fn installations(&self, kind: AppKind) -> Result<Vec<Installation>, GitHubError> {
        let jwt = self.app(kind).jwt()?;
        let mut all = Vec::new();
        for page in 1.. {
            let batch: Vec<Installation> = json(
                self.request(Method::GET, "/app/installations", &jwt)
                    .query(&[("per_page", PER_PAGE), ("page", page)])
                    .send_recorded(&self.metrics)
                    .await?,
            )
            .await?;
            let done = batch.len() < PER_PAGE;
            all.extend(batch);
            if done {
                break;
            }
        }
        Ok(all)
    }

    pub async fn installation_token(
        &self,
        kind: AppKind,
        installation_id: u64,
    ) -> Result<String, GitHubError> {
        if let Some(token) = self.installation_tokens.get(&(kind, installation_id)).await {
            return Ok(token);
        }
        #[derive(Deserialize)]
        struct TokenResponse {
            token: String,
        }
        let jwt = self.app(kind).jwt()?;
        let response: TokenResponse = json(
            self.request(
                Method::POST,
                &format!("/app/installations/{installation_id}/access_tokens"),
                &jwt,
            )
            .send_recorded(&self.metrics)
            .await?,
        )
        .await?;
        self.installation_tokens
            .insert((kind, installation_id), response.token.clone())
            .await;
        Ok(response.token)
    }

    /// An App's installation on an organisation, if it is installed there.
    pub async fn org_installation(
        &self,
        kind: AppKind,
        org: &str,
    ) -> Result<Option<Installation>, GitHubError> {
        self.installation_at(kind, &format!("/orgs/{org}/installation"))
            .await
    }

    /// An App's installation on a personal account, if it is installed there.
    pub async fn user_installation(
        &self,
        kind: AppKind,
        login: &str,
    ) -> Result<Option<Installation>, GitHubError> {
        self.installation_at(kind, &format!("/users/{login}/installation"))
            .await
    }

    /// An App's installation on an organisation or a personal account.
    pub async fn account_installation(
        &self,
        kind: AppKind,
        login: &str,
        personal: bool,
    ) -> Result<Option<Installation>, GitHubError> {
        if personal {
            self.user_installation(kind, login).await
        } else {
            self.org_installation(kind, login).await
        }
    }

    async fn installation_at(
        &self,
        kind: AppKind,
        path: &str,
    ) -> Result<Option<Installation>, GitHubError> {
        let jwt = self.app(kind).jwt()?;
        match json(
            self.request(Method::GET, path, &jwt)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
        {
            Ok(installation) => Ok(Some(installation)),
            Err(GitHubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The repositories an installation token has access to.
    pub async fn installation_repositories(&self, token: &str) -> Result<Vec<Repo>, GitHubError> {
        #[derive(Deserialize)]
        struct Page {
            total_count: usize,
            repositories: Vec<Repo>,
        }
        let mut all = Vec::new();
        for page in 1.. {
            let batch: Page = json(
                self.request(Method::GET, "/installation/repositories", token)
                    .query(&[("per_page", PER_PAGE), ("page", page)])
                    .send_recorded(&self.metrics)
                    .await?,
            )
            .await?;
            let empty = batch.repositories.is_empty();
            all.extend(batch.repositories);
            if empty || all.len() >= batch.total_count {
                break;
            }
        }
        Ok(all)
    }

    /// The number of an organisation's members, with an installation token that may read them. Outside
    /// collaborators and pending invitations are not members.
    pub async fn org_member_count(&self, token: &str, org: &str) -> Result<u64, GitHubError> {
        let mut count = 0;
        for page in 1.. {
            let batch: Vec<serde::de::IgnoredAny> = json(
                self.request(Method::GET, &format!("/orgs/{org}/members"), token)
                    .query(&[("per_page", PER_PAGE), ("page", page)])
                    .send_recorded(&self.metrics)
                    .await?,
            )
            .await?;
            count += batch.len() as u64;
            if batch.len() < PER_PAGE {
                break;
            }
        }
        Ok(count)
    }

    /// A user's permission on a repository (`owner/name`), with an installation token that can read the
    /// repository's metadata. `None` if they are not a collaborator, or there is no such user.
    pub async fn collaborator_permission(
        &self,
        token: &str,
        repo: &str,
        username: &str,
    ) -> Result<Option<CollaboratorPermission>, GitHubError> {
        match json(
            self.request(
                Method::GET,
                &format!("/repos/{repo}/collaborators/{username}/permission"),
                token,
            )
            .send_recorded(&self.metrics)
            .await?,
        )
        .await
        {
            Ok(permission) => Ok(Some(permission)),
            Err(GitHubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // --- Calls with the caller's token ---

    pub async fn user(&self, token: &str) -> Result<User, GitHubError> {
        json(
            self.request(Method::GET, "/user", token)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
    }

    /// The organisations the user belongs to.
    pub async fn user_orgs(&self, token: &str) -> Result<Vec<Organization>, GitHubError> {
        let mut all = Vec::new();
        for page in 1.. {
            let batch: Vec<Organization> = json(
                self.request(Method::GET, "/user/orgs", token)
                    .query(&[("per_page", PER_PAGE), ("page", page)])
                    .send_recorded(&self.metrics)
                    .await?,
            )
            .await?;
            let done = batch.len() < PER_PAGE;
            all.extend(batch);
            if done {
                break;
            }
        }
        Ok(all)
    }

    /// Exchanges a web sign-in flow's code for a user access token.
    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
    ) -> Result<UserToken, GitHubError> {
        #[derive(Deserialize)]
        struct Response {
            access_token: Option<String>,
            expires_in: Option<u64>,
            error: Option<String>,
        }
        let url = self
            .web
            .join("login/oauth/access_token")
            .expect("a valid URL path");
        let response: Response = json(
            self.http
                .post(url)
                .header(header::ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                    ("code", code),
                    ("redirect_uri", redirect_uri),
                ])
                .send_recorded(&self.metrics)
                .await?,
        )
        .await?;
        match response.access_token {
            Some(access_token) => Ok(UserToken {
                access_token,
                expires_in: response.expires_in,
            }),
            // GitHub answers 200 with an error code, such as `bad_verification_code`.
            None => Err(GitHubError::Unexpected {
                reason: response.error.unwrap_or_else(|| "no access token".into()),
            }),
        }
    }

    /// The repositories in an installation that the user behind a GitHub App user token can access, with their
    /// permissions (SPEC §6.3). A user with no access to the installation gets an empty list.
    pub async fn user_installation_repositories(
        &self,
        token: &str,
        installation_id: u64,
    ) -> Result<Vec<Repo>, GitHubError> {
        #[derive(Deserialize)]
        struct Page {
            total_count: usize,
            repositories: Vec<Repo>,
        }
        let mut all = Vec::new();
        for page in 1.. {
            let result = json::<Page>(
                self.request(
                    Method::GET,
                    &format!("/user/installations/{installation_id}/repositories"),
                    token,
                )
                .query(&[("per_page", PER_PAGE), ("page", page)])
                .send_recorded(&self.metrics)
                .await?,
            )
            .await;
            let batch = match result {
                Ok(batch) => batch,
                Err(GitHubError::NotFound) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let empty = batch.repositories.is_empty();
            all.extend(batch.repositories);
            if empty || all.len() >= batch.total_count {
                break;
            }
        }
        Ok(all)
    }

    /// A repository by `owner/name`, as seen by `token`. `None` if it does not exist or the token cannot see it.
    pub async fn repository(
        &self,
        token: &str,
        full_name: &str,
    ) -> Result<Option<Repo>, GitHubError> {
        match json(
            self.request(Method::GET, &format!("/repos/{full_name}"), token)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
        {
            Ok(repo) => Ok(Some(repo)),
            Err(GitHubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// A repository by ID, as seen by `token`. `None` if the token cannot see it.
    pub async fn repository_by_id(
        &self,
        token: &str,
        id: u64,
    ) -> Result<Option<Repo>, GitHubError> {
        match json(
            self.request(Method::GET, &format!("/repositories/{id}"), token)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
        {
            Ok(repo) => Ok(Some(repo)),
            Err(GitHubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The user's membership of an organisation. `None` if they are not a member, or if the token may not read
    /// memberships (a personal access token without `read:org`).
    pub async fn org_membership(
        &self,
        token: &str,
        org: &str,
    ) -> Result<Option<Membership>, GitHubError> {
        match json(
            self.request(Method::GET, &format!("/user/memberships/orgs/{org}"), token)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
        {
            Ok(membership) => Ok(Some(membership)),
            Err(GitHubError::NotFound)
            | Err(GitHubError::Status {
                status: StatusCode::FORBIDDEN,
                ..
            }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // --- Storage repository: tree, blobs and contents ---

    /// The full tree of a branch, fetched conditionally: GitHub does not count a 304 against the rate limit.
    pub async fn tree(
        &self,
        token: &str,
        repo: &str,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<Tree>, GitHubError> {
        let mut request = self
            .request(
                Method::GET,
                &format!("/repos/{repo}/git/trees/{branch}"),
                token,
            )
            .query(&[("recursive", "1")]);
        if let Some(etag) = etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let response = request.send_recorded(&self.metrics).await?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(Conditional::NotModified);
        }
        // A repository with no commits yet, such as a newly created storage repository, has no tree: GitHub answers
        // 409 "Git Repository is empty". It holds no files.
        if response.status() == StatusCode::CONFLICT {
            return Ok(Conditional::Modified {
                value: Tree {
                    tree: Vec::new(),
                    truncated: false,
                },
                etag: None,
            });
        }
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let value: Tree = json(response).await?;
        Ok(Conditional::Modified { value, etag })
    }

    pub async fn blob(&self, token: &str, repo: &str, sha: &str) -> Result<Bytes, GitHubError> {
        #[derive(Deserialize)]
        struct Blob {
            content: String,
            encoding: String,
        }
        let blob: Blob = json(
            self.request(
                Method::GET,
                &format!("/repos/{repo}/git/blobs/{sha}"),
                token,
            )
            .send_recorded(&self.metrics)
            .await?,
        )
        .await?;
        match blob.encoding.as_str() {
            "base64" => decode_base64(&blob.content).map(Bytes::from),
            "utf-8" => Ok(Bytes::from(blob.content.into_bytes())),
            other => Err(GitHubError::Unexpected {
                reason: format!("blob encoding {other}"),
            }),
        }
    }

    /// A branch's commits, newest first; with `path`, only those that changed that file or directory.
    pub async fn commits(
        &self,
        token: &str,
        repo: &str,
        branch: &str,
        path: Option<&str>,
    ) -> Result<Vec<Commit>, GitHubError> {
        let mut all = Vec::new();
        for page in 1.. {
            let mut request = self
                .request(Method::GET, &format!("/repos/{repo}/commits"), token)
                .query(&[("sha", branch)])
                .query(&[("per_page", PER_PAGE), ("page", page)]);
            if let Some(path) = path {
                request = request.query(&[("path", path)]);
            }
            let batch: Vec<Commit> = match json(request.send_recorded(&self.metrics).await?).await {
                Ok(batch) => batch,
                // An empty repository has no commits: GitHub answers 409 "Git Repository is empty".
                Err(GitHubError::Status {
                    status: StatusCode::CONFLICT,
                    ..
                }) => break,
                Err(e) => return Err(e),
            };
            let done = batch.len() < PER_PAGE;
            all.extend(batch);
            if done {
                break;
            }
        }
        Ok(all)
    }

    /// Creates or updates one file in one commit. A mismatch between `write.sha` and the file's current blob sha
    /// is a [`GitHubError::Conflict`]. Returns the new blob sha.
    /// `branch` is `None` for the repository's default branch, which also works in an empty repository, where the
    /// write becomes the first commit.
    pub async fn put_file(
        &self,
        token: &str,
        repo: &str,
        branch: Option<&str>,
        write: FileWrite<'_>,
    ) -> Result<String, GitHubError> {
        #[derive(Serialize)]
        struct Body<'a> {
            message: &'a str,
            content: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            branch: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            sha: Option<&'a str>,
        }
        #[derive(Deserialize)]
        struct Content {
            sha: String,
        }
        #[derive(Deserialize)]
        struct Response {
            content: Content,
        }
        let response = self
            .request(
                Method::PUT,
                &format!("/repos/{repo}/contents/{}", write.path),
                token,
            )
            .json(&Body {
                message: write.message,
                content: base64::engine::general_purpose::STANDARD.encode(write.content),
                branch,
                sha: write.sha,
            })
            .send_recorded(&self.metrics)
            .await?;
        let status = response.status();
        if status == StatusCode::CONFLICT || status == StatusCode::UNPROCESSABLE_ENTITY {
            return Err(GitHubError::Conflict);
        }
        let response: Response = json(response).await?;
        Ok(response.content.sha)
    }

    // --- Releases ---

    pub async fn release_by_tag(
        &self,
        token: &str,
        repo: &str,
        tag: &str,
    ) -> Result<Option<Release>, GitHubError> {
        match json(
            self.request(
                Method::GET,
                &format!("/repos/{repo}/releases/tags/{tag}"),
                token,
            )
            .send_recorded(&self.metrics)
            .await?,
        )
        .await
        {
            Ok(release) => Ok(Some(release)),
            Err(GitHubError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Draft releases with this tag. Drafts have no tag yet, so they are found by listing.
    pub async fn drafts_for_tag(
        &self,
        token: &str,
        repo: &str,
        tag: &str,
    ) -> Result<Vec<Release>, GitHubError> {
        let mut drafts = Vec::new();
        for page in 1.. {
            let batch: Vec<Release> = json(
                self.request(Method::GET, &format!("/repos/{repo}/releases"), token)
                    .query(&[("per_page", PER_PAGE), ("page", page)])
                    .send_recorded(&self.metrics)
                    .await?,
            )
            .await?;
            let done = batch.len() < PER_PAGE;
            drafts.extend(batch.into_iter().filter(|r| r.draft && r.tag_name == tag));
            if done {
                break;
            }
        }
        Ok(drafts)
    }

    pub async fn create_draft_release(
        &self,
        token: &str,
        repo: &str,
        tag: &str,
        target: &str,
        body: &str,
    ) -> Result<Release, GitHubError> {
        json(
            self.request(Method::POST, &format!("/repos/{repo}/releases"), token)
                .json(&serde_json::json!({
                    "tag_name": tag,
                    "target_commitish": target,
                    "name": tag,
                    "body": body,
                    "draft": true,
                }))
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
    }

    pub async fn upload_asset(
        &self,
        token: &str,
        release: &Release,
        name: &str,
        content: Bytes,
    ) -> Result<Asset, GitHubError> {
        // `upload_url` is a URI template: `https://uploads.github.com/repos/o/r/releases/1/assets{?name,label}`.
        let base = release
            .upload_url
            .split('{')
            .next()
            .unwrap_or(&release.upload_url);
        let url = Url::parse(base).map_err(|e| GitHubError::Unexpected {
            reason: e.to_string(),
        })?;
        json(
            with_headers(self.http.post(url), token)
                .query(&[("name", name)])
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(content)
                .send_recorded(&self.metrics)
                .await?,
        )
        .await
    }

    pub async fn publish_release(
        &self,
        token: &str,
        repo: &str,
        release_id: u64,
    ) -> Result<Release, GitHubError> {
        json(
            self.request(
                Method::PATCH,
                &format!("/repos/{repo}/releases/{release_id}"),
                token,
            )
            .json(&serde_json::json!({ "draft": false }))
            .send_recorded(&self.metrics)
            .await?,
        )
        .await
    }

    pub async fn delete_release(
        &self,
        token: &str,
        repo: &str,
        release_id: u64,
    ) -> Result<(), GitHubError> {
        check(
            self.request(
                Method::DELETE,
                &format!("/repos/{repo}/releases/{release_id}"),
                token,
            )
            .send_recorded(&self.metrics)
            .await?,
        )
        .await
        .map(drop)
    }

    /// The short-lived signed URL GitHub redirects to for an asset's bytes (SPEC §4.3).
    pub async fn asset_download_url(
        &self,
        token: &str,
        repo: &str,
        asset_id: u64,
    ) -> Result<String, GitHubError> {
        let response = with_accept(
            self.no_redirect
                .get(self.url(&format!("/repos/{repo}/releases/assets/{asset_id}"))),
            token,
            "application/octet-stream",
        )
        .send_recorded(&self.metrics)
        .await?;
        if !response.status().is_redirection() {
            check(response).await?;
            return Err(GitHubError::Unexpected {
                reason: "asset download did not redirect".into(),
            });
        }
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Unexpected {
                reason: "redirect without Location".into(),
            })
    }

    /// Fetches a signed URL from [`Self::asset_download_url`]. No credentials: the signature authorises it.
    pub async fn download(&self, url: &str) -> Result<Bytes, GitHubError> {
        Ok(
            check(self.http.get(url).send_recorded(&self.metrics).await?)
                .await?
                .bytes()
                .await?,
        )
    }
}

fn with_headers(request: RequestBuilder, token: &str) -> RequestBuilder {
    with_accept(request, token, "application/vnd.github+json")
}

fn with_accept(request: RequestBuilder, token: &str, accept: &str) -> RequestBuilder {
    request
        .bearer_auth(token)
        .header(header::ACCEPT, accept)
        .header("X-GitHub-Api-Version", "2022-11-28")
}

/// Maps GitHub's error responses to [`GitHubError`].
async fn check(response: Response) -> Result<Response, GitHubError> {
    let status = response.status();
    if status.is_success() || status.is_redirection() {
        return Ok(response);
    }
    let headers = response.headers();
    if status == StatusCode::FORBIDDEN
        && let Some(sso) = headers.get("x-github-sso").and_then(|v| v.to_str().ok())
    {
        // `required; url=https://github.com/orgs/acme/sso?authorization_request=…`
        let url = sso
            .split(';')
            .find_map(|part| part.trim().strip_prefix("url="))
            .unwrap_or("https://github.com/settings/tokens")
            .to_owned();
        return Err(GitHubError::Sso { url });
    }
    let rate_limited = headers
        .get("x-ratelimit-remaining")
        .is_some_and(|v| v.as_bytes() == b"0");
    match status {
        StatusCode::UNAUTHORIZED => Err(GitHubError::Unauthorized),
        StatusCode::NOT_FOUND => Err(GitHubError::NotFound),
        StatusCode::TOO_MANY_REQUESTS => Err(GitHubError::RateLimited),
        StatusCode::FORBIDDEN if rate_limited => Err(GitHubError::RateLimited),
        _ => {
            let body = response.text().await.unwrap_or_default();
            Err(GitHubError::Status {
                status,
                body: body.chars().take(500).collect(),
            })
        }
    }
}

async fn json<T: DeserializeOwned>(response: Response) -> Result<T, GitHubError> {
    Ok(check(response).await?.json().await?)
}

fn decode_base64(content: &str) -> Result<Vec<u8>, GitHubError> {
    // GitHub wraps base64 content at 60 characters.
    let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|e| GitHubError::Unexpected {
            reason: format!("bad base64: {e}"),
        })
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_with_line_breaks() {
        assert_eq!(decode_base64("aGVs\nbG8=\n").unwrap(), b"hello");
    }

    #[test]
    fn write_maintain_and_admin_can_release() {
        // GitHub's `permission` for each role; `role_name` is ignored.
        for (role, permission, can) in [
            ("admin", "admin", true),
            ("maintain", "write", true),
            ("write", "write", true),
            ("triage", "read", false),
            ("read", "read", false),
            ("none", "none", false),
        ] {
            let response: CollaboratorPermission = serde_json::from_value(serde_json::json!({
                "permission": permission,
                "role_name": role,
                "user": { "login": "alice", "id": 7 },
            }))
            .unwrap();
            assert_eq!(response.can_release(), can, "{role}");
        }
    }
}
