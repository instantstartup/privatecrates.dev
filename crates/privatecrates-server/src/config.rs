//! Configuration, from the environment only (SPEC §11).

use std::{env, net::SocketAddr, time::Duration};

use apollo_errors::Error;
use miette::Diagnostic;
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// Tenants are served at `{slug}.{base_domain}`. May include a port, for local testing.
    pub base_domain: String,
    /// `https` in production; `http` only for local testing.
    pub public_scheme: String,
    pub github_api: Url,
    /// Where users sign in: `https://github.com`.
    pub github_web: Url,
    /// The reader App's OAuth client ID, which the credential provider uses for the device flow.
    pub reader_client_id: String,
    pub reader_app: AppConfig,
    pub storage_app: AppConfig,
    /// Secret for signing read-only registry tokens (`pcr_…`).
    pub registry_token_secret: Vec<u8>,
    pub webhook_secret: Option<Vec<u8>>,
    pub oidc_issuer: String,
    pub oidc_jwks_url: Url,
    pub crates_io_api: Url,
    pub max_crate_bytes: usize,
    pub publish_rate_per_minute: u32,
    pub permission_ttl: Duration,
    pub tenant_refresh: Duration,
    pub storage_refresh: Duration,
}

#[derive(Clone)]
pub struct AppConfig {
    pub id: u64,
    /// PEM-encoded RSA private key. Milestone 4 moves this into a KMS (SPEC §10.2).
    pub private_key_pem: Vec<u8>,
}

#[derive(Debug, Error, Diagnostic)]
pub enum ConfigError {
    #[error("{name} is not set")]
    #[diagnostic(code(config::missing))]
    Missing { name: &'static str },
    #[error("{name} is invalid: {reason}")]
    #[diagnostic(code(config::invalid))]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let port: u16 = parse_or("PORT", 8080)?;
        let registry_token_secret = required("REGISTRY_TOKEN_SECRET")?.into_bytes();
        if registry_token_secret.len() < 32 {
            return Err(ConfigError::Invalid {
                name: "REGISTRY_TOKEN_SECRET",
                reason: "must be at least 32 bytes".into(),
            });
        }
        Ok(Self {
            bind: SocketAddr::from(([0, 0, 0, 0], port)),
            base_domain: required("BASE_DOMAIN")?,
            public_scheme: optional("PUBLIC_SCHEME").unwrap_or_else(|| "https".into()),
            github_api: url_or("GITHUB_API_URL", "https://api.github.com")?,
            github_web: url_or("GITHUB_WEB_URL", "https://github.com")?,
            reader_client_id: required("READER_APP_CLIENT_ID")?,
            reader_app: AppConfig {
                id: parse_required("READER_APP_ID")?,
                private_key_pem: required("READER_APP_PRIVATE_KEY")?.into_bytes(),
            },
            storage_app: AppConfig {
                id: parse_required("STORAGE_APP_ID")?,
                private_key_pem: required("STORAGE_APP_PRIVATE_KEY")?.into_bytes(),
            },
            registry_token_secret,
            webhook_secret: optional("WEBHOOK_SECRET").map(String::into_bytes),
            oidc_issuer: optional("OIDC_ISSUER")
                .unwrap_or_else(|| "https://token.actions.githubusercontent.com".into()),
            oidc_jwks_url: url_or(
                "OIDC_JWKS_URL",
                "https://token.actions.githubusercontent.com/.well-known/jwks",
            )?,
            crates_io_api: url_or("CRATES_IO_API_URL", "https://crates.io")?,
            max_crate_bytes: parse_or("MAX_CRATE_BYTES", 20 * 1024 * 1024)?,
            publish_rate_per_minute: parse_or("PUBLISH_RATE_PER_MINUTE", 30)?,
            permission_ttl: Duration::from_secs(parse_or("PERMISSION_TTL_SECS", 300)?),
            tenant_refresh: Duration::from_secs(parse_or("TENANT_REFRESH_SECS", 600)?),
            storage_refresh: Duration::from_secs(parse_or("STORAGE_REFRESH_SECS", 60)?),
        })
    }

    /// The public base URL of a tenant, e.g. `https://acme.privatecrates.dev`.
    pub fn tenant_base_url(&self, slug: &str) -> String {
        format!("{}://{slug}.{}", self.public_scheme, self.base_domain)
    }

    /// The tenant slug for a `Host` header, if it is a subdomain of the base domain.
    pub fn slug_for_host<'a>(&self, host: &'a str) -> Option<&'a str> {
        let slug = host.strip_suffix(&self.base_domain)?.strip_suffix('.')?;
        (!slug.is_empty() && !slug.contains('.')).then_some(slug)
    }
}

fn optional(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.is_empty())
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    optional(name).ok_or(ConfigError::Missing { name })
}

fn parse_required<T: std::str::FromStr>(name: &'static str) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    parse(name, &required(name)?)
}

fn parse_or<T: std::str::FromStr>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    optional(name).map_or(Ok(default), |v| parse(name, &v))
}

fn parse<T: std::str::FromStr>(name: &'static str, value: &str) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|e: T::Err| ConfigError::Invalid {
        name,
        reason: e.to_string(),
    })
}

fn url_or(name: &'static str, default: &str) -> Result<Url, ConfigError> {
    parse(name, &optional(name).unwrap_or_else(|| default.into()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn config(base_domain: &str) -> Config {
        Config {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            base_domain: base_domain.into(),
            public_scheme: "https".into(),
            github_api: "https://api.github.com".parse().unwrap(),
            github_web: "https://github.com".parse().unwrap(),
            reader_client_id: "Iv1.test".into(),
            reader_app: AppConfig {
                id: 1,
                private_key_pem: vec![],
            },
            storage_app: AppConfig {
                id: 2,
                private_key_pem: vec![],
            },
            registry_token_secret: vec![7; 32],
            webhook_secret: None,
            oidc_issuer: "https://token.actions.githubusercontent.com".into(),
            oidc_jwks_url: "https://token.actions.githubusercontent.com/.well-known/jwks"
                .parse()
                .unwrap(),
            crates_io_api: "https://crates.io".parse().unwrap(),
            max_crate_bytes: 1024,
            publish_rate_per_minute: 30,
            permission_ttl: Duration::from_secs(300),
            tenant_refresh: Duration::from_secs(600),
            storage_refresh: Duration::from_secs(60),
        }
    }

    #[test]
    fn slug_from_host() {
        let c = config("privatecrates.dev");
        assert_eq!(c.slug_for_host("acme.privatecrates.dev"), Some("acme"));
        assert_eq!(c.slug_for_host("privatecrates.dev"), None);
        assert_eq!(c.slug_for_host("a.b.privatecrates.dev"), None);
        assert_eq!(c.slug_for_host("acmeprivatecrates.dev"), None);
        assert_eq!(c.slug_for_host("acme.example.com"), None);
        let local = config("localhost:8080");
        assert_eq!(local.slug_for_host("acme.localhost:8080"), Some("acme"));
    }
}
