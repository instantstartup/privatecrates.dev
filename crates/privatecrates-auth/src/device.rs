//! Signing a developer in with GitHub's device flow for the registry's reader App (SPEC §3.1).
//!
//! The resulting user access token (`ghu_…`) can do only what the reader App may do: read repository metadata. It
//! lasts eight hours and is refreshed directly with GitHub; the refresh token never goes to the registry.

use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::{
    Error, now,
    store::{Store, Stored},
};

#[derive(Deserialize)]
struct AuthInfo {
    github_client_id: String,
    github_url: String,
}

/// A usable access token for the registry: from the store, refreshed, or from a new sign-in.
pub fn token(http: &Client, base: &str, store: &Store) -> Result<Stored, Error> {
    match current(http, base, store)? {
        Some(stored) => Ok(stored),
        None => sign_in(http, base, store),
    }
}

/// A usable access token from the store, refreshed with GitHub if it has expired, without signing in. `None` when
/// the user must sign in again.
pub fn current(http: &Client, base: &str, store: &Store) -> Result<Option<Stored>, Error> {
    let now = now();
    let Some(stored) = store.load(base)? else {
        return Ok(None);
    };
    if stored.expires_at > now + 60 {
        return Ok(Some(stored));
    }
    if let Some(refresh) = stored.refresh_token.as_deref()
        && stored.refresh_expires_at.is_none_or(|at| at > now)
    {
        let info = auth_info(http, base)?;
        if let Ok(Some(fresh)) = grant(
            http,
            &info,
            &[("grant_type", "refresh_token"), ("refresh_token", refresh)],
        ) {
            store.save(base, &fresh)?;
            return Ok(Some(fresh));
        }
    }
    Ok(None)
}

/// Runs the device flow and stores the result.
pub fn sign_in(http: &Client, base: &str, store: &Store) -> Result<Stored, Error> {
    #[derive(Deserialize)]
    struct DeviceCode {
        device_code: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
    }
    let info = auth_info(http, base)?;
    let code: DeviceCode = http
        .post(format!("{}/login/device/code", info.github_url))
        .header("Accept", "application/json")
        .form(&[("client_id", info.github_client_id.as_str())])
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("could not start signing in with GitHub: {e}"))?;
    eprintln!(
        "To use {base}, sign in with GitHub: open {} and enter the code {}",
        code.verification_uri, code.user_code
    );
    let mut interval = code.interval;
    let deadline = now() + code.expires_in as i64;
    while now() < deadline {
        std::thread::sleep(Duration::from_secs(interval));
        match grant(
            http,
            &info,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", code.device_code.as_str()),
            ],
        ) {
            Ok(Some(stored)) => {
                store.save(base, &stored)?;
                eprintln!("Signed in.");
                return Ok(stored);
            }
            Ok(None) => {}
            Err(Pending::SlowDown) => interval += 5,
            Err(Pending::Failed(e)) => return Err(e),
        }
    }
    Err("the GitHub sign-in code expired; run the command again".into())
}

enum Pending {
    SlowDown,
    Failed(Error),
}

impl From<Error> for Pending {
    fn from(e: Error) -> Self {
        Self::Failed(e)
    }
}

/// Asks GitHub for a token. `Ok(None)` while the user has not yet approved.
fn grant(
    http: &Client,
    info: &AuthInfo,
    params: &[(&str, &str)],
) -> Result<Option<Stored>, Pending> {
    #[derive(Deserialize)]
    struct Response {
        access_token: Option<String>,
        expires_in: Option<i64>,
        refresh_token: Option<String>,
        refresh_token_expires_in: Option<i64>,
        error: Option<String>,
    }
    let mut form = vec![("client_id", info.github_client_id.as_str())];
    form.extend_from_slice(params);
    let response: Response = http
        .post(format!("{}/login/oauth/access_token", info.github_url))
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| Error::from(format!("could not sign in with GitHub: {e}")))?;
    match (response.access_token, response.error.as_deref()) {
        (Some(access_token), _) => {
            let now = now();
            Ok(Some(Stored {
                access_token,
                // GitHub App user tokens last eight hours unless expiry is disabled.
                expires_at: now + response.expires_in.unwrap_or(8 * 60 * 60),
                refresh_token: response.refresh_token,
                refresh_expires_at: response.refresh_token_expires_in.map(|s| now + s),
            }))
        }
        (None, Some("authorization_pending")) => Ok(None),
        (None, Some("slow_down")) => Err(Pending::SlowDown),
        (None, Some("access_denied")) => {
            Err(Pending::Failed("the GitHub sign-in was declined".into()))
        }
        (None, Some("expired_token")) => Err(Pending::Failed(
            "the GitHub sign-in code expired; run the command again".into(),
        )),
        (None, error) => Err(Pending::Failed(
            format!(
                "GitHub refused the sign-in: {}",
                error.unwrap_or("no reason given")
            )
            .into(),
        )),
    }
}

fn auth_info(http: &Client, base: &str) -> Result<AuthInfo, Error> {
    http.get(format!("{base}/api/v1/auth"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("could not reach {base}: {e}").into())
}
