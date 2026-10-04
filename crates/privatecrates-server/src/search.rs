//! Search (SPEC §9): the caller's readable private crates first, labelled, then crates.io's results for the same
//! query.
//!
//! Each tenant has a small in-memory index, built lazily from its index files with the storage App's token and
//! brought up to date on each search: a crate is re-read only when its index file's blob sha changed, so publishes,
//! yanks and `push` webhooks are picked up without hooks of their own. Index lines hold no descriptions, so those
//! come from the latest version's `.crate`, cached forever because releases are immutable.

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
};
use privatecrates_common::index::IndexFile;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::{
    AppState,
    crate_file::{self, Metadata},
    error::ApiError,
    routes::{self, TenantHost},
    tenant::{Tenant, index_path},
};

const DEFAULT_PER_PAGE: usize = 10;
const MAX_PER_PAGE: usize = 100;
/// How long a search waits for crates.io before answering with private results alone.
const CRATES_IO_WAIT: Duration = Duration::from_secs(5);
const CLASH_NOTE: &str = " (a crate with this name also exists on crates.io)";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/crates", get(search))
}

/// A crate in a tenant's search index.
#[derive(Clone)]
struct Entry {
    /// The blob sha of the index file this entry was read from.
    index_sha: String,
    name: String,
    /// The latest version that is not yanked.
    max_version: String,
    /// `None` if the `.crate` could not be read; such entries are not kept, so the next search tries again.
    metadata: Option<Arc<Metadata>>,
}

type TenantIndex = HashMap<String, Entry>;

pub struct Search {
    /// Storage repository ID → lowercase crate name → entry.
    indexes: Mutex<HashMap<u64, Arc<tokio::sync::Mutex<TenantIndex>>>>,
    /// (storage repository ID, crate name, version) → what its `.crate` says.
    metadata: moka::future::Cache<(u64, String, String), Arc<Metadata>>,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            indexes: Mutex::default(),
            metadata: moka::future::Cache::builder().max_capacity(100_000).build(),
        }
    }
}

impl Search {
    /// Every crate in the tenant with a version that is not yanked, with its owning repository's ID.
    async fn entries(&self, state: &AppState, tenant: &Tenant) -> Vec<(Entry, u64)> {
        let index = self
            .indexes
            .lock()
            .expect("search indexes lock")
            .entry(tenant.storage_repo_id)
            .or_default()
            .clone();
        // Held while updating, so that concurrent searches do not read the same `.crate` files twice.
        let mut index = index.lock().await;
        let mut entries = Vec::new();
        for (name, owner) in tenant.owners() {
            let Some(sha) = tenant.file_sha(&index_path(&name)) else {
                continue;
            };
            let cached = index.get(&name).filter(|e| e.index_sha == sha).cloned();
            let entry = match cached {
                Some(entry) => Some(entry),
                None => self.load(state, tenant, &name).await.unwrap_or_else(|e| {
                    tracing::warn!(tenant = %tenant.slug, krate = %name, error = %e, "could not index crate for search");
                    None
                }),
            };
            if let Some(entry) = entry {
                entries.push((entry, owner.repository_id));
            }
        }
        *index = entries
            .iter()
            .filter(|(e, _)| e.metadata.is_some())
            .map(|(e, _)| (e.name.to_ascii_lowercase(), e.clone()))
            .collect();
        entries
    }

    /// Reads a crate's index file and its latest version's metadata. `None` if every version is yanked.
    async fn load(
        &self,
        state: &AppState,
        tenant: &Tenant,
        name: &str,
    ) -> Result<Option<Entry>, ApiError> {
        let Some((index_sha, content)) = tenant
            .file(&state.gh, &state.blobs, &index_path(name))
            .await?
        else {
            return Ok(None);
        };
        let file = std::str::from_utf8(&content)
            .map_err(|e| e.to_string())
            .and_then(|text| IndexFile::parse(text).map_err(|e| e.to_string()))
            .map_err(|e| ApiError::internal(format!("index file for {name} is corrupt: {e}")))?;
        let Some((name, max_version)) = latest(&file) else {
            return Ok(None);
        };
        let metadata = match self.metadata(state, tenant, &name, &max_version).await {
            Ok(metadata) => Some(metadata),
            Err(e) => {
                tracing::warn!(tenant = %tenant.slug, krate = %name, error = %e, "could not read crate metadata");
                None
            }
        };
        Ok(Some(Entry {
            index_sha,
            name,
            max_version,
            metadata,
        }))
    }

