//! GitHub Actions OIDC tokens (SPEC §6.4), and the read-only registry tokens we exchange them for.

use std::{
    sync::RwLock,
    time::{Duration, Instant},
};

use apollo_errors::Error;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, jwk::JwkSet};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::github::now_secs;

/// The claims we use from an Actions OIDC token. GitHub sends IDs as strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionsClaims {
    pub repository: String,
    pub repository_id: String,
    pub repository_owner_id: String,
    /// `owner/repo/.github/workflows/release.yml@refs/tags/v1`
    pub job_workflow_ref: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
    /// The login of the account that started the run: who pushed, published the release or ran the workflow.
    pub actor: String,
    pub actor_id: String,
    /// What triggered the run, e.g. `push` (see [`privatecrates_common::trigger`]).
    pub event_name: String,
    /// The git ref the run is for, e.g. `refs/tags/v1`.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// `branch` or `tag`.
    #[serde(default)]
    pub ref_type: Option<String>,
    #[serde(default)]
    pub run_attempt: Option<String>,
}

impl ActionsClaims {
    pub fn repository_id(&self) -> Option<u64> {
        self.repository_id.parse().ok()
    }

    pub fn actor_id(&self) -> Option<u64> {
        self.actor_id.parse().ok()
    }

    pub fn owner_id(&self) -> Option<u64> {
        self.repository_owner_id.parse().ok()
    }

    /// The workflow file name, e.g. `release.yml`, if the workflow is in the token's own repository. A reusable
    /// workflow from another repository does not count as the repository's own publishing workflow.
    pub fn own_workflow_file(&self) -> Option<&str> {
        let (path, _ref) = self.job_workflow_ref.split_once('@')?;
        let rest = path.strip_prefix(&self.repository)?;
        rest.strip_prefix("/.github/workflows/")
            .filter(|f| !f.is_empty() && !f.contains('/'))
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum OidcError {
    #[error("{reason}")]
    #[diagnostic(code(oidc::invalid))]
    Invalid { reason: String },
    #[error("could not fetch GitHub's OIDC signing keys: {reason}")]
    #[diagnostic(code(oidc::keys_unavailable))]
    Keys { reason: String },
}

impl OidcError {
    fn invalid(reason: impl Into<String>) -> Self {
        Self::Invalid {
            reason: reason.into(),
        }
    }

    fn keys(reason: impl Into<String>) -> Self {
        Self::Keys {
            reason: reason.into(),
        }
    }
}

pub struct Oidc {
    http: reqwest::Client,
    issuer: String,
    jwks_url: Url,
    keys: RwLock<Option<(JwkSet, Instant)>>,
}

const KEYS_TTL: Duration = Duration::from_secs(60 * 60);
/// A token signed by a key we have not seen may mean GitHub rotated keys; refetch, but not more often than this.
const MIN_REFETCH: Duration = Duration::from_secs(60);

impl Oidc {
    pub fn new(issuer: String, jwks_url: Url) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("a basic HTTP client builds"),
            issuer,
            jwks_url,
            keys: RwLock::new(None),
        }
    }

    /// Validates an Actions OIDC token's signature, issuer, expiry and exact audience.
    pub async fn validate(&self, token: &str, audience: &str) -> Result<ActionsClaims, OidcError> {
        let header =
            jsonwebtoken::decode_header(token).map_err(|e| OidcError::invalid(e.to_string()))?;
        let kid = header.kid.ok_or_else(|| OidcError::invalid("no key ID"))?;
        let jwk = match self.key(&kid, false).await? {
            Some(jwk) => jwk,
            None => self
                .key(&kid, true)
                .await?
                .ok_or_else(|| OidcError::invalid("unknown signing key"))?,
        };
        let key = DecodingKey::from_jwk(&jwk).map_err(|e| OidcError::invalid(e.to_string()))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        jsonwebtoken::decode::<ActionsClaims>(token, &key, &validation)
            .map(|data| data.claims)
            .map_err(|e| OidcError::invalid(e.to_string()))
    }

    async fn key(
        &self,
        kid: &str,
        force: bool,
    ) -> Result<Option<jsonwebtoken::jwk::Jwk>, OidcError> {
        let fresh = {
            let keys = self.keys.read().expect("keys lock");
            match &*keys {
                Some((set, fetched)) => {
                    let age = fetched.elapsed();
                    let usable = age < KEYS_TTL && !(force && age >= MIN_REFETCH);
                    usable.then(|| set.find(kid).cloned())
                }
                None => None,
            }
        };
        if let Some(found) = fresh {
            return Ok(found);
        }
        Ok(self.fetch().await?.find(kid).cloned())
    }

