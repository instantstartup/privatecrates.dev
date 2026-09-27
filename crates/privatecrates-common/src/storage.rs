//! The layout of a tenant's storage repository (SPEC §5), shared by the server and the verifier.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SETTINGS_PATH: &str = "privatecrates.toml";
pub const INDEX_DIR: &str = "index/";
pub const OWNERS_DIR: &str = "owners/";

/// `owners/{name}.toml`: the repository that governs a crate (SPEC §6.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub repository_id: u64,
    pub repository: String,
    /// The workflow files in the owning repository that may publish. Empty (a crate first published from a
    /// developer's machine) means any of them.
    #[serde(default)]
    pub publish_workflows: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_environment: Option<String>,
    #[serde(default)]
    pub allow_manual_publish: bool,
    /// GitHub App bots, such as `release-please[bot]`, whose workflow runs may publish although a bot cannot be
    /// given Write access to the repository. Every other actor needs it (SPEC §6.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publish_bots: Vec<String>,
}

impl Owner {
    /// Whether a workflow file of the owning repository may publish (SPEC §6.2).
    pub fn allows_workflow(&self, workflow: &str) -> bool {
        self.publish_workflows.is_empty() || self.publish_workflows.iter().any(|w| w == workflow)
    }

    /// Whether `actor` is a bot listed in `publish_bots`. GitHub logins are case-insensitive.
    pub fn allows_bot(&self, actor: &str) -> bool {
        actor.ends_with("[bot]")
            && self
                .publish_bots
                .iter()
                .any(|bot| bot.eq_ignore_ascii_case(actor))
    }
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

/// A repository's own settings in `privatecrates.toml` (`[repositories.<name>]`), overriding the defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySettings {
    pub allow_manual_publish: Option<bool>,
}

/// Whether crates in a repository may be published from a developer's machine (SPEC §6.4): the setting of the first
/// of `names` (`owner/name`: the current name, then earlier ones) that has its own, else the default. A key is a
/// repository's name, with or without the organisation.
pub fn manual_publish_allowed(
    default: bool,
    repositories: &BTreeMap<String, RepositorySettings>,
    names: &[&str],
) -> bool {
    names
        .iter()
        .find_map(|full_name| {
            let name = full_name.rsplit('/').next().unwrap_or(full_name);
            repositories
                .iter()
                .find(|(key, _)| {
                    key.eq_ignore_ascii_case(full_name) || key.eq_ignore_ascii_case(name)
                })
                .and_then(|(_, settings)| settings.allow_manual_publish)
        })
        .unwrap_or(default)
}

/// The publishing settings of `privatecrates.toml`, read leniently (other settings are ignored), for the verifier.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PublishSettings {
    #[serde(default)]
    pub allow_manual_publish: bool,
    #[serde(default)]
    pub repositories: BTreeMap<String, RepositorySettings>,
}

impl PublishSettings {
    pub fn allows_manual_publish(&self, names: &[&str]) -> bool {
        manual_publish_allowed(self.allow_manual_publish, &self.repositories, names)
    }
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

    #[test]
    fn allowed_workflows() {
        let mut owner: Owner =
            serde_json::from_str(r#"{"repository_id":5,"repository":"acme/a"}"#).unwrap();
        assert!(owner.allows_workflow("anything.yml"));
        owner.publish_workflows = vec!["release.yml".into()];
        assert!(owner.allows_workflow("release.yml"));
        assert!(!owner.allows_workflow("anything.yml"));
    }

    #[test]
    fn listed_bots() {
        let owner: Owner = serde_json::from_str(
            r#"{"repository_id":5,"repository":"acme/a","publish_bots":["release-please[bot]","alice"]}"#,
        )
        .unwrap();
        assert!(owner.allows_bot("release-please[bot]"));
        assert!(owner.allows_bot("Release-Please[bot]"));
        assert!(!owner.allows_bot("renovate[bot]"));
        // Only bots: a person needs Write access, listed or not.
        assert!(!owner.allows_bot("alice"));
        let bare: Owner =
            serde_json::from_str(r#"{"repository_id":5,"repository":"acme/a"}"#).unwrap();
        assert!(bare.publish_bots.is_empty());
    }
}
