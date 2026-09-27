//! Cargo credential provider for PrivateCrates (SPEC §3.1).
//!
//! - In GitHub Actions, it uses the job's OIDC token: exchanged for a read-only registry token to read, or bound
//!   to the exact crate, version and checksum Cargo is about to upload to publish.
//! - On a developer's machine, it signs in with GitHub's device flow for the registry's reader App, and keeps the
//!   resulting narrow token in the operating system's keyring.

mod actions;

use cargo_credential::{
    Action, CacheControl, Credential, CredentialResponse, Error, Operation, RegistryInfo, Secret,
};
use privatecrates_auth::{device, store::Store};
use privatecrates_common::audience;

struct Provider;

impl Credential for Provider {
    fn perform(
        &self,
        registry: &RegistryInfo<'_>,
        action: &Action<'_>,
        _args: &[&str],
    ) -> Result<CredentialResponse, Error> {
        let base =
            audience::base_url_from_index(registry.index_url).ok_or(Error::UrlNotSupported)?;
        match action {
            Action::Get(operation) => get(&base, operation),
            Action::Login(_) => {
                let store = Store::open().map_err(auth)?;
                device::sign_in(&http()?, &base, &store).map_err(auth)?;
                Ok(CredentialResponse::Login)
            }
            Action::Logout => {
                Store::open()
                    .and_then(|store| store.delete(&base))
                    .map_err(auth)?;
                Ok(CredentialResponse::Logout)
            }
            _ => Err(Error::OperationNotSupported),
        }
    }
}

fn get(base: &str, operation: &Operation<'_>) -> Result<CredentialResponse, Error> {
    if let Some(job) = actions::Job::from_env() {
        return job.get(base, operation);
    }
    if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        return Err(format!(
            "this GitHub Actions job cannot request an OIDC token. Add `permissions: id-token: write` to the job \
             (see {base}/login#ci)"
        )
        .into());
    }
    if std::env::var_os("CI").is_some() {
        return Err(format!(
            "PrivateCrates supports CI on GitHub Actions only; other CI cannot sign in interactively. Build in GitHub \
             Actions, or vendor dependencies there (see {base}/login#ci)"
        )
        .into());
    }
    let store = Store::open().map_err(auth)?;
    let token = device::token(&http()?, base, &store).map_err(auth)?;
    Ok(CredentialResponse::Get {
        token: Secret::from(token.access_token),
        cache: expires(token.expires_at),
        operation_independent: true,
    })
}

/// Cache a token for this Cargo invocation until a minute before it expires.
fn expires(at: i64) -> CacheControl {
    match time::OffsetDateTime::from_unix_timestamp(at - 60) {
        Ok(expiration) => CacheControl::Expires { expiration },
        Err(_) => CacheControl::Never,
    }
}

/// Signing in and the token store report what to do in their message, which Cargo shows.
fn auth(e: privatecrates_auth::Error) -> Error {
    e.to_string().into()
}

fn http() -> Result<reqwest::blocking::Client, Error> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "cargo-credential-privatecrates/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Other(Box::new(e)))
}

fn main() {
    cargo_credential::main(Provider);
}