    /// A version's description and keywords, from its `.crate`'s `Cargo.toml`.
    async fn metadata(
        &self,
        state: &AppState,
        tenant: &Tenant,
        name: &str,
        version: &str,
    ) -> Result<Arc<Metadata>, ApiError> {
        let key = (tenant.storage_repo_id, name.to_owned(), version.to_owned());
        if let Some(metadata) = self.metadata.get(&key).await {
            return Ok(metadata);
        }
        let token = tenant.storage_token(&state.gh).await?;
        let asset_id = routes::crate_asset_id(state, tenant, &token, name, version).await?;
        let url = state
            .gh
            .asset_download_url(&token, &tenant.storage_repo, asset_id)
            .await?;
        let bytes = state.gh.download(&url).await?;
        let metadata = Arc::new(
            crate_file::metadata(&bytes, name, version)
                .map_err(|e| ApiError::internal(e.to_string()))?,
        );
        self.metadata.insert(key, metadata.clone()).await;
        Ok(metadata)
    }
}

/// The crate's name and its latest version that is not yanked, preferring stable versions as crates.io does.
fn latest(file: &IndexFile) -> Option<(String, String)> {
    let line = file
        .lines()
        .iter()
        .filter(|l| l.get("yanked").and_then(Value::as_bool) != Some(true))
        .filter_map(|l| {
            let version = semver::Version::parse(l.get("vers")?.as_str()?).ok()?;
            Some((version.pre.is_empty(), version, l))
        })
        .max_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)))?
        .2;
    Some((
        line.get("name")?.as_str()?.to_owned(),
        line.get("vers")?.as_str()?.to_owned(),
    ))
}

/// Crate names treat `-` and `_` as the same.
fn normalise(s: &str) -> String {
    s.to_lowercase().replace('-', "_")
}

/// How well a crate matches a query, best first: 0 for the exact name (or an empty query), 1 for a name prefix,
/// 2 for a name substring, 3 when every word is in the name or a keyword, 4 when every word is also found in
/// the description. `None` if it does not match.
fn rank(name: &str, metadata: Option<&Metadata>, query: &str) -> Option<u8> {
    let query = normalise(query.trim());
    let name = normalise(name);
    if query.is_empty() || name == query {
        return Some(0);
    }
    if name.starts_with(&query) {
        return Some(1);
    }
    if name.contains(&query) {
        return Some(2);
    }
    let keywords: Vec<String> = metadata
        .map(|m| m.keywords.iter().map(|k| normalise(k)).collect())
        .unwrap_or_default();
    let description = normalise(
        metadata
            .and_then(|m| m.description.as_deref())
            .unwrap_or_default(),
    );
    let in_name_or_keywords =
        |word: &str| name.contains(word) || keywords.iter().any(|k| k.contains(word));
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.iter().all(|w| in_name_or_keywords(w)) {
        return Some(3);
    }
    if words
        .iter()
        .all(|w| in_name_or_keywords(w) || description.contains(w))
    {
        return Some(4);
    }
    None
}

/// A private crate's description, labelled with the registry because `cargo search` shows only names and
/// descriptions (SPEC §9.2), and flagged if its name is also on crates.io (SPEC §9.3).
fn describe(slug: &str, description: Option<&str>, clash: bool) -> String {
    let mut out = format!("[{slug}]");
    let description = description
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !description.is_empty() {
        out.push(' ');
        out.push_str(&description);
    }
    if clash {
        out.push_str(CLASH_NOTE);
    }
    out
}

