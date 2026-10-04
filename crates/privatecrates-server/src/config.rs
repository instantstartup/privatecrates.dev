//! Configuration, from the environment only (SPEC §11).

use std::{collections::BTreeSet, env, net::SocketAddr, path::PathBuf, time::Duration};

use apollo_errors::Error;
use miette::Diagnostic;
use url::Url;

use crate::tenant::is_reserved;

/// The production apex; any other `BASE_DOMAIN` is kept out of search engines.
const PRODUCTION_DOMAIN: &str = "privatecrates.dev";
/// Our deployments, which must record terms acceptances in Postgres.
const DEPLOYED_DOMAINS: &[&str] = &[PRODUCTION_DOMAIN, "dev.privatecrates.dev"];

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
    /// The reader App's client secret, for the website's sign-in (GitHub's web flow).
    pub reader_client_secret: String,
    /// The Apps' names in their GitHub URLs (`https://github.com/apps/{slug}`), for installation links.
    pub reader_app_slug: String,
    pub storage_app_slug: String,
    pub reader_app: AppConfig,
    pub storage_app: AppConfig,
    /// Secret for signing read-only registry tokens (`pcr_…`).
    pub registry_token_secret: Vec<u8>,
    /// Secrets for verifying GitHub webhooks, one per App: each App has its own. Empty if webhooks are off.
    pub webhook_secrets: Vec<Vec<u8>>,
    /// Secret for the website's session cookie and sign-in state; at least 32 bytes.
    pub session_secret: Vec<u8>,
    /// The website's static build, served on the apex host. Without it, the apex host serves only the account API.
    pub website_dir: Option<PathBuf>,
    /// The preview (docs/preview.md): PrivateCrates is free, and billing is off whatever Stripe configuration is
    /// present.
    pub preview: bool,
    /// The private preview: only these organisations (lowercase GitHub logins) may have a registry. `None` lets
    /// every organisation in (docs/preview.md §5).
    pub invited_orgs: Option<BTreeSet<String>>,
    /// Postgres, for the terms acceptances (the only records of our own). Without it, in local development only,
    /// they are kept in memory.
    pub database_url: Option<String>,
    /// Stripe billing. Without it, every tenant is treated as active.
    pub stripe: Option<StripeConfig>,
    /// Organisations with at most this many members use PrivateCrates for free.
    pub free_member_limit: u64,
    /// The length of the no-card trial that larger organisations get once.
    pub trial_days: u32,
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

#[derive(Clone)]
pub struct StripeConfig {
    pub api: Url,
    pub secret_key: String,
    pub webhook_secret: Vec<u8>,
    /// The monthly price every subscription is for.
    pub price_id: String,
}

/// What a request's `Host` names.
#[derive(Debug, PartialEq, Eq)]
pub enum HostKind<'a> {
    /// The website and account API.
    Apex,
    /// `www.` + the apex, redirected to the apex.
    Www,
    Tenant(&'a str),
    Unknown,
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
        let base_domain = required("BASE_DOMAIN")?;
        let public_scheme = optional("PUBLIC_SCHEME").unwrap_or_else(|| "https".into());
        let preview = parse_or("PREVIEW", true)?;
        let database_url = optional("DATABASE_URL");
        if database_url.is_none() && database_required(&base_domain, &public_scheme, preview) {
            // A registry is never created without its terms acceptance recorded.
            return Err(ConfigError::Missing {
                name: "DATABASE_URL",
            });
        }
        let stripe = match optional("STRIPE_SECRET_KEY") {
            Some(secret_key) => Some(StripeConfig {
                api: url_or("STRIPE_API_URL", "https://api.stripe.com")?,
                secret_key,
                webhook_secret: required("STRIPE_WEBHOOK_SECRET")?.into_bytes(),
                price_id: required("STRIPE_PRICE_ID")?,
            }),
            None => None,
        };
        Ok(Self {
            bind: SocketAddr::from(([0, 0, 0, 0], port)),
            base_domain,
            public_scheme,
            github_api: url_or("GITHUB_API_URL", "https://api.github.com")?,
            github_web: url_or("GITHUB_WEB_URL", "https://github.com")?,
            reader_client_id: required("READER_APP_CLIENT_ID")?,
            reader_client_secret: required("READER_APP_CLIENT_SECRET")?,
            reader_app_slug: required("READER_APP_SLUG")?,
            storage_app_slug: required("STORAGE_APP_SLUG")?,
            reader_app: AppConfig {
                id: parse_required("READER_APP_ID")?,
                private_key_pem: required("READER_APP_PRIVATE_KEY")?.into_bytes(),
            },
            storage_app: AppConfig {
                id: parse_required("STORAGE_APP_ID")?,
                private_key_pem: required("STORAGE_APP_PRIVATE_KEY")?.into_bytes(),
            },
            registry_token_secret: secret("REGISTRY_TOKEN_SECRET")?,
            // Comma-separated, e.g. `WEBHOOK_SECRET=reader-secret,storage-secret`: a delivery signed with any of
            // them is accepted. The secrets themselves cannot contain commas.
            webhook_secrets: optional("WEBHOOK_SECRET")
                .map(|v| secrets(&v))
                .unwrap_or_default(),
            session_secret: secret("SESSION_SECRET")?,
            website_dir: optional("WEBSITE_DIR").map(PathBuf::from),
            preview,
            // Invitation only during the preview: without INVITED_ORGS, no organisation is invited.
            invited_orgs: preview
                .then(|| invited(optional("INVITED_ORGS").as_deref().unwrap_or(""))),
            database_url,
            stripe,
            free_member_limit: parse_or("FREE_MEMBER_LIMIT", 5)?,
            trial_days: parse_or("TRIAL_DAYS", 90)?,
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

    /// The website's URL, e.g. `https://privatecrates.dev`: also the only `Origin` its API accepts.
    pub fn apex_url(&self) -> String {
        format!("{}://{}", self.public_scheme, self.base_domain)
    }

    /// Where an organisation admin manages the organisation's registry and subscription.
    pub fn account_url(&self) -> String {
        format!("{}/account", self.apex_url())
    }

    /// Where the terms an organisation admin accepts are published.
    pub fn terms_url(&self) -> String {
        format!("{}/legal/terms", self.apex_url())
    }

    /// Whether this is the production deployment, the only one search engines may index.
    pub fn is_invited(&self, org_login: &str) -> bool {
        self.invited_orgs
            .as_ref()
            .is_none_or(|orgs| orgs.contains(&org_login.to_ascii_lowercase()))
    }

    pub fn is_production(&self) -> bool {
        self.base_domain == PRODUCTION_DOMAIN
    }

    /// The tenant slug for a `Host` header, if it is a subdomain of the base domain. Reserved names are never
    /// tenants.
    pub fn slug_for_host<'a>(&self, host: &'a str) -> Option<&'a str> {
        let slug = host.strip_suffix(&self.base_domain)?.strip_suffix('.')?;
        // Only names a registry could have: anything else never becomes a stand-in registry, and so never reaches
        // the HTML of its /login page.
        (crate::tenant::slug_is_valid(slug) && !is_reserved(slug)).then_some(slug)
    }

    /// Classifies a lowercase `Host` header.
    pub fn host_kind<'a>(&self, host: &'a str) -> HostKind<'a> {
        if host == self.base_domain {
            HostKind::Apex
        } else if host.strip_prefix("www.") == Some(&self.base_domain) {
            HostKind::Www
        } else {
            self.slug_for_host(host)
                .map_or(HostKind::Unknown, HostKind::Tenant)
        }
    }
}

