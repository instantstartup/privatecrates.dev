//! The few GitHub REST endpoints PrivateCrates uses.
//!
//! Two kinds of calls: with an installation token of one of our Apps (storage reads and writes, tenant discovery),
//! and with a caller's own token (permission lookups on their behalf, SPEC §6.3).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apollo_errors::Error;
use base64::Engine;
use bytes::Bytes;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use miette::Diagnostic;
use reqwest::{Method, RequestBuilder, Response, StatusCode, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::Url;

use crate::config::{AppConfig, Config};

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
    reader: App,
    storage: App,
    installation_tokens: moka::future::Cache<(AppKind, u64), String>,
}

// Response types: only the fields we use.

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub login: String,
    pub id: u64,
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
            reader: App::new(&config.reader_app)?,
            storage: App::new(&config.storage_app)?,
            installation_tokens: moka::future::Cache::builder()
                .max_capacity(100_000)
                // Installation tokens last an hour; refresh well before that.
                .time_to_live(Duration::from_secs(45 * 60))
                .build(),
        })
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
                    .send()
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
            .send()
            .await?,
        )
        .await?;
        self.installation_tokens
            .insert((kind, installation_id), response.token.clone())
            .await;
        Ok(response.token)
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
                    .send()
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

    // --- Calls with the caller's token ---

    pub async fn user(&self, token: &str) -> Result<User, GitHubError> {
        json(self.request(Method::GET, "/user", token).send().await?).await
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
                .send()
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

    /// A repository by ID, as seen by `token`. `None` if the token cannot see it.
    pub async fn repository_by_id(
        &self,
        token: &str,
        id: u64,
    ) -> Result<Option<Repo>, GitHubError> {
        match json(
            self.request(Method::GET, &format!("/repositories/{id}"), token)
                .send()
                .await?,
        )
        .await
        {
            Ok(repo) => Ok(Some(repo)),
            Err(GitHubError::NotFound) => Ok(None),
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
        let response = request.send().await?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(Conditional::NotModified);
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
            .send()
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

    /// Creates or updates one file in one commit. A mismatch between `write.sha` and the file's current blob sha
    /// is a [`GitHubError::Conflict`]. Returns the new blob sha.
    pub async fn put_file(
        &self,
        token: &str,
        repo: &str,
        branch: &str,
        write: FileWrite<'_>,
    ) -> Result<String, GitHubError> {
        #[derive(Serialize)]
        struct Body<'a> {
            message: &'a str,
            content: String,
            branch: &'a str,
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
            .send()
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
            .send()
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
                    .send()
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
                .send()
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
                .send()
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
            .send()
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
            .send()
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
        .send()
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
}