    /// GitHub's signing keys, for checking provenance long after its tokens expired.
    pub async fn key_set(&self) -> Result<JwkSet, OidcError> {
        let cached = self
            .keys
            .read()
            .expect("keys lock")
            .as_ref()
            .filter(|(_, fetched)| fetched.elapsed() < KEYS_TTL)
            .map(|(set, _)| set.clone());
        match cached {
            Some(set) => Ok(set),
            None => self.fetch().await,
        }
    }

    async fn fetch(&self) -> Result<JwkSet, OidcError> {
        let set: JwkSet = self
            .http
            .get(self.jwks_url.clone())
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| OidcError::keys(e.to_string()))?
            .json()
            .await
            .map_err(|e| OidcError::keys(e.to_string()))?;
        *self.keys.write().expect("keys lock") = Some((set.clone(), Instant::now()));
        Ok(set)
    }
}

/// Read-only registry tokens (`pcr_…`): signed by us, verified without storage, valid for one hour.
pub struct RegistryTokens {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

pub const REGISTRY_TOKEN_PREFIX: &str = "pcr_";
const REGISTRY_TOKEN_TTL: u64 = 60 * 60;
const REGISTRY_TOKEN_ISSUER: &str = "privatecrates";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryClaims {
    iss: String,
    /// The tenant slug.
    aud: String,
    /// The GitHub repository ID the token was issued to.
    sub: String,
    pub org_id: u64,
    pub scope: String,
    iat: u64,
    pub exp: u64,
}

impl RegistryClaims {
    pub fn repository_id(&self) -> Option<u64> {
        self.sub.parse().ok()
    }
}

impl RegistryTokens {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    pub fn issue(&self, slug: &str, org_id: u64, repository_id: u64) -> (String, u64) {
        let now = now_secs();
        let exp = now + REGISTRY_TOKEN_TTL;
        let claims = RegistryClaims {
            iss: REGISTRY_TOKEN_ISSUER.into(),
            aud: slug.into(),
            sub: repository_id.to_string(),
            org_id,
            scope: "read".into(),
            iat: now,
            exp,
        };
        let jwt = jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .expect("HS256 signing cannot fail");
        (format!("{REGISTRY_TOKEN_PREFIX}{jwt}"), exp)
    }

    /// Verifies a registry token for this tenant.
    pub fn verify(&self, token: &str, slug: &str) -> Option<RegistryClaims> {
        let jwt = token.strip_prefix(REGISTRY_TOKEN_PREFIX)?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[REGISTRY_TOKEN_ISSUER]);
        validation.set_audience(&[slug]);
        validation.leeway = 0;
        jsonwebtoken::decode::<RegistryClaims>(jwt, &self.decoding, &validation)
            .ok()
            .map(|d| d.claims)
            .filter(|c| c.scope == "read")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(repository: &str, workflow_ref: &str) -> ActionsClaims {
        ActionsClaims {
            repository: repository.into(),
            repository_id: "5".into(),
            repository_owner_id: "100".into(),
            job_workflow_ref: workflow_ref.into(),
            environment: None,
            run_id: None,
            sha: None,
            actor: "alice".into(),
            actor_id: "7".into(),
            event_name: "push".into(),
            git_ref: "refs/tags/v1".into(),
            ref_type: Some("tag".into()),
            run_attempt: Some("1".into()),
        }
    }

    #[test]
    fn workflow_file_must_be_in_the_same_repository() {
        let c = claims(
            "acme/story-engine",
            "acme/story-engine/.github/workflows/release.yml@refs/tags/v1",
        );
        assert_eq!(c.own_workflow_file(), Some("release.yml"));
        let reusable = claims(
            "acme/story-engine",
            "acme/shared/.github/workflows/release.yml@refs/heads/main",
        );
        assert_eq!(reusable.own_workflow_file(), None);
        let prefix_trick = claims(
            "acme/story",
            "acme/story-engine/.github/workflows/release.yml@refs/heads/main",
        );
        assert_eq!(prefix_trick.own_workflow_file(), None);
    }

    #[test]
    fn registry_tokens_are_bound_to_their_tenant() {
        let tokens = RegistryTokens::new(&[1; 32]);
        let (token, _) = tokens.issue("acme", 100, 5);
        assert!(token.starts_with("pcr_"));
        let claims = tokens.verify(&token, "acme").unwrap();
        assert_eq!(claims.repository_id(), Some(5));
        assert_eq!(claims.org_id, 100);
        assert!(tokens.verify(&token, "other").is_none());
        assert!(
            RegistryTokens::new(&[2; 32])
                .verify(&token, "acme")
                .is_none()
        );
        assert!(tokens.verify(&token[4..], "acme").is_none());
    }
}
