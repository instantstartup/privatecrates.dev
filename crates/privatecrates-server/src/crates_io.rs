//! The two crates.io lookups we need: does a name exist there (SPEC §9.3), and search (SPEC §9.2).

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use tokio::{sync::Mutex, time::Instant};
use url::Url;

/// crates.io's data access policy asks for at most one request per second.
const MIN_INTERVAL: Duration = Duration::from_secs(1);

pub struct CratesIo {
    http: reqwest::Client,
    api: Url,
    exists: moka::future::Cache<String, bool>,
    /// (query, per page) → results.
    searches: moka::future::Cache<(String, usize), Arc<SearchPage>>,
    /// When the next request may be sent.
    next_request: Mutex<Instant>,
}

/// One page of crates.io search results: only the fields Cargo shows.
#[derive(Debug, Deserialize)]
pub struct SearchPage {
    pub crates: Vec<SearchResult>,
    pub meta: SearchMeta,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    pub name: String,
    pub max_version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchMeta {
    pub total: usize,
}

impl CratesIo {
    pub fn new(api: Url) -> Self {
        Self {
            // crates.io's crawler policy asks for an identifying User-Agent.
            http: reqwest::Client::builder()
                .user_agent("privatecrates (https://privatecrates.dev; support@privatecrates.dev)")
                .timeout(Duration::from_secs(10))
                .build()
                .expect("a basic HTTP client builds"),
            api,
            exists: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(60 * 60))
                .build(),
            searches: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(10 * 60))
                .build(),
            next_request: Mutex::new(Instant::now()),
        }
    }

    /// Waits until the rate limit allows another request.
    async fn pace(&self) {
        let mut next = self.next_request.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = Instant::now() + MIN_INTERVAL;
    }

    /// Whether a crate with this name exists on crates.io.
    pub async fn exists(&self, name: &str) -> Result<bool, reqwest::Error> {
        let name = name.to_ascii_lowercase();
        if let Some(found) = self.exists.get(&name).await {
            return Ok(found);
        }
        let url = self
            .api
            .join(&format!("api/v1/crates/{name}"))
            .expect("crate names are valid URL segments");
        self.pace().await;
        let response = self.http.get(url).send().await?;
        let found = match response.status() {
            reqwest::StatusCode::NOT_FOUND => false,
            _ => {
                response.error_for_status()?;
                true
            }
        };
        self.exists.insert(name, found).await;
        Ok(found)
    }

    /// Searches crates.io, as `cargo search` would.
    pub async fn search(
        &self,
        query: &str,
        per_page: usize,
    ) -> Result<Arc<SearchPage>, reqwest::Error> {
        let key = (query.to_owned(), per_page);
        if let Some(page) = self.searches.get(&key).await {
            return Ok(page);
        }
        let url = self.api.join("api/v1/crates").expect("a valid URL path");
        self.pace().await;
        let page: Arc<SearchPage> = Arc::new(
            self.http
                .get(url)
                .query(&[("q", query), ("per_page", &per_page.to_string())])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?,
        );
        self.searches.insert(key, page.clone()).await;
        Ok(page)
    }
}
