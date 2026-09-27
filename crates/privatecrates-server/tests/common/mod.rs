//! A running PrivateCrates server against a fake GitHub.
#![allow(dead_code)] // Each test binary uses a different part of the harness.

use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use bytes::Bytes;
use privatecrates_common::{audience, sha256_hex};
use privatecrates_server::{
    AppState,
    config::{AppConfig, Config, StripeConfig},
    router,
};
use privatecrates_testkit::{FakeGitHub, Org, READER_APP_ID, STORAGE_APP_ID, stripe::FakeStripe};
use serde_json::{Value, json};

/// Each App signs its webhooks with its own secret.
pub const READER_WEBHOOK_SECRET: &[u8] = b"the reader App's webhook secret";
pub const STORAGE_WEBHOOK_SECRET: &[u8] = b"the storage App's webhook secret";

pub struct Harness {
    pub fake: FakeGitHub,
    pub stripe: Option<FakeStripe>,
    pub org: Org,
    pub state: Arc<AppState>,
    pub port: u16,
    pub client: reqwest::Client,
}

/// What a test needs beyond the registry.
pub struct Options {
    /// The preview (`PREVIEW`), free for everyone, as the server defaults to; billing tests turn it off.
    pub preview: bool,
    /// Bill through a fake Stripe; otherwise every tenant is active.
    pub stripe: bool,
    pub website_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            preview: true,
            stripe: false,
            website_dir: None,
        }
    }
}

impl Harness {
    pub async fn start() -> Self {
        Self::start_with(Options::default()).await
    }

    pub async fn start_with(options: Options) -> Self {
        let fake = FakeGitHub::start().await;
        let stripe = match options.stripe {
            true => Some(FakeStripe::start().await),
            false => None,
        };
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
            reader_client_secret: privatecrates_testkit::READER_CLIENT_SECRET.into(),
            reader_app_slug: "privatecrates-reader".into(),
            storage_app_slug: "privatecrates-storage".into(),
            reader_app: AppConfig {
                id: READER_APP_ID,
                private_key_pem: privatecrates_testkit::app_private_key().as_bytes().to_vec(),
            },
            storage_app: AppConfig {
                id: STORAGE_APP_ID,
                private_key_pem: privatecrates_testkit::app_private_key().as_bytes().to_vec(),
            },
            registry_token_secret: b"a test secret that is long enough to sign with".to_vec(),
            webhook_secrets: vec![
                READER_WEBHOOK_SECRET.to_vec(),
                STORAGE_WEBHOOK_SECRET.to_vec(),
            ],
            session_secret: b"another test secret, long enough for a key".to_vec(),
            website_dir: options.website_dir,
            preview: options.preview,
            // Terms acceptances are kept in memory.
            database_url: None,
            stripe: stripe.as_ref().map(|s| StripeConfig {
                api: s.url.parse().unwrap(),
                secret_key: privatecrates_testkit::stripe::SECRET_KEY.into(),
                webhook_secret: privatecrates_testkit::stripe::WEBHOOK_SECRET.into(),
                price_id: privatecrates_testkit::stripe::PRICE_ID.into(),
            }),
            free_member_limit: 5,
            trial_days: 90,
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
        state.billing.load().await.unwrap();
        let app = router(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let local = SocketAddr::from(([127, 0, 0, 1], port));
        let client = reqwest::Client::builder()
            .resolve("localhost", local)
            .resolve("www.localhost", local)
            .resolve("acme.localhost", local)
            .resolve("other.localhost", local)
            .resolve("api.localhost", local)
            .resolve("globex.localhost", local)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        Self {
            fake,
            stripe,
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

    /// The website's URL, on the apex host.
    pub fn apex(&self, path: &str) -> String {
        format!("http://localhost:{}{path}", self.port)
    }

    /// Signs in on the website as the user holding `token`, as a browser would, and returns the session cookie
    /// (`pc_session=…`).
    pub async fn sign_in(&self, token: &str) -> String {
        let response = self
            .client
            .get(self.apex("/auth/github/login?return_to=/account%3Forg%3Dacme"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 302);
        let location =
            reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
        let state = location
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned();
        let sign_in = cookie(&response, "pc_sign_in").unwrap();
        let code = self.fake.web_flow_code(token);
        let response = self
            .client
            .get(self.apex("/auth/github/callback"))
            .query(&[("code", code.as_str()), ("state", state.as_str())])
            .header("Cookie", sign_in)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 302, "{}", response.text().await.unwrap());
        assert_eq!(response.headers()["location"], "/account?org=acme");
        cookie(&response, "pc_session").unwrap()
    }

    /// A request to the account API, from the website: JSON, with the website's `Origin`.
    pub fn api_post(&self, path: &str, session: &str) -> reqwest::RequestBuilder {
        self.client
            .post(self.apex(path))
            .header("Cookie", session)
            .header("Origin", self.apex(""))
            .header("Content-Type", "application/json")
    }

    pub async fn api_get(&self, path: &str, session: &str) -> reqwest::Response {
        self.client
            .get(self.apex(path))
            .header("Cookie", session)
            .send()
            .await
            .unwrap()
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

    /// Delivers a GitHub webhook to the website's host, signed with `secret`.
    pub async fn github_webhook(
        &self,
        secret: &[u8],
        event: &str,
        delivery: &str,
        payload: &Value,
    ) -> reqwest::Response {
        let body = serde_json::to_vec(payload).unwrap();
        let signature = privatecrates_testkit::github_webhook_signature(secret, &body);
        self.client
            .post(self.apex("/webhooks/github"))
            .header("X-GitHub-Event", event)
            .header("X-GitHub-Delivery", delivery)
            .header("X-Hub-Signature-256", signature)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
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

/// The `name=value` part of a `Set-Cookie` header of the response.
pub fn cookie(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .find(|v| v.starts_with(&format!("{name}=")))
        .map(str::to_owned)
}

pub async fn error_code(response: reqwest::Response) -> String {
    let body: Value = response.json().await.unwrap();
    body["errors"][0]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

pub async fn error_detail(response: reqwest::Response) -> String {
    let body: Value = response.json().await.unwrap();
    body["errors"][0]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}
