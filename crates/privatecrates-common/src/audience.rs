//! OIDC audiences that bind a GitHub Actions token to one use.
//!
//! A publish audience names the crate, version and sha256 of the exact `.crate` being published, so the token
//! cannot be used for any other bytes (SPEC §3.4).

/// The audience for reading: the tenant's base URL, e.g. `https://acme.privatecrates.dev`.
pub fn read(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_owned()
}

/// The audience for publishing one exact `.crate`.
pub fn publish(base_url: &str, name: &str, version: &str, cksum: &str) -> String {
    format!(
        "{}/publish/{}/{}/{}",
        base_url.trim_end_matches('/'),
        name.to_ascii_lowercase(),
        version,
        cksum.to_ascii_lowercase()
    )
}

/// The base URL of a registry from its sparse index URL, e.g.
/// `sparse+https://acme.privatecrates.dev/index/` → `https://acme.privatecrates.dev`.
pub fn base_url_from_index(index_url: &str) -> Option<String> {
    let url = index_url.strip_prefix("sparse+")?;
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix("/index")?;
    Some(url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_audience_is_normalised() {
        assert_eq!(
            publish(
                "https://acme.privatecrates.dev/",
                "Story_Engine",
                "0.2.0",
                "ABCD"
            ),
            "https://acme.privatecrates.dev/publish/story_engine/0.2.0/abcd"
        );
    }

    #[test]
    fn base_url_from_sparse_index() {
        assert_eq!(
            base_url_from_index("sparse+https://acme.privatecrates.dev/index/").as_deref(),
            Some("https://acme.privatecrates.dev")
        );
        assert_eq!(
            base_url_from_index("sparse+http://acme.localhost:8080/index").as_deref(),
            Some("http://acme.localhost:8080")
        );
        assert_eq!(base_url_from_index("https://example.com/index/"), None);
    }
}