/// A crates.io call, bounded by the search's deadline. `None` if it failed or ran out of time.
async fn crates_io<T>(
    deadline: Instant,
    what: &str,
    call: impl Future<Output = Result<T, reqwest::Error>>,
) -> Option<T> {
    match tokio::time::timeout_at(deadline, call).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "crates.io {what} failed");
            None
        }
        Err(_) => {
            tracing::warn!("crates.io {what} timed out");
            None
        }
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    per_page: Option<usize>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    TenantHost(tenant): TenantHost,
    Query(query): Query<SearchQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let caller = routes::caller(&state, &tenant, &headers).await?;
    let resolver = routes::resolver(&state, &tenant);
    // As for the registry's configuration: an outsider gets nothing, crates.io's results included, and costs no
    // walk of the registry (the same answer for a name with no registry).
    if !resolver.can_use_registry(&caller).await? {
        return Err(ApiError::NoAccess);
    }
    let per_page = query
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);

    let mut matches = Vec::new();
    for (entry, repository_id) in state.search.entries(&state, &tenant).await {
        if let Some(rank) = rank(&entry.name, entry.metadata.as_deref(), &query.q)
            && resolver.can_read(&caller, repository_id).await?
        {
            matches.push((rank, entry));
        }
    }
    matches
        .sort_by(|(rank_a, a), (rank_b, b)| rank_a.cmp(rank_b).then_with(|| a.name.cmp(&b.name)));

    let deadline = Instant::now() + CRATES_IO_WAIT;
    let mut crates = Vec::new();
    for (_, entry) in matches.iter().take(per_page) {
        let clash = crates_io(deadline, "lookup", state.crates_io.exists(&entry.name))
            .await
            .unwrap_or(false);
        let description = entry
            .metadata
            .as_ref()
            .and_then(|m| m.description.as_deref());
        crates.push(json!({
            "name": entry.name,
            "max_version": entry.max_version,
            "description": describe(&tenant.slug, description, clash),
        }));
    }
    let mut total = matches.len();
    if let Some(page) = crates_io(
        deadline,
        "search",
        state.crates_io.search(&query.q, per_page),
    )
    .await
    {
        total += page.meta.total;
        let room = per_page - crates.len();
        crates.extend(page.crates.iter().take(room).map(|c| {
            json!({ "name": c.name, "max_version": c.max_version, "description": c.description })
        }));
    }
    Ok(Json(
        json!({ "crates": crates, "meta": { "total": total } }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(description: &str, keywords: &[&str]) -> Metadata {
        Metadata {
            description: Some(description.into()),
            keywords: keywords.iter().map(|k| (*k).into()).collect(),
        }
    }

    #[test]
    fn ranks_name_then_keywords_then_description() {
        let m = metadata(
            "Tell tales with branching plots",
            &["narrative", "game-dev"],
        );
        let m = Some(&m);
        assert_eq!(rank("story_engine", m, "story-engine"), Some(0));
        assert_eq!(rank("story_engine", m, ""), Some(0));
        assert_eq!(rank("story_engine", m, "Story"), Some(1));
        assert_eq!(rank("story_engine", m, "engine"), Some(2));
        assert_eq!(rank("story_engine", m, "narrative"), Some(3));
        assert_eq!(rank("story_engine", m, "story narr"), Some(3));
        assert_eq!(rank("story_engine", m, "game_dev"), Some(3));
        assert_eq!(rank("story_engine", m, "branching plots"), Some(4));
        assert_eq!(rank("story_engine", m, "branching dragons"), None);
        assert_eq!(rank("story_engine", None, "plots"), None);
    }

    #[test]
    fn latest_skips_yanked_and_prefers_stable() {
        let file = IndexFile::parse(concat!(
            r#"{"name":"A","vers":"0.9.0","yanked":false}"#,
            "\n",
            r#"{"name":"A","vers":"1.1.0","yanked":true}"#,
            "\n",
            r#"{"name":"A","vers":"1.0.0","yanked":false}"#,
            "\n",
            r#"{"name":"A","vers":"2.0.0-rc.1","yanked":false}"#,
            "\n",
        ))
        .unwrap();
        assert_eq!(latest(&file), Some(("A".into(), "1.0.0".into())));
        let file = IndexFile::parse(r#"{"name":"A","vers":"2.0.0-rc.1","yanked":false}"#).unwrap();
        assert_eq!(latest(&file), Some(("A".into(), "2.0.0-rc.1".into())));
        let file = IndexFile::parse(r#"{"name":"A","vers":"1.0.0","yanked":true}"#).unwrap();
        assert_eq!(latest(&file), None);
    }

    #[test]
    fn descriptions_are_labelled_and_flagged() {
        assert_eq!(
            describe("acme", Some("Tell\n  tales"), false),
            "[acme] Tell tales"
        );
        assert_eq!(describe("acme", None, false), "[acme]");
        assert_eq!(
            describe("acme", Some("Tales"), true),
            "[acme] Tales (a crate with this name also exists on crates.io)"
        );
    }
}
