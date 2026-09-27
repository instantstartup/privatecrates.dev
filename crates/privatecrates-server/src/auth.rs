//! Who is calling, and what GitHub lets them read or push (SPEC §6).

use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::http::{HeaderMap, header};
use sha2::{Digest, Sha256};

use crate::{
    error::ApiError,
    github::{GitHub, GitHubError},
    oidc::{REGISTRY_TOKEN_PREFIX, RegistryTokens},
    tenant::{CiRead, Tenant},
};

/// A credential from the `Authorization` header, classified by its form (SPEC §6.6).
pub enum Credential {
    /// A reader App user token (`ghu_…`), from the device flow.
    AppUser(String),
    /// A read-only registry token (`pcr_…`), from the OIDC exchange.
    Registry(String),
    /// A GitHub Actions OIDC token (a JWT), accepted only for publishing.
    Oidc(String),
    /// Any other GitHub token: `gho_`, `ghp_`, `github_pat_`, `ghs_`.
    GitHub(String),
}

impl Credential {
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
        // Cargo sends the token verbatim; tolerate the usual schemes.
        let is_scheme =
            |s: &str| s.eq_ignore_ascii_case("bearer") || s.eq_ignore_ascii_case("token");
        let token = match raw.split_once(' ') {
            Some((scheme, rest)) if is_scheme(scheme) => rest.trim(),
            _ if is_scheme(raw) => "",
            _ => raw,
        };
        if token.is_empty() {
            return None;
        }
        let token = token.to_owned();
        Some(if token.starts_with("ghu_") {
            Self::AppUser(token)
        } else if token.starts_with(REGISTRY_TOKEN_PREFIX) {
            Self::Registry(token)
        } else if token.starts_with("eyJ") && token.matches('.').count() == 2 {
            Self::Oidc(token)
        } else {
            Self::GitHub(token)
        })
    }
}

/// A hash of a token, the only form in which tokens are kept, as cache keys (SPEC §10.2).
fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// What a user may access in one installation, from one lookup.
#[derive(Debug)]
pub struct UserAccess {
    pub user_id: u64,
    pub login: String,
    /// Repository ID → can push.
    repos: HashMap<u64, bool>,
}

type Cached<T> = Result<T, CachedDenial>;

/// A failed lookup, cached briefly so a bad token cannot make us call GitHub on every request.
#[derive(Debug, Clone)]
enum CachedDenial {
    Unauthorized,
    Sso(String),
}

impl From<CachedDenial> for ApiError {
    fn from(d: CachedDenial) -> Self {
        match d {
            CachedDenial::Unauthorized => GitHubError::Unauthorized.into(),
            CachedDenial::Sso(url) => GitHubError::Sso { url }.into(),
        }
    }
}

pub struct PermissionCache {
    /// (token hash, installation) → repository set, for App user tokens.
    users: moka::future::Cache<([u8; 32], u64), Cached<Arc<UserAccess>>>,
    /// (token hash, repository) → push permission if readable, for other GitHub tokens.
    repos: moka::future::Cache<([u8; 32], u64), Cached<Option<bool>>>,
    /// token hash → user, for other GitHub tokens.
    logins: moka::future::Cache<[u8; 32], Cached<String>>,
}

const DENIAL_TTL: Duration = Duration::from_secs(30);

struct Expiry;

impl<K, T> moka::Expiry<K, Cached<T>> for Expiry {
    fn expire_after_create(
        &self,
        _key: &K,
        value: &Cached<T>,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        value.is_err().then_some(DENIAL_TTL)
    }
}

impl PermissionCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            users: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(ttl)
                .expire_after(Expiry)
                .build(),
            repos: moka::future::Cache::builder()
                .max_capacity(1_000_000)
                .time_to_live(ttl)
                .expire_after(Expiry)
                .build(),
            logins: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(ttl)
                .expire_after(Expiry)
                .build(),
        }
    }
}

/// Converts a lookup error into either a cacheable denial or a transient error that must not be cached.
fn denial(e: GitHubError) -> Result<CachedDenial, ApiError> {
    match e {
        GitHubError::Unauthorized => Ok(CachedDenial::Unauthorized),
        GitHubError::Sso { url } => Ok(CachedDenial::Sso(url)),
        other => Err(other.into()),
    }
}

/// An authenticated caller of one tenant.
pub enum Caller {
    User(Arc<UserAccess>),
    GitHubToken {
        token: String,
        hash: [u8; 32],
    },
    /// A CI job, by repository ID, holding a read-only registry token.
    Ci {
        repository_id: u64,
    },
}

pub struct Resolver<'a> {
    pub gh: &'a GitHub,
    pub cache: &'a PermissionCache,
    pub registry_tokens: &'a RegistryTokens,
    pub tenant: &'a Tenant,
}

