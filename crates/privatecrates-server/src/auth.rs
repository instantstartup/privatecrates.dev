//! Who is calling, and what GitHub lets them read or push (SPEC §6).

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use axum::http::{HeaderMap, header};
use sha2::{Digest, Sha256};

use crate::{
    error::ApiError,
    github::{AppKind, GitHub, GitHubError},
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

/// (token hash, reader installation): the key of one token's repository set in one tenant.
type UserKey = ([u8; 32], u64);

pub struct PermissionCache {
    /// (token hash, reader installation) → repository set, for App user tokens.
    users: moka::future::Cache<UserKey, Cached<Arc<UserAccess>>>,
    /// GitHub user ID → the keys of their repository sets, so that a webhook can drop them (SPEC §7). An entry is
    /// refreshed whenever a key is added, so it outlives the sets it lists; keys of expired sets are pruned then.
    by_user: moka::future::Cache<u64, Arc<HashSet<UserKey>>>,
    /// (token hash, reader installation, repository) → push permission if readable, for other GitHub tokens.
    repos: moka::future::Cache<([u8; 32], u64, u64), Cached<Option<bool>>>,
    /// token hash → user, for other GitHub tokens.
    logins: moka::future::Cache<[u8; 32], Cached<String>>,
    /// (token hash, reader installation) → whether the user is an owner of the organisation (SPEC §6.2).
    org_admins: moka::future::Cache<UserKey, Cached<bool>>,
    /// Reader installation → the repositories in it, to tell when an owning repository was deleted (SPEC §6.2).
    live_repos: moka::future::Cache<u64, Arc<HashSet<u64>>>,
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
            by_user: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(ttl)
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
            org_admins: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(ttl)
                .expire_after(Expiry)
                .build(),
            live_repos: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(ttl)
                .build(),
        }
    }

    async fn insert_user(&self, key: UserKey, value: Cached<Arc<UserAccess>>) {
        let user_id = value.as_ref().ok().map(|access| access.user_id);
        self.users.insert(key, value).await;
        let Some(user_id) = user_id else {
            return;
        };
        self.by_user
            .entry(user_id)
            .and_upsert_with(|existing| async move {
                let mut keys: HashSet<UserKey> = existing
                    .map(|e| {
                        e.into_value()
                            .iter()
                            .filter(|k| self.users.contains_key(*k))
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                keys.insert(key);
                Arc::new(keys)
            })
            .await;
    }

    /// Drops what is cached about one user's access, for a webhook saying it changed (SPEC §7): their repository
    /// sets in every tenant and, in the tenant with this reader installation, the entries of other token types,
    /// whose users are not known.
    pub async fn forget_user(&self, user_id: u64, reader_installation: Option<u64>) {
        if let Some(keys) = self.by_user.remove(&user_id).await {
            for key in keys.iter() {
                self.users.invalidate(key).await;
            }
        }
        if let Some(installation) = reader_installation {
            invalidate_where(&self.repos, |k| k.1 == installation).await;
            invalidate_where(&self.org_admins, |k| k.1 == installation).await;
        }
    }

    /// Drops everything cached about permissions in the tenant with this reader installation (SPEC §7).
    pub async fn forget_installation(&self, reader_installation: u64) {
        invalidate_where(&self.users, |k| k.1 == reader_installation).await;
        invalidate_where(&self.repos, |k| k.1 == reader_installation).await;
        invalidate_where(&self.org_admins, |k| k.1 == reader_installation).await;
        self.live_repos.invalidate(&reader_installation).await;
    }
}

/// Invalidates the entries whose keys match: a scan, but the webhooks that need one are rare.
async fn invalidate_where<K, V>(cache: &moka::future::Cache<K, V>, matches: impl Fn(&K) -> bool)
where
    K: std::hash::Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    let keys: Vec<Arc<K>> = cache
        .iter()
        .filter(|(k, _)| matches(k))
        .map(|(k, _)| k)
        .collect();
    for key in keys {
        cache.invalidate(key.as_ref()).await;
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
    User {
        access: Arc<UserAccess>,
        token: String,
    },
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
            Credential::AppUser(token) => {
                let access = self.user(&token).await?;
                Ok(Caller::User { access, token })
            }
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
        self.cache.insert_user(key, value.clone()).await;
        value.map_err(Into::into)
    }

    /// For a non-App GitHub token: whether it can read a repository, and if so whether it can push.
    async fn repo_access(
        &self,
        token: &str,
        hash: [u8; 32],
        repository_id: u64,
    ) -> Result<Option<bool>, ApiError> {
        let key = (hash, self.tenant.reader_installation, repository_id);
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
        let readable = match caller {
            Caller::User { access, .. } => access.repos.contains_key(&repository_id),
            Caller::GitHubToken { token, hash } => self
                .repo_access(token, *hash, repository_id)
                .await?
                .is_some(),
            Caller::Ci { .. } => {
                return match self.tenant.settings.ci_read {
                    CiRead::Organisation => Ok(true),
                };
            }
        };
        if readable {
            return Ok(true);
        }
        // SPEC §6.2: while a crate's owning repository is deleted, organisation owners can still read the crate.
        if self.repository_exists(repository_id).await? {
            return Ok(false);
        }
        self.is_org_admin(caller).await
    }

    /// Whether a repository still exists in the tenant, as the reader App's installation sees it. One lookup per
    /// tenant per cache period, with our installation token.
    async fn repository_exists(&self, repository_id: u64) -> Result<bool, ApiError> {
        let installation = self.tenant.reader_installation;
        if let Some(live) = self.cache.live_repos.get(&installation).await {
            return Ok(live.contains(&repository_id));
        }
        let token = self
            .gh
            .installation_token(AppKind::Reader, installation)
            .await?;
        let live: Arc<HashSet<u64>> = Arc::new(
            self.gh
                .installation_repositories(&token)
                .await?
                .into_iter()
                .map(|r| r.id)
                .collect(),
        );
        self.cache
            .live_repos
            .insert(installation, live.clone())
            .await;
        Ok(live.contains(&repository_id))
    }

    /// Whether the caller is an owner of the tenant's organisation: an active member with the `admin` role.
    async fn is_org_admin(&self, caller: &Caller) -> Result<bool, ApiError> {
        let (token, hash) = match caller {
            Caller::User { token, .. } => (token.as_str(), token_hash(token)),
            Caller::GitHubToken { token, hash } => (token.as_str(), *hash),
            Caller::Ci { .. } => return Ok(false),
        };
        let key = (hash, self.tenant.reader_installation);
        if let Some(cached) = self.cache.org_admins.get(&key).await {
            return cached.map_err(Into::into);
        }
        let value = match self.gh.org_membership(token, &self.tenant.org_login).await {
            Ok(membership) => Ok(membership.is_some_and(|m| m.is_admin())),
            Err(e) => Err(denial(e)?),
        };
        self.cache.org_admins.insert(key, value.clone()).await;
        value.map_err(Into::into)
    }

    pub async fn can_push(&self, caller: &Caller, repository_id: u64) -> Result<bool, ApiError> {
        match caller {
            Caller::User { access, .. } => Ok(access.repos.get(&repository_id) == Some(&true)),
            Caller::GitHubToken { token, hash } => {
                Ok(self.repo_access(token, *hash, repository_id).await? == Some(true))
            }
            Caller::Ci { .. } => Ok(false),
        }
    }

    /// Whether the caller may use this registry at all: they can read at least one repository in it (SPEC §4.1).
    pub async fn can_use_registry(&self, caller: &Caller) -> Result<bool, ApiError> {
        match caller {
            Caller::User { access, .. } => Ok(!access.repos.is_empty()),
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
            Caller::User { access, .. } => Ok(access.login.clone()),
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
