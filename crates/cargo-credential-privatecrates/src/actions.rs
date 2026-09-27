//! Credentials inside a GitHub Actions job, from the job's OIDC token (SPEC §3.2, §3.4).

use cargo_credential::{CacheControl, CredentialResponse, Error, Operation, Secret};
use privatecrates_common::audience;
use serde::Deserialize;

pub struct Job {
    request_url: String,
    request_token: String,
}

impl Job {
    /// The job can request OIDC tokens when it has `permissions: id-token: write`.
    pub fn from_env() -> Option<Self> {
        Some(Self {
            request_url: std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").ok()?,
            request_token: std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").ok()?,
        })
    }

    pub fn get(&self, base: &str, operation: &Operation<'_>) -> Result<CredentialResponse, Error> {
        match operation {
            Operation::Publish { name, vers, cksum } => {
                let token = self.oidc_token(&audience::publish(base, name, vers, cksum))?;
                // Bound to one crate, version and checksum: never reuse it.
                Ok(CredentialResponse::Get {
                    token: Secret::from(token),
                    cache: CacheControl::Never,
                    operation_independent: false,
                })
            }
            Operation::Read => {
                let oidc = self.oidc_token(&audience::read(base))?;
                let registry = exchange(base, &oidc)?;
                Ok(CredentialResponse::Get {
                    token: Secret::from(registry.token),
                    cache: crate::expires(registry.expires_at),
                    operation_independent: false,
                })
            }
            _ => Err(format!(
                "in GitHub Actions, this credential provider can read and publish; yank from a developer machine with \
                 push access to the crate's repository (see {base}/login)"
            )
            .into()),
        }
    }

    fn oidc_token(&self, audience: &str) -> Result<String, Error> {
        #[derive(Deserialize)]
        struct Response {
            value: String,
        }
        let mut url = url::Url::parse(&self.request_url)
            .map_err(|e| format!("ACTIONS_ID_TOKEN_REQUEST_URL is invalid: {e}"))?;
        url.query_pairs_mut().append_pair("audience", audience);
        let response = crate::http()?
            .get(url)
            .bearer_auth(&self.request_token)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("could not get an OIDC token from GitHub Actions: {e}"))?;
        let response: Response = response
            .json()
            .map_err(|e| format!("GitHub Actions returned an unexpected OIDC response: {e}"))?;
        Ok(response.value)
    }
}

#[derive(Deserialize)]
struct RegistryToken {
    token: String,
    expires_at: i64,
}

fn exchange(base: &str, oidc: &str) -> Result<RegistryToken, Error> {
    let response = crate::http()?
        .post(format!("{base}/api/v1/oidc/exchange"))
        .bearer_auth(oidc)
        .send()
        .map_err(|e| format!("could not reach {base}: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|v| v["errors"][0]["detail"].as_str().map(str::to_owned))
            .unwrap_or_default();
        return Err(format!("{base} refused the OIDC token ({status}): {detail}").into());
    }
    response
        .json()
        .map_err(|e| format!("{base} returned an unexpected token response: {e}").into())
}
