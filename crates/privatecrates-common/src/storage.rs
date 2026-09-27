//! The layout of a tenant's storage repository (SPEC §5), shared by the server and the verifier.

use serde::{Deserialize, Serialize};

pub const SETTINGS_PATH: &str = "privatecrates.toml";
pub const INDEX_DIR: &str = "index/";
pub const OWNERS_DIR: &str = "owners/";

/// `owners/{name}.toml`: the repository that governs a crate (SPEC §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub repository_id: u64,
    pub repository: String,
    #[serde(default)]
    pub publish_workflows: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_environment: Option<String>,
    #[serde(default)]
    pub allow_manual_publish: bool,
}

pub fn owner_path(name: &str) -> String {
    format!("{OWNERS_DIR}{}.toml", name.to_ascii_lowercase())
}

/// The crate an owners file path is for, lowercased.
pub fn owner_name(path: &str) -> Option<String> {
    path.strip_prefix(OWNERS_DIR)?
        .strip_suffix(".toml")
        .filter(|n| !n.is_empty() && !n.contains('/'))
        .map(str::to_ascii_lowercase)
}

pub fn index_path(name: &str) -> String {
    format!("{INDEX_DIR}{}", crate::index::path(name))
}

pub fn release_tag(name: &str, version: &str) -> String {
    format!("{}-{version}", name.to_ascii_lowercase())
}

pub fn crate_asset_name(name: &str, version: &str) -> String {
    format!("{}.crate", release_tag(name, version))
}

pub fn provenance_asset_name(name: &str, version: &str) -> String {
    format!("{}.provenance.jwt", release_tag(name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_names() {
        assert_eq!(owner_path("Story_Engine"), "owners/story_engine.toml");
        assert_eq!(
            owner_name("owners/story_engine.toml").as_deref(),
            Some("story_engine")
        );
        assert_eq!(owner_name("owners/x/y.toml"), None);
        assert_eq!(owner_name("owners/.toml"), None);
        assert_eq!(index_path("story_engine"), "index/st/or/story_engine");
        assert_eq!(release_tag("Story_Engine", "0.2.0"), "story_engine-0.2.0");
        assert_eq!(crate_asset_name("a", "1.0.0"), "a-1.0.0.crate");
        assert_eq!(
            provenance_asset_name("a", "1.0.0"),
            "a-1.0.0.provenance.jwt"
        );
    }
}
