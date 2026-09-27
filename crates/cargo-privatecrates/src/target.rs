//! Where PrivateCrates is: the apex (the website and account API) and each organisation's registry host.

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use reqwest::blocking::Client;

use crate::error::Error;

pub const DEFAULT_DOMAIN: &str = "privatecrates.dev";

/// A PrivateCrates deployment: `privatecrates.dev`, `dev.privatecrates.dev`, or the URL of a local server such as
/// `http://localhost:8080`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    scheme: String,
    /// The host, with its port if any.
    host: String,
}

impl FromStr for Domain {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (scheme, host) = match s.split_once("://") {
            Some((scheme, host)) => (scheme, host.trim_end_matches('/')),
            None => ("https", s),
        };
        let valid = matches!(scheme, "https" | "http")
            && !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'));
        if !valid {
            return Err(format!(
                "`{s}` is not a domain such as {DEFAULT_DOMAIN}, or a URL such as http://localhost:8080"
            ));
        }
        Ok(Self {
            scheme: scheme.to_owned(),
            host: host.to_ascii_lowercase(),
        })
    }
}

impl Default for Domain {
    fn default() -> Self {
        DEFAULT_DOMAIN.parse().expect("a valid domain")
    }
}

impl Domain {
    /// The website and account API, e.g. `https://privatecrates.dev`.
    pub fn apex(&self) -> String {
        format!("{}://{}", self.scheme, self.host)
    }

    /// An organisation's registry, e.g. `https://acme.privatecrates.dev`.
    pub fn registry(&self, slug: &str) -> String {
        format!("{}://{slug}.{}", self.scheme, self.host)
    }

    /// ` --domain …` to repeat in a suggested command, or nothing for the default.
    pub fn flag(&self) -> String {
        if *self == Self::default() {
            String::new()
        } else if self.scheme == "https" {
            format!(" --domain {}", self.host)
        } else {
            format!(" --domain {}", self.apex())
        }
    }

    /// The deployment a registry belongs to: its URL without the registry's own label.
    pub fn of_registry(base: &str) -> Option<Self> {
        let url = url::Url::parse(base).ok()?;
        let (_, apex) = url.host_str()?.split_once('.')?;
        let host = match url.port() {
            Some(port) => format!("{apex}:{port}"),
            None => apex.to_owned(),
        };
        Some(Self {
            scheme: url.scheme().to_owned(),
            host,
        })
    }
}

/// The sparse index URL Cargo uses for a registry.
pub fn index_url(base: &str) -> String {
    format!("sparse+{}/index/", base.trim_end_matches('/'))
}

/// An HTTP client for `base`. Names under `localhost` always mean this machine (RFC 6761), as they do for Cargo, so
/// a local server works even where the system resolver does not know them.
pub fn http(base: &str) -> Result<Client, Error> {
    let mut builder = Client::builder()
        .user_agent(concat!("cargo-privatecrates/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30));
    if let Some(host) = url::Url::parse(base)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        && (host == "localhost" || host.ends_with(".localhost"))
    {
        builder = builder.resolve_to_addrs(
            &host,
            &[
                SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
            ],
        );
    }
    builder.build().map_err(|source| Error::Unreachable {
        url: base.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains() {
        let prod = Domain::default();
        assert_eq!(prod.apex(), "https://privatecrates.dev");
        assert_eq!(prod.registry("acme"), "https://acme.privatecrates.dev");
        assert_eq!(prod.flag(), "");
        let dev: Domain = "dev.privatecrates.dev".parse().unwrap();
        assert_eq!(dev.registry("acme"), "https://acme.dev.privatecrates.dev");
        assert_eq!(dev.flag(), " --domain dev.privatecrates.dev");
        let local: Domain = "http://localhost:8080/".parse().unwrap();
        assert_eq!(local.apex(), "http://localhost:8080");
        assert_eq!(local.registry("acme"), "http://acme.localhost:8080");
        assert_eq!(local.flag(), " --domain http://localhost:8080");
        assert!("ftp://example.com".parse::<Domain>().is_err());
        assert!("exa mple.com".parse::<Domain>().is_err());
    }

    #[test]
    fn a_registry_belongs_to_its_deployment() {
        assert_eq!(
            Domain::of_registry("https://acme.dev.privatecrates.dev"),
            Some("dev.privatecrates.dev".parse().unwrap())
        );
        assert_eq!(
            Domain::of_registry("http://acme.localhost:8080"),
            Some("http://localhost:8080".parse().unwrap())
        );
        assert_eq!(Domain::of_registry("http://localhost:8080"), None);
    }
}
