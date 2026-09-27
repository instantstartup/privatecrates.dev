//! Tenants and their storage repositories (SPEC §2.1, §5).
//!
//! There is no database. Tenants come from our Apps' installations; each tenant's settings, owners and index come
//! from its storage repository. The repository's tree is refreshed with conditional requests, and blobs are cached
//! by sha forever, because a git blob sha names its content.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use apollo_errors::Error;
use bytes::Bytes;
use miette::Diagnostic;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::github::{AppKind, Conditional, GitHub, GitHubError, Repo};
pub use privatecrates_common::storage::{
    OWNERS_DIR, Owner, RepositorySettings, SETTINGS_PATH, index_path, manual_publish_allowed,
    owner_name, owner_path,
};

/// Subdomains of the base domain that are never tenants (docs/website-api.md).
pub const RESERVED_SLUGS: &[&str] = &[
    "www", "dev", "api", "app", "docs", "status", "mail", "admin", "billing", "login", "static",
    "assets",
];

pub fn is_reserved(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

pub use privatecrates_common::slug_is_valid;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub slug: String,
    #[serde(default)]
    pub name_clash: NameClash,
    #[serde(default)]
    pub ci_read: CiRead,
    /// The default for every repository: may its crates also be published from a developer's machine, first
    /// versions included (SPEC §6.4)?
    #[serde(default)]
    pub allow_manual_publish: bool,
    /// Per-repository settings, by repository name (`tools`, or `acme/tools`), overriding the defaults above.
    #[serde(default)]
    pub repositories: BTreeMap<String, RepositorySettings>,
}

impl Settings {
    /// Whether crates in a repository may be published from a developer's machine, by its names (`owner/name`):
    /// its current name first, then the one recorded at its crates' first publish.
    pub fn allows_manual_publish(&self, names: &[&str]) -> bool {
        manual_publish_allowed(self.allow_manual_publish, &self.repositories, names)
    }