/// Whether this looks like a deployment rather than local development: one of our domains, or a public preview.
fn database_required(base_domain: &str, public_scheme: &str, preview: bool) -> bool {
    DEPLOYED_DOMAINS.contains(&base_domain) || (preview && public_scheme == "https")
}

fn secret(name: &'static str) -> Result<Vec<u8>, ConfigError> {
    let value = required(name)?.into_bytes();
    if value.len() < 32 {
        return Err(ConfigError::Invalid {
            name,
            reason: "must be a string of at least 32 bytes, such as the output of `openssl rand -base64 48`"
                .into(),
        });
    }
    Ok(value)
}

/// GitHub organisation logins, separated by commas or whitespace, case-insensitively.
fn invited(list: &str) -> BTreeSet<String> {
    list.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|login| !login.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
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

/// A comma-separated list of secrets, ignoring surrounding whitespace and empty entries.
fn secrets(value: &str) -> Vec<Vec<u8>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.as_bytes().to_vec())
        .collect()
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
            reader_client_secret: "secret".into(),
            reader_app_slug: "privatecrates-reader".into(),
            storage_app_slug: "privatecrates-storage".into(),
            reader_app: AppConfig {
                id: 1,
                private_key_pem: vec![],
            },
            storage_app: AppConfig {
                id: 2,
                private_key_pem: vec![],
            },
            registry_token_secret: vec![7; 32],
            webhook_secrets: Vec::new(),
            session_secret: vec![8; 32],
            website_dir: None,
            preview: false,
            invited_orgs: None,
            database_url: None,
            stripe: None,
            free_member_limit: 5,
            trial_days: 90,
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
        assert_eq!(c.slug_for_host("www.privatecrates.dev"), None);
        let local = config("localhost:8080");
        assert_eq!(local.slug_for_host("acme.localhost:8080"), Some("acme"));
    }

    #[test]
    fn webhook_secrets() {
        assert_eq!(secrets("one"), vec![b"one".to_vec()]);
        assert_eq!(
            secrets(" reader , storage,"),
            vec![b"reader".to_vec(), b"storage".to_vec()]
        );
        assert!(secrets(",").is_empty());
    }

    #[test]
    fn only_the_production_domain_is_production() {
        assert!(config("privatecrates.dev").is_production());
        assert!(!config("dev.privatecrates.dev").is_production());
        assert!(!config("localhost:8080").is_production());
    }

    #[test]
    fn deployments_need_a_database() {
        assert!(database_required("privatecrates.dev", "https", false));
        assert!(database_required("dev.privatecrates.dev", "http", false));
        assert!(database_required("staging.example.com", "https", true));
        assert!(!database_required("localhost:8080", "http", true));
        assert!(!database_required("staging.example.com", "https", false));
    }

    #[test]
    fn host_kinds() {
        let c = config("privatecrates.dev");
        assert_eq!(c.host_kind("privatecrates.dev"), HostKind::Apex);
        assert_eq!(c.host_kind("www.privatecrates.dev"), HostKind::Www);
        assert_eq!(
            c.host_kind("acme.privatecrates.dev"),
            HostKind::Tenant("acme")
        );
        assert_eq!(c.host_kind("api.privatecrates.dev"), HostKind::Unknown);
        assert_eq!(c.host_kind("example.com"), HostKind::Unknown);
        assert_eq!(c.host_kind("wwwprivatecrates.dev"), HostKind::Unknown);
    }
}
