//! What the verifier asks GitHub (and crates.io), behind a trait so the checks can be tested without a network.

use std::time::Duration;

use jsonwebtoken::jwk::JwkSet;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("{0}: {1}")]
    Http(String, reqwest::Error),
}

pub struct CommitInfo {
    /// The GitHub account the commit's author is linked to.
    pub login: Option<String>,
    /// Whether GitHub verified the commit's signature.
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub immutable: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

pub trait Remote {
    fn commit(&self, sha: &str) -> Result<CommitInfo, RemoteError>;
    fn release(&self, tag: &str) -> Result<Option<Release>, RemoteError>;
    fn asset(&self, id: u64) -> Result<Vec<u8>, RemoteError>;
    fn jwks(&self) -> Result<JwkSet, RemoteError>;
    fn crates_io_exists(&self, name: &str) -> Result<bool, RemoteError>;
}

pub struct GitHub {
    pub http: reqwest::blocking::Client,
    pub api: String,
    /// `owner/name` of the storage repository.
    pub repo: String,
    pub token: String,
    pub jwks_url: String,
    pub crates_io: String,
}

impl GitHub {
    pub fn client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .user_agent(concat!("privatecrates-verify/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("a basic HTTP client builds")
    }

    fn get(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        self.http
            .get(format!("{}/repos/{}{path}", self.api, self.repo))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }
}

fn http(context: &str) -> impl FnOnce(reqwest::Error) -> RemoteError + '_ {
    move |e| RemoteError::Http(context.to_owned(), e)
}

impl Remote for GitHub {
    fn commit(&self, sha: &str) -> Result<CommitInfo, RemoteError> {
        #[derive(Deserialize)]
        struct Author {
            login: String,
        }
        #[derive(Deserialize)]
        struct Verification {
            verified: bool,
        }
        #[derive(Deserialize)]
        struct Inner {
            verification: Verification,
        }
        #[derive(Deserialize)]
        struct Commit {
            author: Option<Author>,
            commit: Inner,
        }
        let context = format!("commit {sha}");
        let commit: Commit = self
            .get(&format!("/commits/{sha}"))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(http(&context))?;
        Ok(CommitInfo {
            login: commit.author.map(|a| a.login),
            verified: commit.commit.verification.verified,
        })
    }

    fn release(&self, tag: &str) -> Result<Option<Release>, RemoteError> {
        let context = format!("release {tag}");
        let response = self
            .get(&format!("/releases/tags/{tag}"))
            .send()
            .map_err(http(&context))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()
            .and_then(|r| r.json())
            .map(Some)
            .map_err(http(&context))
    }

    fn asset(&self, id: u64) -> Result<Vec<u8>, RemoteError> {
        let context = format!("asset {id}");
        // The redirect goes to a signed URL on another host; reqwest drops the token when following it.
        self.get(&format!("/releases/assets/{id}"))
            .header("Accept", "application/octet-stream")
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.bytes())
            .map(|b| b.to_vec())
            .map_err(http(&context))
    }

    fn jwks(&self) -> Result<JwkSet, RemoteError> {
        self.http
            .get(&self.jwks_url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(http("GitHub's OIDC keys"))
    }

    fn crates_io_exists(&self, name: &str) -> Result<bool, RemoteError> {
        let context = format!("crates.io {name}");
        let response = self
            .http
            .get(format!("{}/api/v1/crates/{name}", self.crates_io))
            .send()
            .map_err(http(&context))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        response.error_for_status().map_err(http(&context))?;
        Ok(true)
    }
}