impl Resolver<'_> {
    /// Resolves a credential for reading, yanking and manual publishing. OIDC tokens are not accepted here: they
    /// are only for publishing, where the audience binds them to the bytes (see `publish`).
    pub async fn caller(&self, credential: Credential) -> Result<Caller, ApiError> {
        match credential {
            Credential::AppUser(token) => self.user(&token).await.map(Caller::User),
            Credential::Registry(token) => {
                let claims = self
                    .registry_tokens
                    .verify(&token, &self.tenant.slug)
                    .filter(|c| c.org_id == self.tenant.org_id)
                    .ok_or(ApiError::RegistryTokenInvalid)?;
                let repository_id = claims
                    .repository_id()
                    .ok_or(ApiError::RegistryTokenInvalid)?;
                Ok(Caller::Ci { repository_id })
            }
            Credential::Oidc(_) => Err(ApiError::OidcTokenForPublishing),
            Credential::GitHub(token) => {
                let hash = token_hash(&token);
                Ok(Caller::GitHubToken { token, hash })
            }
        }
    }

    async fn user(&self, token: &str) -> Result<Arc<UserAccess>, ApiError> {
        let key = (token_hash(token), self.tenant.reader_installation);
        if let Some(cached) = self.cache.users.get(&key).await {
            return cached.map_err(Into::into);
        }
        let lookup = async {
            let user = self.gh.user(token).await?;
            let repos = self
                .gh
                .user_installation_repositories(token, self.tenant.reader_installation)
                .await?;
            Ok::<_, GitHubError>(Arc::new(UserAccess {
                user_id: user.id,
                login: user.login,
                repos: repos
                    .into_iter()
                    .map(|r| (r.id, r.permissions.is_some_and(|p| p.push || p.admin)))
                    .collect(),
            }))
        };
        let value = match lookup.await {
            Ok(access) => Ok(access),
            Err(e) => Err(denial(e)?),
        };
        self.cache.users.insert(key, value.clone()).await;
        value.map_err(Into::into)
    }

    /// For a non-App GitHub token: whether it can read a repository, and if so whether it can push.
    async fn repo_access(
        &self,
        token: &str,
        hash: [u8; 32],
        repository_id: u64,
    ) -> Result<Option<bool>, ApiError> {
        let key = (hash, repository_id);
        if let Some(cached) = self.cache.repos.get(&key).await {
            return cached.map_err(Into::into);
        }
        let value = match self.gh.repository_by_id(token, repository_id).await {
            Ok(repo) => Ok(repo.map(|r| r.permissions.is_some_and(|p| p.push || p.admin))),
            Err(e) => Err(denial(e)?),
        };
        self.cache.repos.insert(key, value.clone()).await;
        value.map_err(Into::into)
    }

    pub async fn can_read(&self, caller: &Caller, repository_id: u64) -> Result<bool, ApiError> {
        match caller {
            Caller::User(access) => Ok(access.repos.contains_key(&repository_id)),
            Caller::GitHubToken { token, hash } => Ok(self
                .repo_access(token, *hash, repository_id)
                .await?
                .is_some()),
            Caller::Ci { .. } => match self.tenant.settings.ci_read {
                CiRead::Organisation => Ok(true),
            },
        }
    }

    pub async fn can_push(&self, caller: &Caller, repository_id: u64) -> Result<bool, ApiError> {
        match caller {
            Caller::User(access) => Ok(access.repos.get(&repository_id) == Some(&true)),
            Caller::GitHubToken { token, hash } => {
                Ok(self.repo_access(token, *hash, repository_id).await? == Some(true))
            }
            Caller::Ci { .. } => Ok(false),
        }
    }

    /// Whether the caller may use this registry at all: they can read at least one repository in it (SPEC §4.1).
    pub async fn can_use_registry(&self, caller: &Caller) -> Result<bool, ApiError> {
        match caller {
            Caller::User(access) => Ok(!access.repos.is_empty()),
            Caller::Ci { .. } => Ok(true),
            Caller::GitHubToken { .. } => {
                if self.can_read(caller, self.tenant.storage_repo_id).await? {
                    return Ok(true);
                }
                for (_, owner) in self.tenant.owners() {
                    if self.can_read(caller, owner.repository_id).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// The caller's GitHub login, for the audit trail.
    pub async fn login(&self, caller: &Caller) -> Result<String, ApiError> {
        match caller {
            Caller::User(access) => Ok(access.login.clone()),
            Caller::Ci { repository_id } => Ok(format!("ci:repository:{repository_id}")),
            Caller::GitHubToken { token, hash } => {
                if let Some(cached) = self.cache.logins.get(hash).await {
                    return cached.map_err(Into::into);
                }
                let value = match self.gh.user(token).await {
                    Ok(user) => Ok(user.login),
                    Err(e) => Err(denial(e)?),
                };
                self.cache.logins.insert(*hash, value.clone()).await;
                value.map_err(Into::into)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn classify(value: &str) -> Option<&'static str> {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        Credential::from_headers(&headers).map(|c| match c {
            Credential::AppUser(_) => "app-user",
            Credential::Registry(_) => "registry",
            Credential::Oidc(_) => "oidc",
            Credential::GitHub(_) => "github",
        })
    }

    #[test]
    fn classifies_tokens() {
        assert_eq!(classify("ghu_abc"), Some("app-user"));
        assert_eq!(classify("Bearer ghu_abc"), Some("app-user"));
        assert_eq!(classify("token ghp_abc"), Some("github"));
        assert_eq!(classify("gho_abc"), Some("github"));
        assert_eq!(classify("github_pat_abc"), Some("github"));
        assert_eq!(classify("pcr_eyJ.a.b"), Some("registry"));
        assert_eq!(classify("eyJhbGc.eyJzdWI.sig"), Some("oidc"));
        assert_eq!(classify("Bearer "), None);
    }
}