    /// Whether any repository may publish from a developer's machine: otherwise there is nothing to look up.
    pub fn any_manual_publish(&self) -> bool {
        self.allow_manual_publish
            || self
                .repositories
                .values()
                .any(|r| r.allow_manual_publish == Some(true))
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NameClash {
    #[default]
    Refuse,
    Warn,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CiRead {
    /// An Actions OIDC token from any repository in the organisation may read every crate.
    #[default]
    Organisation,
}

/// What we know of a storage repository's current commit.
#[derive(Default)]
struct Snapshot {
    etag: Option<String>,
    /// Path → blob sha, for every file.
    files: HashMap<String, String>,
    /// Lowercase crate name → owner.
    owners: HashMap<String, Owner>,
}

pub struct Tenant {
    pub slug: String,
    pub org_login: String,
    pub org_id: u64,
    pub reader_installation: u64,
    pub storage_installation: u64,
    /// `owner/name` of the storage repository.
    pub storage_repo: String,
    pub storage_repo_id: u64,
    pub branch: String,
    pub settings: Settings,
    snapshot: RwLock<Snapshot>,
    /// Serialises writes to the storage repository from this instance.
    pub write_lock: Mutex<()>,
}

/// Blobs by sha, shared by all tenants. A blob sha is a hash of the content, so entries never go stale.
pub type BlobCache = moka::future::Cache<String, Bytes>;

pub fn blob_cache() -> BlobCache {
    moka::future::Cache::builder()
        .weigher(|_: &String, v: &Bytes| u32::try_from(v.len()).unwrap_or(u32::MAX))
        .max_capacity(256 * 1024 * 1024)
        .build()
}

#[derive(Debug, Error, Diagnostic)]
pub enum TenantError {
    #[error("{source}")]
    #[diagnostic(code(tenant::github))]
    GitHub {
        #[from]
        source: GitHubError,
    },
    #[error("the storage App must be installed on exactly one repository, not {count}")]
    #[diagnostic(code(tenant::storage_repositories))]
    StorageRepositories { count: usize },
    #[error("privatecrates.toml is missing from the storage repository")]
    #[diagnostic(code(tenant::no_settings))]
    NoSettings,
    #[error("{path} is invalid: {reason}")]
    #[diagnostic(code(tenant::invalid_file))]
    Invalid { path: String, reason: String },
    #[error("the storage repository's tree is too large to list")]
    #[diagnostic(code(tenant::tree_truncated))]
    Truncated,
}

impl Tenant {
    pub async fn storage_token(&self, gh: &GitHub) -> Result<String, GitHubError> {
        gh.installation_token(AppKind::Storage, self.storage_installation)
            .await
    }

    pub fn owner(&self, name: &str) -> Option<Owner> {
        self.snapshot
            .read()
            .expect("snapshot lock")
            .owners
            .get(&name.to_ascii_lowercase())
            .cloned()
    }

    pub fn owners(&self) -> Vec<(String, Owner)> {
        self.snapshot
            .read()
            .expect("snapshot lock")
            .owners
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The paths of the storage repository's files that start with `prefix`, sorted.
    pub fn paths(&self, prefix: &str) -> Vec<String> {
        let mut paths: Vec<String> = self
            .snapshot
            .read()
            .expect("snapshot lock")
            .files
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect();
        paths.sort();
        paths
    }

    /// The blob sha of a file in the storage repository, if it exists.
    pub fn file_sha(&self, path: &str) -> Option<String> {
        self.snapshot
            .read()
            .expect("snapshot lock")
            .files
            .get(path)
            .cloned()
    }

    /// A file's content and blob sha, if it exists.
    pub async fn file(
        &self,
        gh: &GitHub,
        blobs: &BlobCache,
        path: &str,
    ) -> Result<Option<(String, Bytes)>, GitHubError> {
        let Some(sha) = self.file_sha(path) else {
            return Ok(None);
        };
        let content = blob(self, gh, blobs, &sha).await?;
        Ok(Some((sha, content)))
    }

    /// Records a write we made, so reads see it without waiting for the next refresh.
    pub async fn record_write(&self, blobs: &BlobCache, path: &str, sha: String, content: Bytes) {
        if let Some(name) = owner_name(path)
            && let Ok(owner) = parse_owner(path, &content)
        {
            self.snapshot
                .write()
                .expect("snapshot lock")
                .owners
                .insert(name, owner);
        }
        blobs.insert(sha.clone(), content).await;
        let mut snapshot = self.snapshot.write().expect("snapshot lock");
        snapshot.files.insert(path.to_owned(), sha);
        // Our own commit changed the tree, so the next conditional refresh must not be skipped.
        snapshot.etag = None;
    }

    /// Re-reads the storage repository's tree, if it changed. Cheap when nothing changed: a conditional request.
    pub async fn refresh(&self, gh: &GitHub, blobs: &BlobCache) -> Result<(), TenantError> {
        let token = self.storage_token(gh).await?;
        let etag = self.snapshot.read().expect("snapshot lock").etag.clone();
        let (tree, etag) = match gh
            .tree(&token, &self.storage_repo, &self.branch, etag.as_deref())
            .await?
        {
            Conditional::NotModified => return Ok(()),
            Conditional::Modified { value, etag } => (value, etag),
        };
        if tree.truncated {
            return Err(TenantError::Truncated);
        }
        let files: HashMap<String, String> = tree
            .tree
            .into_iter()
            .filter(|e| e.kind == "blob")
            .map(|e| (e.path, e.sha))
            .collect();
        let mut owners = HashMap::new();
        for (path, sha) in &files {
            let Some(name) = owner_name(path) else {
                continue;
            };
            let content = blob(self, gh, blobs, sha).await?;
            match parse_owner(path, &content) {
                Ok(owner) => {
                    owners.insert(name, owner);
                }
                // One bad owners file must not take the whole tenant down; its crate becomes unreadable.
                Err(e) => tracing::warn!(tenant = %self.slug, error = %e, "ignoring owners file"),
            }
        }
        *self.snapshot.write().expect("snapshot lock") = Snapshot {
            etag,
            files,
            owners,
        };
        Ok(())
    }
}

async fn blob(
    tenant: &Tenant,
    gh: &GitHub,
    blobs: &BlobCache,
    sha: &str,
) -> Result<Bytes, GitHubError> {
    if let Some(content) = blobs.get(sha).await {
        return Ok(content);
    }
    let token = tenant.storage_token(gh).await?;
    let content = gh.blob(&token, &tenant.storage_repo, sha).await?;
    blobs.insert(sha.to_owned(), content.clone()).await;
    Ok(content)
}

fn parse_owner(path: &str, content: &[u8]) -> Result<Owner, TenantError> {
    let text = std::str::from_utf8(content).map_err(|e| TenantError::Invalid {
        path: path.into(),
        reason: e.to_string(),
    })?;
    toml::from_str(text).map_err(|e| TenantError::Invalid {
        path: path.into(),
        reason: e.to_string(),
    })
}

/// All tenants, by slug.
#[derive(Default)]
pub struct Tenants {
    by_slug: RwLock<HashMap<String, Arc<Tenant>>>,
}

impl Tenants {
    pub fn get(&self, slug: &str) -> Option<Arc<Tenant>> {
        self.by_slug
            .read()
            .expect("tenants lock")
            .get(slug)
            .cloned()
    }

    pub fn by_org(&self, org_id: u64) -> Option<Arc<Tenant>> {
        self.by_slug
            .read()
            .expect("tenants lock")
            .values()
            .find(|t| t.org_id == org_id)
            .cloned()
    }

    pub fn all(&self) -> Vec<Arc<Tenant>> {
        self.by_slug
            .read()
            .expect("tenants lock")
            .values()
            .cloned()
            .collect()
    }

    /// Rebuilds the tenant list from our Apps' installations. An organisation is a tenant when it has installed
    /// both Apps, the storage App on exactly one repository with a valid settings file.
    pub async fn discover(&self, gh: &GitHub, blobs: &BlobCache) -> Result<(), GitHubError> {
        let readers: HashMap<u64, u64> = gh
            .installations(AppKind::Reader)
            .await?
            .into_iter()
            .map(|i| (i.account.id, i.id))
            .collect();
        let mut found = HashMap::new();
        for storage in gh.installations(AppKind::Storage).await? {
            let Some(&reader) = readers.get(&storage.account.id) else {
                continue;
            };
            let existing = self
                .all()
                .into_iter()
                .find(|t| t.org_id == storage.account.id);
            match load(gh, blobs, reader, storage.id, &storage.account, existing).await {
                Ok(tenant) => {
                    if found.contains_key(&tenant.slug) {
                        tracing::error!(slug = %tenant.slug, "two organisations claim the same slug; serving neither");
                        found.remove(&tenant.slug);
                        continue;
                    }
                    found.insert(tenant.slug.clone(), tenant);
                }
                Err(e) => {
                    tracing::warn!(org = %storage.account.login, error = %e, "organisation is not a working tenant");
                }
            }
        }
        *self.by_slug.write().expect("tenants lock") = found;
        Ok(())
    }

    #[cfg(test)]
    pub fn insert(&self, tenant: Tenant) {
        self.by_slug
            .write()
            .expect("tenants lock")
            .insert(tenant.slug.clone(), Arc::new(tenant));
    }
}

async fn load(
    gh: &GitHub,
    blobs: &BlobCache,
    reader_installation: u64,
    storage_installation: u64,
    org: &crate::github::Account,
    existing: Option<Arc<Tenant>>,
) -> Result<Arc<Tenant>, TenantError> {
    let token = gh
        .installation_token(AppKind::Storage, storage_installation)
        .await?;
    let repos = gh.installation_repositories(&token).await?;
    let [repo]: [Repo; 1] = repos
        .try_into()
        .map_err(|r: Vec<Repo>| TenantError::StorageRepositories { count: r.len() })?;
    let tenant = Tenant {
        slug: String::new(),
        org_login: org.login.clone(),
        org_id: org.id,
        reader_installation,
        storage_installation,
        storage_repo: repo.full_name.clone(),
        storage_repo_id: repo.id,
        branch: repo.default_branch.clone(),
        settings: Settings {
            slug: String::new(),
            name_clash: NameClash::default(),
            ci_read: CiRead::default(),
            allow_manual_publish: false,
            repositories: BTreeMap::new(),
        },
        snapshot: RwLock::new(Snapshot::default()),
        write_lock: Mutex::new(()),
    };
    // Keep what the previous instance of this tenant already knew, so a rediscovery costs a conditional request.
    if let Some(existing) = existing.filter(|t| t.storage_repo_id == repo.id) {
        let old = existing.snapshot.read().expect("snapshot lock");
        *tenant.snapshot.write().expect("snapshot lock") = Snapshot {
            etag: old.etag.clone(),
            files: old.files.clone(),
            owners: old.owners.clone(),
        };
    }
    tenant.refresh(gh, blobs).await?;
    let (_, settings) = tenant
        .file(gh, blobs, SETTINGS_PATH)
        .await?
        .ok_or(TenantError::NoSettings)?;
    let settings = parse_settings(&settings)?;
    Ok(Arc::new(Tenant {
        slug: settings.slug.clone(),
        settings,
        ..tenant
    }))
}

pub fn parse_settings(content: &[u8]) -> Result<Settings, TenantError> {
    let invalid = |reason: String| TenantError::Invalid {
        path: SETTINGS_PATH.into(),
        reason,
    };
    let text = std::str::from_utf8(content).map_err(|e| invalid(e.to_string()))?;
    let settings: Settings = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
    if !slug_is_valid(&settings.slug) {
        return Err(invalid(
            "slug must be 1 to 63 lowercase letters, digits or hyphens".into(),
        ));
    }
    if is_reserved(&settings.slug) {
        return Err(invalid(format!("the slug {} is reserved", settings.slug)));
    }
    Ok(settings)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn tenant(slug: &str) -> Tenant {
        Tenant {
            slug: slug.into(),
            org_login: "acme".into(),
            org_id: 100,
            reader_installation: 1,
            storage_installation: 2,
            storage_repo: "acme/crates-store".into(),
            storage_repo_id: 200,
            branch: "main".into(),
            settings: Settings {
                slug: slug.into(),
                name_clash: NameClash::Refuse,
                ci_read: CiRead::Organisation,
                allow_manual_publish: false,
                repositories: BTreeMap::new(),
            },
            snapshot: RwLock::new(Snapshot::default()),
            write_lock: Mutex::new(()),
        }
    }

    #[test]
    fn settings() {
        let s = parse_settings(b"slug = \"acme\"\n").unwrap();
        assert_eq!(s.name_clash, NameClash::Refuse);
        assert_eq!(s.ci_read, CiRead::Organisation);
        assert!(!s.allow_manual_publish);
        assert!(
            parse_settings(b"slug = \"acme\"\nallow_manual_publish = true\n")
                .unwrap()
                .allow_manual_publish
        );
        let s = parse_settings(
            b"slug = \"acme\"\n\n[repositories.tools]\nallow_manual_publish = true\n\n\
              [repositories.\"acme/Core\"]\nallow_manual_publish = false\n",
        )
        .unwrap();
        assert!(s.allows_manual_publish(&["acme/tools"]));
        assert!(!s.allows_manual_publish(&["acme/core"]));
        assert!(!s.allows_manual_publish(&["acme/other"]));
        // The current name decides; an earlier one counts only when the current has no setting.
        assert!(!s.allows_manual_publish(&["acme/core", "acme/tools"]));
        assert!(s.allows_manual_publish(&["acme/renamed", "acme/tools"]));
        assert!(s.any_manual_publish());
        let s = parse_settings(
            b"slug = \"acme\"\nallow_manual_publish = true\n[repositories.core]\nallow_manual_publish = false\n",
        )
        .unwrap();
        assert!(s.allows_manual_publish(&["acme/tools"]));
        assert!(!s.allows_manual_publish(&["acme/core"]));
        assert!(parse_settings(b"slug = \"acme\"\n[repositories.core]\nunknown = 1\n").is_err());
        let s = parse_settings(b"slug = \"acme-2\"\nname_clash = \"warn\"\n").unwrap();
        assert_eq!(s.name_clash, NameClash::Warn);
        assert!(parse_settings(b"slug = \"Acme\"").is_err());
        assert!(parse_settings(b"slug = \"a.b\"").is_err());
        assert!(parse_settings(b"slug = \"acme\"\nunknown = 1").is_err());
        assert!(parse_settings(b"slug = \"www\"").is_err());
    }

    #[test]
    fn owner_files() {
        assert_eq!(
            owner_name("owners/story_engine.toml").as_deref(),
            Some("story_engine")
        );
        assert_eq!(owner_name("owners/x/y.toml"), None);
        assert_eq!(owner_name("index/st/or/story_engine"), None);
        let owner = parse_owner(
            "owners/a.toml",
            b"repository_id = 5\nrepository = \"acme/a\"\npublish_workflows = [\"release.yml\"]\n",
        )
        .unwrap();
        assert_eq!(owner.repository_id, 5);
        assert!(!owner.allow_manual_publish);
        let rendered = toml::to_string(&owner).unwrap();
        assert_eq!(
            parse_owner("owners/a.toml", rendered.as_bytes()).unwrap(),
            owner
        );
    }

    #[tokio::test]
    async fn recorded_writes_are_visible() {
        let t = tenant("acme");
        let blobs = blob_cache();
        t.record_write(
            &blobs,
            "owners/a.toml",
            "s1".into(),
            Bytes::from_static(b"repository_id = 5\nrepository = \"acme/a\"\n"),
        )
        .await;
        assert_eq!(t.owner("A").unwrap().repository_id, 5);
        assert_eq!(t.file_sha("owners/a.toml").as_deref(), Some("s1"));
    }
}
