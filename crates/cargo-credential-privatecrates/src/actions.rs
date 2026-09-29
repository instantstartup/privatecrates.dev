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
        let http = crate::http()?;
        // GitHub's token endpoint does fail now and then (504s); asking again is harmless.
        let response = retrying("GitHub Actions' OIDC endpoint", || {
            http.get(url.clone())
                .bearer_auth(&self.request_token)
                .send()
        })
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

/// Delays before each retry of a request that failed for a reason worth retrying.
const BACKOFF_SECONDS: [u64; 4] = [1, 2, 4, 8];

/// Sends a request, again after each backoff while it times out, cannot connect, or is answered with 429 or 5xx.
/// Only for requests that are safe to repeat.
fn retrying(
    what: &str,
    send: impl Fn() -> reqwest::Result<reqwest::blocking::Response>,
) -> reqwest::Result<reqwest::blocking::Response> {
    let mut delays = BACKOFF_SECONDS.iter();
    loop {
        let result = send();
        let reason = match &result {
            Ok(r) if r.status().is_server_error() || r.status().as_u16() == 429 => {
                r.status().to_string()
            }
            Ok(_) => return result,
            Err(e) if e.is_timeout() || e.is_connect() || e.is_request() => e.to_string(),
            Err(_) => return result,
        };
        let Some(delay) = delays.next() else {
            return result;
        };
        eprintln!("{what} failed ({reason}); trying again in {delay}s");
        std::thread::sleep(std::time::Duration::from_secs(*delay));
    }
}

fn exchange(base: &str, oidc: &str) -> Result<RegistryToken, Error> {
    let http = crate::http()?;
    let response = retrying(base, || {
        http.post(format!("{base}/api/v1/oidc/exchange"))
            .bearer_auth(oidc)
            .send()
    })
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
