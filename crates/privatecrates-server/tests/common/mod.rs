//! A running PrivateCrates server against a fake GitHub.
#![allow(dead_code)] // Each test binary uses a different part of the harness.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use bytes::Bytes;
use privatecrates_common::{audience, sha256_hex};
use privatecrates_server::{
    AppState,
    config::{AppConfig, Config},
    router,
};
use privatecrates_testkit::{APP_PRIVATE_KEY, FakeGitHub, Org, READER_APP_ID, STORAGE_APP_ID};
use serde_json::{Value, json};

/// Each App signs its webhooks with its own secret.
pub const READER_WEBHOOK_SECRET: &[u8] = b"the reader App's webhook secret";
pub const STORAGE_WEBHOOK_SECRET: &[u8] = b"the storage App's webhook secret";

pub struct Harness {
    pub fake: FakeGitHub,
    pub org: Org,
    pub state: Arc<AppState>,
    pub port: u16,
    pub client: reqwest::Client,
}

impl Harness {
    pub async fn start() -> Self {
        let fake = FakeGitHub::start().await;
        let org = fake.add_org("acme", "acme");
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = Config {
            bind: SocketAddr::from(([127, 0, 0, 1], port)),
            base_domain: format!("localhost:{port}"),
            public_scheme: "http".into(),
            github_api: fake.url.parse().unwrap(),
            github_web: fake.url.parse().unwrap(),
            reader_client_id: privatecrates_testkit::READER_CLIENT_ID.into(),
            reader_app: AppConfig {
                id: READER_APP_ID,
                private_key_pem: APP_PRIVATE_KEY.as_bytes().to_vec(),
            },
            storage_app: AppConfig {
                id: STORAGE_APP_ID,
                private_key_pem: APP_PRIVATE_KEY.as_bytes().to_vec(),
            },
            registry_token_secret: b"a test secret that is long enough to sign with".to_vec(),
            webhook_secrets: vec![
                READER_WEBHOOK_SECRET.to_vec(),
                STORAGE_WEBHOOK_SECRET.to_vec(),
            ],
            oidc_issuer: fake.oidc_issuer(),
            oidc_jwks_url: fake.oidc_jwks_url().parse().unwrap(),
            crates_io_api: fake.url.parse().unwrap(),
            max_crate_bytes: 1024 * 1024,
            publish_rate_per_minute: 1000,
            permission_ttl: Duration::from_secs(300),
            tenant_refresh: Duration::from_secs(600),
            storage_refresh: Duration::from_secs(60),
        };
        let state = Arc::new(AppState::new(config).unwrap());
        state.discover().await.unwrap();
        let app = router(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let client = reqwest::Client::builder()
            .resolve("localhost", SocketAddr::from(([127, 0, 0, 1], port)))
            .resolve("acme.localhost", SocketAddr::from(([127, 0, 0, 1], port)))
            .resolve("other.localhost", SocketAddr::from(([127, 0, 0, 1], port)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        Self {
            fake,
            org,
            state,
            port,
            client,
        }
    }

    pub fn base(&self) -> String {
        format!("http://acme.localhost:{}", self.port)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base())
    }

    pub async fn get(&self, path: &str, token: Option<&str>) -> reqwest::Response {
        let mut request = self.client.get(self.url(path));
        if let Some(token) = token {
            request = request.header("Authorization", token);
        }
        request.send().await.unwrap()
    }

    /// Adds a crate repository in the organisation.
    pub fn repo(&self, name: &str) -> u64 {
        self.fake.add_repo(&self.org, name)
    }

    /// A bound OIDC token for publishing `krate` from `workflow` in `repo`.
    pub fn publish_token(&self, repo: &str, repo_id: u64, workflow: &str, krate: &Crate) -> String {
        let claims = FakeGitHub::actions_claims(&self.org, repo, repo_id, workflow);
        self.fake.oidc_token(
            &audience::publish(&self.base(), &krate.name, &krate.version, &krate.cksum()),
            &claims,
        )
    }

    pub async fn publish(&self, krate: &Crate, token: &str) -> reqwest::Response {
        self.client
            .put(self.url("/api/v1/crates/new"))
            .header("Authorization", token)
            .body(krate.body())
            .send()
            .await
            .unwrap()
    }

    /// Publishes from CI with provenance, asserting success.
    pub async fn publish_from_ci(&self, repo: &str, repo_id: u64, krate: &Crate) {
        let token = self.publish_token(repo, repo_id, "release.yml", krate);
        let response = self.publish(krate, &token).await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, 200, "publish failed: {body}");
    }

    /// Makes the server re-read the storage repository, as the periodic refresh or a webhook would.
    pub async fn refresh(&self) {
        self.state.refresh_storage().await;
    }
}

/// A crate to publish: metadata plus a `.crate` built to match.
#[derive(Clone)]
pub struct Crate {
    pub name: String,
    pub version: String,
    pub repository: Option<String>,
    pub deps: Vec<Value>,
    /// Extra files in the archive, to vary the bytes.
    pub extra: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
}

impl Crate {
    pub fn new(name: &str, version: &str, repository: &str) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            repository: Some(format!("https://github.com/{repository}")),
            deps: Vec::new(),
            extra: String::new(),
            description: None,
            keywords: Vec::new(),
        }
    }

    pub fn described(mut self, description: &str, keywords: &[&str]) -> Self {
        self.description = Some(description.into());
        self.keywords = keywords.iter().map(|k| (*k).into()).collect();
        self
    }

    pub fn dep(mut self, name: &str, req: &str, registry: Option<&str>) -> Self {
        self.deps.push(json!({
            "name": name, "version_req": req, "features": [], "optional": false,
            "default_features": true, "target": null, "kind": "normal", "registry": registry,
        }));
        self
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let prefix = format!("{}-{}", self.name, self.version);
        let mut manifest = format!(
            "[package]\nname = \"{}\"\nversion = \"{}\"\n",
            self.name, self.version
        );
        if let Some(description) = &self.description {
            manifest.push_str(&format!("description = {description:?}\n"));
        }
        if !self.keywords.is_empty() {
            manifest.push_str(&format!("keywords = {:?}\n", self.keywords));
        }
        for (path, content) in [
            (format!("{prefix}/Cargo.toml"), manifest),
            (format!("{prefix}/src/lib.rs"), self.extra.clone()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder
                .append_data(&mut header, path, content.as_bytes())
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    pub fn cksum(&self) -> String {
        sha256_hex(&self.bytes())
    }

    pub fn body(&self) -> Bytes {
        let meta = serde_json::to_vec(&json!({
            "name": self.name, "vers": self.version, "deps": self.deps, "features": {},
            "authors": [], "description": self.description, "documentation": null, "homepage": null,
            "readme": null, "readme_file": null, "keywords": self.keywords, "categories": [], "license": "MIT", "license_file": null,
            "repository": self.repository, "badges": {}, "links": null, "rust_version": null,
        }))
        .unwrap();
        let krate = self.bytes();
        let mut out = Vec::new();
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(&meta);
        out.extend_from_slice(&(krate.len() as u32).to_le_bytes());
        out.extend_from_slice(&krate);
        Bytes::from(out)
    }
}

pub async fn error_detail(response: reqwest::Response) -> String {
    let body: Value = response.json().await.unwrap();
    body["errors"][0]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}
