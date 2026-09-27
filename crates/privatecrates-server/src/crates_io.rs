//! The one crates.io lookup we need: does a name exist there (SPEC §9.3).

use std::time::Duration;

use url::Url;

pub struct CratesIo {
    http: reqwest::Client,
    api: Url,
    exists: moka::future::Cache<String, bool>,
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
        }
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
}
