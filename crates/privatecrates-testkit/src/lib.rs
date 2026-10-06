//! An in-process fake of the GitHub (and crates.io) endpoints PrivateCrates uses, for tests.
//!
//! It models what the design depends on: installation and user permissions, SAML SSO refusals, blob shas and
//! optimistic concurrency on file writes, draft and immutable releases with asset digests, signed download
//! redirects, and Actions OIDC tokens.

// Handlers return ready-made error responses; their size does not matter in a test fake.
#![allow(clippy::result_large_err)]

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use base64::Engine;
use bytes::Bytes;
use hmac::{Hmac, KeyInit, Mac};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub mod stripe;

/// An RSA key generated for this test run: never written to disk or committed, so there is no key to leak or to
/// trip secret scanners.
struct TestKey {
    /// PKCS #8, PEM-encoded.
    pem: String,
    /// The public modulus, base64url without padding, as a JWK's `n`.
    modulus: String,
}

impl TestKey {
    fn generate() -> Self {
        use aws_lc_rs::{
            encoding::AsDer,
            rsa::{KeyPair, KeySize},
            signature::KeyPair as _,
        };
        let pair = KeyPair::generate(KeySize::Rsa2048).expect("generate a test RSA key");
        let der = pair.as_der().expect("encode the test key");
        let body = base64::engine::general_purpose::STANDARD.encode(der.as_ref());
        let lines: Vec<&str> = body
            .as_bytes()
            .chunks(64)
            .map(|line| std::str::from_utf8(line).expect("base64 is ASCII"))
            .collect();
        let modulus = pair.public_key().modulus();
        Self {
            pem: format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
                lines.join("\n")
            ),
            modulus: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(modulus.big_endian_without_leading_zero()),
        }
    }
}

static APP_KEY: std::sync::LazyLock<TestKey> = std::sync::LazyLock::new(TestKey::generate);
static OIDC_KEY: std::sync::LazyLock<TestKey> = std::sync::LazyLock::new(TestKey::generate);

/// The private key of the fake Apps, PEM-encoded; generated once per test run.
pub fn app_private_key() -> &'static str {
    &APP_KEY.pem
}

const OIDC_KID: &str = "fake-oidc-key";
/// The bearer token the fake Actions runtime expects when a job requests an OIDC token.
pub const ACTIONS_REQUEST_TOKEN: &str = "fake-actions-request-token";

pub const READER_APP_ID: u64 = 1;
/// The reader App's OAuth client ID, for the device flow.
pub const READER_CLIENT_ID: &str = "Iv1.fakereader";
/// The reader App's client secret, for the web sign-in flow.
pub const READER_CLIENT_SECRET: &str = "fake-reader-client-secret";
pub const STORAGE_APP_ID: u64 = 2;
/// The bot accounts the Apps' commits are attributed to.
pub const READER_APP_LOGIN: &str = "privatecrates-reader[bot]";
/// The user who starts the workflow runs of [`FakeGitHub::actions_claims`].
pub const ACTIONS_ACTOR: &str = "releaser";
pub const STORAGE_APP_LOGIN: &str = "privatecrates-storage[bot]";

#[derive(Clone, Debug)]
pub struct Org {
    pub id: u64,
    pub login: String,
    pub reader_installation: u64,
    pub storage_installation: u64,
    pub storage_repo: u64,
    /// A user's personal account rather than an organisation: its ID and login are the user's.
    pub personal: bool,
}

#[derive(Default)]
struct Repo {
    id: u64,
    owner_id: u64,
    owner: String,
    name: String,
    files: BTreeMap<String, String>,
    immutable_releases: bool,
    reserved_tags: HashSet<String>,
    commits: Vec<Commit>,
}

/// A commit, each changing one file: made through the Contents API, or directly as test setup.
#[derive(Clone, Debug)]
pub struct Commit {
    pub sha: String,
    pub path: String,
    pub message: String,
    pub by_installation: Option<u64>,
    /// The GitHub account the author is linked to: an App's bot for its installation's commits.
    pub author: Option<String>,
    /// Whether GitHub verified the signature, as it does for commits made through the API.
    pub verified: bool,
    /// Unix seconds.
    pub at: u64,
}

#[derive(Clone, Debug)]
struct Release {
    id: u64,
    repo: u64,
    tag: String,
    draft: bool,
    immutable: bool,
    body: String,
    assets: Vec<u64>,
}

#[derive(Clone, Debug)]
struct Asset {
    id: u64,
    name: String,
    content: Bytes,
}

#[derive(Clone, Debug)]
struct User {
    id: u64,
    login: String,
    /// Repository ID → can push.
    repos: HashMap<u64, bool>,
    /// Repository ID → the user's role there, when set by [`FakeGitHub::set_repo_role`]; otherwise `write` if they
    /// can push and `read` if not.
    roles: HashMap<u64, String>,
    /// The organisation enforces SSO and this token has not been authorised.
    sso_blocked: bool,
    /// Whether this is a GitHub App user token (`ghu_`), which can list installation repositories.
    app_user: bool,
}

#[derive(Default)]
struct World {
    next_id: u64,
    orgs: Vec<Org>,
    repos: HashMap<u64, Repo>,
    blobs: HashMap<String, String>,
    users: HashMap<String, User>,
    /// Installation token → installation ID.
    installation_tokens: HashMap<String, u64>,
    releases: HashMap<u64, Release>,
    assets: HashMap<u64, Asset>,
    crates_io: HashSet<String>,
    /// Whether crates.io answers with errors.
    crates_io_down: bool,
    /// Claims the fake Actions runtime puts in the OIDC tokens it issues.
    actions_claims: Value,
    /// How many of the next OIDC token requests fail with 504, as GitHub's endpoint sometimes does.
    actions_token_failures: u32,
    /// The user token the device flow hands out once approved.
    device_flow_token: Option<String>,
    /// Device code → polls so far.
    device_codes: HashMap<String, u32>,
    /// Refresh token → user token.
    refresh_tokens: HashMap<String, String>,
    web: Web,
    calls: Vec<String>,
    /// Answers to give instead of the real ones, to the next requests whose path contains the pattern.
    faults: Vec<Fault>,
    /// The rate-limit allowance every answer reports, as `(remaining, limit)`.
    allowance: Option<(u64, u64)>,
    /// Requests whose path contains the pattern take this long to answer, as large uploads do.
    delays: Vec<(String, std::time::Duration)>,
}

struct Fault {
    path_part: String,
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: String,
}

impl World {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

#[derive(Clone)]
pub struct FakeGitHub {
    world: Arc<Mutex<World>>,
    pub url: String,
}

fn blob_sha(content: &str) -> String {
    hex::encode(Sha256::digest(format!("blob {}\0{content}", content.len())))[..40].to_owned()
}

impl FakeGitHub {
    pub async fn start() -> Self {
        let world = Arc::new(Mutex::new(World {
            next_id: 1000,
            ..World::default()
        }));
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("bind the fake GitHub");
        let url = format!("http://{}", listener.local_addr().expect("local address"));
        let fake = Self { world, url };
        let app = router(fake.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        fake
    }

    fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().expect("fake GitHub lock")
    }

    /// Adds an organisation that has installed both Apps, with a storage repository containing `privatecrates.toml`.
    pub fn add_org(&self, login: &str, slug: &str) -> Org {
        let mut w = self.world();
        let id = w.id();
        let reader_installation = w.id();
        let storage_installation = w.id();
        let storage_repo = w.id();
        w.repos.insert(
            storage_repo,
            Repo {
                id: storage_repo,
                owner_id: id,
                owner: login.into(),
                name: "crates-store".into(),
                immutable_releases: true,
                ..Repo::default()
            },
        );
        let org = Org {
            id,
            login: login.into(),
            reader_installation,
            storage_installation,
            storage_repo,
            personal: false,
        };
        w.orgs.push(org.clone());
        drop(w);
        self.write_file(
            storage_repo,
            "privatecrates.toml",
            &format!("slug = \"{slug}\"\n"),
        );
        org
    }

    pub fn add_repo(&self, org: &Org, name: &str) -> u64 {
        let mut w = self.world();
        let id = w.id();
        w.repos.insert(
            id,
            Repo {
                id,
                owner_id: org.id,
                owner: org.login.clone(),
                name: name.into(),
                ..Repo::default()
            },
        );
        id
    }

    /// Renames a repository, keeping its ID, as GitHub does.
    pub fn rename_repo(&self, id: u64, name: &str) {
        self.world()
            .repos
            .get_mut(&id)
            .expect("known repository")
            .name = name.into();
    }

    /// Adds a user and returns a token of the given kind (`ghu_` for an App user token, `gho_` for OAuth).
    pub fn add_user(&self, login: &str, prefix: &str, repos: &[(u64, bool)]) -> String {
        let mut w = self.world();
        let id = w.id();
        let token = format!("{prefix}{login}{id}");
        w.users.insert(
            token.clone(),
            User {
                id,
                login: login.into(),
                repos: repos.iter().copied().collect(),
                roles: HashMap::new(),
                sso_blocked: false,
                app_user: prefix == "ghu_",
            },
        );
        token
    }

    /// Gives a user a role on a repository: `read`, `triage`, `write`, `maintain` or `admin`. Write and above can
    /// push.
    /// Every repository of `org`, the storage repository included.
    pub fn org_repos(&self, org: &Org) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .world()
            .repos
            .values()
            .filter(|r| r.owner_id == org.id)
            .map(|r| r.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn set_repo_role(&self, token: &str, repo: u64, role: &str) {
        let mut w = self.world();
        let user = w.users.get_mut(token).expect("known token");
        user.repos
            .insert(repo, matches!(role, "write" | "maintain" | "admin"));
        user.roles.insert(repo, role.into());
    }

    pub fn block_sso(&self, token: &str) {
        self.world()
            .users
            .get_mut(token)
            .expect("known token")
            .sso_blocked = true;
    }

    pub fn set_user_repos(&self, token: &str, repos: &[(u64, bool)]) {
        self.world()
            .users
            .get_mut(token)
            .expect("known token")
            .repos = repos.iter().copied().collect();
    }

    pub fn set_immutable_releases(&self, repo: u64, on: bool) {
        self.world()
            .repos
            .get_mut(&repo)
            .expect("known repo")
            .immutable_releases = on;
    }

    pub fn add_crates_io_crate(&self, name: &str) {
        self.world().crates_io.insert(name.to_ascii_lowercase());
    }

    /// Writes a file as test setup: a commit whose author is linked to no GitHub account.
    pub fn write_file(&self, repo: u64, path: &str, content: &str) {
        self.commit_file(repo, path, content, "test setup", None);
    }

    /// Writes a file as a person pushing a commit would, with `author` the GitHub account the commit links to.
    pub fn commit_file(
        &self,
        repo: u64,
        path: &str,
        content: &str,
        message: &str,
        author: Option<&str>,
    ) {
        let mut w = self.world();
        let sha = blob_sha(content);
        w.blobs.insert(sha.clone(), content.into());
        w.repos
            .get_mut(&repo)
            .expect("known repo")
            .files
            .insert(path.into(), sha);
        record_commit(
            &mut w,
            repo,
            path,
            message,
            None,
            author.map(str::to_owned),
            false,
        );
    }

    pub fn file(&self, repo: u64, path: &str) -> Option<String> {
        let w = self.world();
        let sha = w.repos.get(&repo)?.files.get(path)?;
        w.blobs.get(sha).cloned()
    }

    pub fn commits(&self, repo: u64) -> Vec<Commit> {
        self.world().repos[&repo].commits.clone()
    }

    /// The published releases of a repository: (tag, immutable, asset names).
    pub fn releases(&self, repo: u64) -> Vec<(String, bool, Vec<String>)> {
        let w = self.world();
        let mut out: Vec<_> = w
            .releases
            .values()
            .filter(|r| r.repo == repo && !r.draft)
            .map(|r| {
                (
                    r.tag.clone(),
                    r.immutable,
                    r.assets.iter().map(|a| w.assets[a].name.clone()).collect(),
                )
            })
            .collect();
        out.sort();
        out
    }

    pub fn asset(&self, repo: u64, tag: &str, name: &str) -> Option<Bytes> {
        let w = self.world();
        let release = w
            .releases
            .values()
            .find(|r| r.repo == repo && r.tag == tag && !r.draft)?;
        release
            .assets
            .iter()
            .map(|a| &w.assets[a])
            .find(|a| a.name == name)
            .map(|a| a.content.clone())
    }

    /// A published, immutable release created outside the service, as if left by a crash before the index append.
    pub fn add_release(&self, repo: u64, tag: &str, asset_name: &str, content: &[u8]) {
        let mut w = self.world();
        let release_id = w.id();
        let asset_id = w.id();
        w.assets.insert(
            asset_id,
            Asset {
                id: asset_id,
                name: asset_name.into(),
                content: Bytes::copy_from_slice(content),
            },
        );
        let immutable = w.repos[&repo].immutable_releases;
        if immutable {
            w.repos
                .get_mut(&repo)
                .expect("repo")
                .reserved_tags
                .insert(tag.into());
        }
        w.releases.insert(
            release_id,
            Release {
                id: release_id,
                repo,
                tag: tag.into(),
                draft: false,
                immutable,
                body: String::new(),
                assets: vec![asset_id],
            },
        );
    }

    /// Replaces a published asset's content, as tampering on GitHub's side would: its digest changes with it.
    pub fn replace_asset(&self, repo: u64, tag: &str, name: &str, content: &[u8]) {
        let mut w = self.world();
        let id = find_asset(&w, repo, tag, name).expect("known asset");
        w.assets.get_mut(&id).expect("asset").content = Bytes::copy_from_slice(content);
    }

    /// Removes an asset from a published release.
    pub fn remove_asset(&self, repo: u64, tag: &str, name: &str) {
        let mut w = self.world();
        let id = find_asset(&w, repo, tag, name).expect("known asset");
        w.assets.remove(&id);
        for release in w.releases.values_mut() {
            release.assets.retain(|a| *a != id);
        }
    }

    /// Makes a published release mutable, as it would be had immutable releases been off.
    /// Deletes a published release, as a repository admin can.
    pub fn delete_release(&self, repo: u64, tag: &str) {
        let mut w = self.world();
        w.releases.retain(|_, r| !(r.repo == repo && r.tag == tag));
    }

    pub fn make_release_mutable(&self, repo: u64, tag: &str) {
        let mut w = self.world();
        let release = w
            .releases
            .values_mut()
            .find(|r| r.repo == repo && r.tag == tag && !r.draft)
            .expect("known release");
        release.immutable = false;
    }

    /// The API calls made so far, as `METHOD path`.
    pub fn calls(&self) -> Vec<String> {
        self.world().calls.clone()
    }

    pub fn clear_calls(&self) {
        self.world().calls.clear();
    }

    /// Mints an Actions OIDC token, as GitHub would for a job.
    pub fn oidc_token(&self, audience: &str, claims: &Value) -> String {
        sign_oidc(&self.oidc_issuer(), audience, claims)
    }

    /// Standard claims for a workflow run in `repository`, started by pushing a tag. The actor is
    /// [`ACTIONS_ACTOR`], a user this gives push access to the repository.
    pub fn actions_claims(
        &self,
        org: &Org,
        repository: &str,
        repository_id: u64,
        workflow: &str,
    ) -> Value {
        let existing = self
            .world()
            .users
            .iter()
            .find(|(_, u)| u.login == ACTIONS_ACTOR)
            .map(|(token, _)| token.clone());
        let token = existing.unwrap_or_else(|| self.add_user(ACTIONS_ACTOR, "ghu_", &[]));
        let actor_id = {
            let mut w = self.world();
            let user = w.users.get_mut(&token).expect("the actor");
            user.repos.insert(repository_id, true);
            user.id
        };
        Self::actions_claims_for(
            org,
            repository,
            repository_id,
            workflow,
            (ACTIONS_ACTOR, actor_id),
            "push",
        )
    }

    /// Claims for a workflow run in `repository`, started by `actor` (login and user ID) with `event`.
    pub fn actions_claims_for(
        org: &Org,
        repository: &str,
        repository_id: u64,
        workflow: &str,
        (actor, actor_id): (&str, u64),
        event: &str,
    ) -> Value {
        json!({
            "repository": repository,
            "repository_id": repository_id.to_string(),
            "repository_owner": org.login,
            "repository_owner_id": org.id.to_string(),
            "job_workflow_ref": format!("{repository}/.github/workflows/{workflow}@refs/tags/v1"),
            "run_id": "42",
            "run_attempt": "1",
            "sha": "deadbeef",
            "ref": "refs/tags/v1",
            "ref_type": "tag",
            "sub": format!("repo:{repository}:ref:refs/tags/v1"),
            "actor": actor,
            "actor_id": actor_id.to_string(),
            "event_name": event,
        })
    }

    /// Sets the claims the fake Actions runtime puts in tokens requested at [`Self::actions_token_url`].
    pub fn set_actions_claims(&self, claims: Value) {
        self.world().actions_claims = claims;
    }

    /// Makes the device flow sign in as the user holding `token`.
    /// Makes the next `count` OIDC token requests from the fake Actions runtime fail with 504.
    /// The next request whose path contains `path_part` gets this answer instead, as GitHub's rate limits and
    /// outages answer. Queued faults for the same path are used in order.
    pub fn fail_next(&self, path_part: &str, status: u16, headers: &[(&str, &str)], body: &str) {
        self.world().faults.push(Fault {
            path_part: path_part.into(),
            status: StatusCode::from_u16(status).expect("a status"),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body: body.into(),
        });
    }

    /// Every request whose path contains `path_part` takes `delay` longer to answer, from now on.
    pub fn slow_down(&self, path_part: &str, delay: std::time::Duration) {
        self.world().delays.push((path_part.into(), delay));
    }

    /// Every answer from now on reports this much of the hourly allowance left, as GitHub's `x-ratelimit-*`
    /// headers do.
    pub fn set_allowance(&self, remaining: u64, limit: u64) {
        self.world().allowance = Some((remaining, limit));
    }

    pub fn fail_actions_tokens(&self, count: u32) {
        self.world().actions_token_failures = count;
    }

    pub fn set_device_flow_user(&self, token: &str) {
        self.world().device_flow_token = Some(token.into());
    }

    /// What a job sees as `ACTIONS_ID_TOKEN_REQUEST_URL`.
    pub fn actions_token_url(&self) -> String {
        format!("{}/actions/token?api-version=2.0", self.url)
    }

    pub fn oidc_issuer(&self) -> String {
        format!("{}/oidc", self.url)
    }

    pub fn oidc_jwks_url(&self) -> String {
        format!("{}/.well-known/jwks", self.url)
    }
}

/// The `X-Hub-Signature-256` header GitHub sends with a webhook `body`, for an App's webhook `secret`.
pub fn github_webhook_signature(secret: &[u8], body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Signs an Actions OIDC token with the fake's key, as GitHub would for a job.
pub fn sign_oidc(issuer: &str, audience: &str, claims: &Value) -> String {
    let now = now();
    let mut claims = claims.clone();
    let object = claims.as_object_mut().expect("claims are an object");
    object.insert("iss".into(), json!(issuer));
    object.insert("aud".into(), json!(audience));
    object.insert("iat".into(), json!(now));
    object.insert("exp".into(), json!(now + 300));
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(OIDC_KID.into());
    jsonwebtoken::encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(OIDC_KEY.pem.as_bytes()).expect("test key"),
    )
    .expect("sign OIDC token")
}

/// The fake's published OIDC signing keys, as a JWKS document.
pub fn oidc_jwks() -> Value {
    json!({
        "keys": [{ "kty": "RSA", "alg": "RS256", "use": "sig", "kid": OIDC_KID, "n": OIDC_KEY.modulus, "e": "AQAB" }]
    })
}

/// A token signed by a key GitHub never published, as a forger would make.
pub fn forge_oidc(issuer: &str, audience: &str, claims: &Value) -> String {
    let mut claims = claims.clone();
    let object = claims.as_object_mut().expect("claims are an object");
    object.insert("iss".into(), json!(issuer));
    object.insert("aud".into(), json!(audience));
    object.insert("exp".into(), json!(now() + 300));
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(OIDC_KID.into());
    jsonwebtoken::encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(app_private_key().as_bytes()).expect("test key"),
    )
    .expect("sign forged token")
}

fn find_asset(w: &World, repo: u64, tag: &str, name: &str) -> Option<u64> {
    let release = w
        .releases
        .values()
        .find(|r| r.repo == repo && r.tag == tag && !r.draft)?;
    release
        .assets
        .iter()
        .copied()
        .find(|a| w.assets[a].name == name)
}

/// Records a commit of one file. Commits are a second apart, so their order shows in their dates.
fn record_commit(
    w: &mut World,
    repo: u64,
    path: &str,
    message: &str,
    by_installation: Option<u64>,
    author: Option<String>,
    verified: bool,
) {
    let n = w.id();
    let commits = &mut w.repos.get_mut(&repo).expect("known repo").commits;
    let at = commits.last().map_or(now(), |c| c.at + 1);
    commits.push(Commit {
        sha: hex::encode(Sha256::digest(format!("{repo} {n} {path} {message}")))[..40].to_owned(),
        path: path.into(),
        message: message.into(),
        by_installation,
        author,
        verified,
        at,
    });
}

/// Unix seconds as GitHub writes dates: `2026-09-27T14:20:01Z`.
fn rfc3339(secs: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let time = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

// --- Website sign-in: the web flow, organisation membership and App installation per organisation ---

#[derive(Default)]
struct Web {
    /// Installations of organisations that have not installed that App (yet).
    uninstalled: HashSet<u64>,
    /// User ID → (organisation ID, role).
    memberships: HashMap<u64, Vec<(u64, String)>>,
    /// Web flow code → the user token it exchanges for.
    codes: HashMap<String, String>,
    /// Organisation ID → members beyond the users added with [`FakeGitHub::add_member`].
    other_members: HashMap<u64, u64>,
}

impl FakeGitHub {
    /// Adds an organisation that has installed neither App, with an empty repository meant for storage.
    pub fn add_org_without_apps(&self, login: &str) -> Org {
        let mut w = self.world();
        let org = Org {
            id: w.id(),
            login: login.into(),
            reader_installation: w.id(),
            storage_installation: w.id(),
            storage_repo: w.id(),
            personal: false,
        };
        w.repos.insert(
            org.storage_repo,
            Repo {
                id: org.storage_repo,
                owner_id: org.id,
                owner: login.into(),
                name: "crates-store".into(),
                immutable_releases: true,
                ..Repo::default()
            },
        );
        w.web.uninstalled.insert(org.reader_installation);
        w.web.uninstalled.insert(org.storage_installation);
        w.orgs.push(org.clone());
        org
    }

    /// The personal account of the user holding `token`, with a `crates-store` repository they own and push to.
    /// Neither App is installed yet (see [`Self::install_app`]).
    pub fn add_personal_account(&self, token: &str) -> Org {
        let mut w = self.world();
        let (id, login) = {
            let user = &w.users[token];
            (user.id, user.login.clone())
        };
        let org = Org {
            id,
            login: login.clone(),
            reader_installation: w.id(),
            storage_installation: w.id(),
            storage_repo: w.id(),
            personal: true,
        };
        w.repos.insert(
            org.storage_repo,
            Repo {
                id: org.storage_repo,
                owner_id: id,
                owner: login,
                name: "crates-store".into(),
                immutable_releases: true,
                ..Repo::default()
            },
        );
        w.users
            .get_mut(token)
            .expect("the user exists")
            .repos
            .insert(org.storage_repo, true);
        w.web.uninstalled.insert(org.reader_installation);
        w.web.uninstalled.insert(org.storage_installation);
        w.orgs.push(org.clone());
        org
    }

    pub fn install_app(&self, org: &Org, app_id: u64) {
        let installation = if app_id == READER_APP_ID {
            org.reader_installation
        } else {
            org.storage_installation
        };
        self.world().web.uninstalled.remove(&installation);
    }

    /// Makes the user holding `token` a member of `org` with `role` (`admin` or `member`).
    pub fn add_member(&self, token: &str, org: &Org, role: &str) {
        let mut w = self.world();
        let user = w.users[token].id;
        w.web
            .memberships
            .entry(user)
            .or_default()
            .push((org.id, role.into()));
    }

    /// Another token of the same kind for the same user, as when they sign in on a second device.
    pub fn another_token(&self, token: &str) -> String {
        let mut w = self.world();
        let user = w.users[token].clone();
        let prefix = if user.app_user { "ghu_" } else { "gho_" };
        let another = format!("{prefix}{}{}", user.login, w.id());
        w.users.insert(another.clone(), user);
        another
    }

    /// Adds members to `org` who have no token here, as people join the organisation.
    pub fn add_org_members(&self, org: &Org, count: u64) {
        *self.world().web.other_members.entry(org.id).or_default() += count;
    }

    /// Removes members added with [`Self::add_org_members`], as people leave the organisation.
    pub fn remove_org_members(&self, org: &Org, count: u64) {
        let mut w = self.world();
        let members = w.web.other_members.entry(org.id).or_default();
        *members = members
            .checked_sub(count)
            .expect("enough members to remove");
    }

    /// A code that the web flow exchanges for `token`, as GitHub hands to the callback after the user approves.
    pub fn web_flow_code(&self, token: &str) -> String {
        let mut w = self.world();
        let code = format!("code-{}", w.id());
        w.web.codes.insert(code.clone(), token.into());
        code
    }
}

fn web_routes() -> Router<FakeGitHub> {
    Router::new()
        .route("/user/orgs", get(user_orgs))
        .route("/user/memberships/orgs/{org}", get(user_membership))
        .route("/orgs/{org}/installation", get(org_installation))
        .route("/users/{login}/installation", get(user_installation))
        .route("/orgs/{org}/members", get(org_members))
}

/// An organisation's members, for the reader App's installation on it (which has Members read).
async fn org_members(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(org): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    let installation = match principal(&w, &headers) {
        Ok(Principal::Installation(i)) => i,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs an installation token"),
        Err(e) => return e,
    };
    let org = match installation_app(&w, installation) {
        Some((READER_APP_ID, o)) if o.login.eq_ignore_ascii_case(&org) => o.id,
        _ => {
            return error(
                StatusCode::FORBIDDEN,
                "Resource not accessible by integration",
            );
        }
    };
    let mut members: Vec<Value> = w
        .users
        .values()
        .filter(|u| {
            w.web
                .memberships
                .get(&u.id)
                .is_some_and(|m| m.iter().any(|(o, _)| *o == org))
        })
        .map(|u| json!({ "login": u.login, "id": u.id }))
        .collect();
    members.sort_by_key(|m| m["id"].as_u64());
    let others = w.web.other_members.get(&org).copied().unwrap_or_default();
    members.extend(
        (0..others).map(|i| json!({ "login": format!("member-{i}"), "id": 1_000_000 + i })),
    );
    Json(paginate(&members, &page)).into_response()
}

fn org_json(org: &Org) -> Value {
    json!({ "id": org.id, "login": org.login, "avatar_url": format!("https://avatars.githubusercontent.com/u/{}", org.id) })
}

fn user_memberships(
    w: &World,
    headers: &HeaderMap,
) -> Result<(User, Vec<(Org, String)>), Response> {
    let Principal::User(user) = principal(w, headers)? else {
        return Err(error(StatusCode::FORBIDDEN, "needs a user token"));
    };
    let memberships = w
        .web
        .memberships
        .get(&user.id)
        .into_iter()
        .flatten()
        .filter_map(|(org, role)| {
            let org = w.orgs.iter().find(|o| o.id == *org)?;
            Some((org.clone(), role.clone()))
        })
        .collect();
    Ok((user, memberships))
}

async fn user_orgs(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    match user_memberships(&w, &headers) {
        Ok((_, memberships)) => {
            let orgs: Vec<Value> = memberships.iter().map(|(org, _)| org_json(org)).collect();
            Json(paginate(&orgs, &page)).into_response()
        }
        Err(e) => e,
    }
}

async fn user_membership(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(org): Path<String>,
) -> Response {
    let w = fake.world();
    let (user, memberships) = match user_memberships(&w, &headers) {
        Ok(m) => m,
        Err(e) => return e,
    };
    match memberships
        .iter()
        .find(|(o, _)| o.login.eq_ignore_ascii_case(&org))
    {
        Some((org, role)) => Json(json!({
            "state": "active",
            "role": role,
            "organization": org_json(org),
            "user": { "login": user.login, "id": user.id },
        }))
        .into_response(),
        None => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

async fn org_installation(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(org): Path<String>,
) -> Response {
    account_installation(&fake, &headers, &org, false)
}

async fn user_installation(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(login): Path<String>,
) -> Response {
    account_installation(&fake, &headers, &login, true)
}

fn account_json(o: &Org) -> Value {
    json!({ "login": o.login, "id": o.id, "type": if o.personal { "User" } else { "Organization" } })
}

/// `/orgs/{org}/installation` finds organisations only, and `/users/{login}/installation` personal accounts.
fn account_installation(
    fake: &FakeGitHub,
    headers: &HeaderMap,
    org: &str,
    personal: bool,
) -> Response {
    let w = fake.world();
    let app = match principal(&w, headers) {
        Ok(Principal::App(app)) => app,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs an App JWT"),
        Err(e) => return e,
    };
    let installation = w
        .orgs
        .iter()
        .find(|o| o.personal == personal && o.login.eq_ignore_ascii_case(org))
        .map(|o| {
            let id = if app == READER_APP_ID {
                o.reader_installation
            } else {
                o.storage_installation
            };
            (id, o)
        });
    match installation {
        Some((id, o)) if !w.web.uninstalled.contains(&id) => {
            Json(json!({ "id": id, "account": account_json(o) })).into_response()
        }
        _ => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

/// The web flow's code exchange: `POST /login/oauth/access_token` with the App's client secret and a code.
fn web_flow_token(w: &mut World, form: &AccessTokenForm, code: &str) -> Response {
    if form.client_secret.as_deref() != Some(READER_CLIENT_SECRET) {
        return Json(json!({ "error": "incorrect_client_credentials" })).into_response();
    }
    match w.web.codes.remove(code) {
        Some(token) => Json(json!({
            "access_token": token,
            "expires_in": 28800,
            "refresh_token": format!("ghr_{}", w.id()),
            "token_type": "bearer",
        }))
        .into_response(),
        None => Json(json!({ "error": "bad_verification_code" })).into_response(),
    }
}

/// `DELETE /applications/{client_id}/grant`, with the App's client credentials: revokes every reader App token of
/// the user behind `access_token`, and their refresh tokens. Other users' tokens, and the user's other kinds of
/// token, keep working.
async fn revoke_grant(
    State(fake): State<FakeGitHub>,
    Path(client_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("{READER_CLIENT_ID}:{READER_CLIENT_SECRET}"))
    );
    if client_id != READER_CLIENT_ID
        || headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            != Some(&expected)
    {
        return error(StatusCode::UNAUTHORIZED, "Bad credentials");
    }
    let mut w = fake.world();
    w.calls
        .push(format!("DELETE /applications/{client_id}/grant"));
    let Some(user_id) = body["access_token"]
        .as_str()
        .and_then(|t| w.users.get(t))
        .filter(|u| u.app_user)
        .map(|u| u.id)
    else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    w.users.retain(|_, u| !(u.app_user && u.id == user_id));
    let users = &w.users;
    let live: HashSet<String> = users.keys().cloned().collect();
    w.refresh_tokens.retain(|_, token| live.contains(token));
    StatusCode::NO_CONTENT.into_response()
}

// --- The HTTP side ---

enum Principal {
    App(u64),
    Installation(u64),
    User(User),
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}

fn principal(w: &World, headers: &HeaderMap) -> Result<Principal, Response> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "Requires authentication"))?;
    if let Some(&installation) = w.installation_tokens.get(token) {
        return Ok(Principal::Installation(installation));
    }
    if let Some(user) = w.users.get(token) {
        if user.sso_blocked {
            let mut response = error(
                StatusCode::FORBIDDEN,
                "Resource protected by organization SAML enforcement.",
            );
            response.headers_mut().insert(
                "x-github-sso",
                "required; url=https://github.com/orgs/acme/sso?authorization_request=abc"
                    .parse()
                    .expect("header"),
            );
            return Err(response);
        }
        return Ok(Principal::User(user.clone()));
    }
    if token.starts_with("eyJ") {
        // An App JWT. The fake trusts the `iss` claim without checking the signature.
        let payload = token.split('.').nth(1).unwrap_or_default();
        let claims: Value = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if let Some(app) = claims["iss"].as_str().and_then(|s| s.parse().ok()) {
            return Ok(Principal::App(app));
        }
    }
    Err(error(StatusCode::UNAUTHORIZED, "Bad credentials"))
}

fn installation_app(w: &World, installation: u64) -> Option<(u64, &Org)> {
    w.orgs.iter().find_map(|o| {
        if o.reader_installation == installation {
            Some((READER_APP_ID, o))
        } else if o.storage_installation == installation {
            Some((STORAGE_APP_ID, o))
        } else {
            None
        }
    })
}

/// The repositories an installation covers: the storage App only its storage repository; the reader App all.
fn installation_repos(w: &World, installation: u64) -> Vec<u64> {
    let Some((app, org)) = installation_app(w, installation) else {
        return Vec::new();
    };
    if app == STORAGE_APP_ID {
        return vec![org.storage_repo];
    }
    let mut ids: Vec<u64> = w
        .repos
        .values()
        .filter(|r| r.owner_id == org.id)
        .map(|r| r.id)
        .collect();
    ids.sort_unstable();
    ids
}

fn repo_json(repo: &Repo, permissions: Option<(bool, bool)>) -> Value {
    let mut value = json!({
        "id": repo.id,
        "name": repo.name,
        "full_name": format!("{}/{}", repo.owner, repo.name),
        "owner": { "login": repo.owner, "id": repo.owner_id },
        "default_branch": "main",
    });
    if let Some((pull, push)) = permissions {
        value["permissions"] = json!({ "pull": pull, "push": push, "admin": false });
    }
    value
}

fn find_repo(w: &World, owner: &str, name: &str) -> Option<u64> {
    w.repos
        .values()
        .find(|r| r.owner == owner && r.name == name)
        .map(|r| r.id)
}

/// Checks that an installation token can use a repository.
fn storage_access(
    w: &World,
    headers: &HeaderMap,
    owner: &str,
    name: &str,
) -> Result<(u64, u64), Response> {
    let repo =
        find_repo(w, owner, name).ok_or_else(|| error(StatusCode::NOT_FOUND, "Not Found"))?;
    match principal(w, headers)? {
        Principal::Installation(i) if installation_repos(w, i).contains(&repo) => Ok((repo, i)),
        _ => Err(error(StatusCode::NOT_FOUND, "Not Found")),
    }
}

fn release_json(w: &World, fake_url: &str, r: &Release) -> Value {
    let repo = &w.repos[&r.repo];
    json!({
        "id": r.id,
        "tag_name": r.tag,
        "draft": r.draft,
        "immutable": r.immutable,
        "body": r.body,
        "upload_url": format!("{fake_url}/uploads/repos/{}/{}/releases/{}/assets{{?name,label}}", repo.owner, repo.name, r.id),
        "assets": r.assets.iter().map(|a| asset_json(&w.assets[a])).collect::<Vec<_>>(),
    })
}

fn asset_json(a: &Asset) -> Value {
    json!({
        "id": a.id,
        "name": a.name,
        "size": a.content.len(),
        "digest": format!("sha256:{}", hex::encode(Sha256::digest(&a.content))),
    })
}

#[derive(Deserialize)]
struct Page {
    page: Option<usize>,
    per_page: Option<usize>,
}

fn paginate<T: Clone>(items: &[T], page: &Page) -> Vec<T> {
    let per_page = page.per_page.unwrap_or(30);
    let start = (page.page.unwrap_or(1) - 1) * per_page;
    items.iter().skip(start).take(per_page).cloned().collect()
}

fn router(fake: FakeGitHub) -> Router {
    Router::new()
        .route("/app/installations", get(app_installations))
        .route("/app/installations/{id}/access_tokens", post(access_token))
        .route("/installation/repositories", get(installation_repositories))
        .route("/user", get(user))
        .route(
            "/user/installations/{id}/repositories",
            get(user_installation_repositories),
        )
        .route("/repositories/{id}", get(repository_by_id))
        .route("/repos/{owner}/{repo}", get(repository_by_name))
        .route("/repos/{owner}/{repo}/git/trees/{branch}", get(tree))
        .route("/repos/{owner}/{repo}/git/blobs/{sha}", get(blob))
        .route("/repos/{owner}/{repo}/contents/{*path}", put(put_contents))
        .route("/repos/{owner}/{repo}/commits", get(list_commits))
        .route(
            "/repos/{owner}/{repo}/collaborators/{username}/permission",
            get(collaborator_permission),
        )
        .route(
            "/repos/{owner}/{repo}/releases",
            get(list_releases).post(create_release),
        )
        .route(
            "/repos/{owner}/{repo}/releases/tags/{tag}",
            get(release_by_tag),
        )
        .route(
            "/repos/{owner}/{repo}/releases/{id}",
            patch(update_release).delete(delete_release),
        )
        .route(
            "/repos/{owner}/{repo}/releases/assets/{id}",
            get(download_asset),
        )
        .route(
            "/uploads/repos/{owner}/{repo}/releases/{id}/assets",
            post(upload_asset),
        )
        .route("/signed/{id}", get(signed_download))
        .route("/.well-known/jwks", get(jwks))
        .route("/actions/token", get(actions_token))
        .route("/api/v1/crates", get(crates_io_search))
        .route("/api/v1/crates/{name}", get(crates_io_crate))
        .route("/login/device/code", post(device_code))
        .route("/login/oauth/access_token", post(oauth_access_token))
        .route("/applications/{client_id}/grant", delete(revoke_grant))
        .merge(web_routes())
        .layer(axum::middleware::from_fn_with_state(
            fake.clone(),
            record_call,
        ))
        .with_state(fake)
}

async fn record_call(
    State(fake): State<FakeGitHub>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let call = format!("{} {}", request.method(), request.uri().path());
    let (fault, allowance, delay) = {
        let mut w = fake.world();
        w.calls.push(call);
        let path = request.uri().path();
        let delay = w
            .delays
            .iter()
            .filter(|(part, _)| path.contains(part.as_str()))
            .map(|(_, d)| *d)
            .sum::<std::time::Duration>();
        let fault = w
            .faults
            .iter()
            .position(|f| path.contains(&f.path_part))
            .map(|i| w.faults.remove(i));
        (fault, w.allowance, delay)
    };
    tokio::time::sleep(delay).await;
    let mut response = match fault {
        Some(fault) => {
            let mut response = (fault.status, fault.body).into_response();
            for (name, value) in fault.headers {
                response.headers_mut().insert(
                    header::HeaderName::try_from(name).expect("a header name"),
                    value.parse().expect("a header value"),
                );
            }
            response
        }
        None => next.run(request).await,
    };
    if let Some((remaining, limit)) = allowance {
        let headers = response.headers_mut();
        headers
            .entry("x-ratelimit-remaining")
            .or_insert(remaining.into());
        headers.entry("x-ratelimit-limit").or_insert(limit.into());
        headers
            .entry("x-ratelimit-reset")
            .or_insert((now() + 3600).into());
    }
    response
}

async fn app_installations(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    let app = match principal(&w, &headers) {
        Ok(Principal::App(app)) => app,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs an App JWT"),
        Err(e) => return e,
    };
    let all: Vec<Value> = w
        .orgs
        .iter()
        .map(|o| {
            let id = if app == READER_APP_ID {
                o.reader_installation
            } else {
                o.storage_installation
            };
            (id, o)
        })
        .filter(|(id, _)| !w.web.uninstalled.contains(id))
        .map(|(id, o)| json!({ "id": id, "account": account_json(o) }))
        .collect();
    Json(paginate(&all, &page)).into_response()
}

async fn access_token(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> Response {
    let mut w = fake.world();
    let app = match principal(&w, &headers) {
        Ok(Principal::App(app)) => app,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs an App JWT"),
        Err(e) => return e,
    };
    if installation_app(&w, id).map(|(a, _)| a) != Some(app) {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    let token = format!("ghs_installation_{id}_{}", w.id());
    w.installation_tokens.insert(token.clone(), id);
    (
        StatusCode::CREATED,
        Json(json!({ "token": token, "expires_at": "2099-01-01T00:00:00Z" })),
    )
        .into_response()
}

async fn installation_repositories(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    let installation = match principal(&w, &headers) {
        Ok(Principal::Installation(i)) => i,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs an installation token"),
        Err(e) => return e,
    };
    let repos: Vec<Value> = installation_repos(&w, installation)
        .iter()
        .map(|id| repo_json(&w.repos[id], None))
        .collect();
    Json(json!({ "total_count": repos.len(), "repositories": paginate(&repos, &page) }))
        .into_response()
}

async fn user(State(fake): State<FakeGitHub>, headers: HeaderMap) -> Response {
    let w = fake.world();
    match principal(&w, &headers) {
        Ok(Principal::User(u)) => Json(json!({ "id": u.id, "login": u.login })).into_response(),
        Ok(_) => error(StatusCode::FORBIDDEN, "needs a user token"),
        Err(e) => e,
    }
}

async fn user_installation_repositories(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    let user = match principal(&w, &headers) {
        Ok(Principal::User(u)) if u.app_user => u,
        Ok(_) => return error(StatusCode::FORBIDDEN, "needs a GitHub App user token"),
        Err(e) => return e,
    };
    let repos: Vec<Value> = installation_repos(&w, id)
        .iter()
        .filter_map(|r| {
            user.repos
                .get(r)
                .map(|push| repo_json(&w.repos[r], Some((true, *push))))
        })
        .collect();
    if repos.is_empty() {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    Json(json!({ "total_count": repos.len(), "repositories": paginate(&repos, &page) }))
        .into_response()
}

async fn repository_by_name(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
) -> Response {
    let id = find_repo(&fake.world(), &owner, &name);
    match id {
        Some(id) => repository_by_id(State(fake), headers, Path(id)).await,
        None => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

async fn repository_by_id(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> Response {
    let w = fake.world();
    let Some(repo) = w.repos.get(&id) else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    match principal(&w, &headers) {
        Ok(Principal::User(u)) => match u.repos.get(&id) {
            Some(push) => Json(repo_json(repo, Some((true, *push)))).into_response(),
            None => error(StatusCode::NOT_FOUND, "Not Found"),
        },
        Ok(Principal::Installation(i)) if installation_repos(&w, i).contains(&id) => {
            Json(repo_json(repo, None)).into_response()
        }
        Ok(_) => error(StatusCode::NOT_FOUND, "Not Found"),
        Err(e) => e,
    }
}

/// A user's permission on a repository, for an installation token that covers it (metadata read is enough). As
/// GitHub does, `permission` is the base role: `maintain` is `write` and `triage` is `read`. Someone with no access
/// is not a collaborator: 404.
async fn collaborator_permission(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, username)): Path<(String, String, String)>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(found) => found,
        Err(e) => return e,
    };
    let Some(user) = w.users.values().find(|u| u.login == username) else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    let Some(&push) = user.repos.get(&repo) else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    let role = user
        .roles
        .get(&repo)
        .cloned()
        .unwrap_or_else(|| if push { "write" } else { "read" }.into());
    let permission = match role.as_str() {
        "admin" => "admin",
        "maintain" | "write" => "write",
        _ => "read",
    };
    Json(json!({
        "permission": permission,
        "role_name": role,
        "user": { "login": user.login, "id": user.id },
    }))
    .into_response()
}

async fn tree(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, _branch)): Path<(String, String, String)>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let files = &w.repos[&repo].files;
    // Like GitHub, a repository with no commits has no tree.
    if files.is_empty() {
        return error(StatusCode::CONFLICT, "Git Repository is empty.");
    }
    let etag = format!("\"{}\"", hex::encode(Sha256::digest(format!("{files:?}"))));
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|v| v.as_bytes() == etag.as_bytes())
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let tree: Vec<Value> = files
        .iter()
        .map(|(path, sha)| json!({ "path": path, "type": "blob", "sha": sha }))
        .collect();
    (
        [(header::ETAG, etag)],
        Json(json!({ "tree": tree, "truncated": false })),
    )
        .into_response()
}

async fn blob(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, sha)): Path<(String, String, String)>,
) -> Response {
    let w = fake.world();
    if let Err(e) = storage_access(&w, &headers, &owner, &name) {
        return e;
    }
    match w.blobs.get(&sha) {
        Some(content) => Json(json!({
            "sha": sha,
            "encoding": "base64",
            "content": base64::engine::general_purpose::STANDARD.encode(content),
        }))
        .into_response(),
        None => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

#[derive(Deserialize)]
struct ContentsBody {
    message: String,
    content: String,
    sha: Option<String>,
}

async fn put_contents(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, path)): Path<(String, String, String)>,
    Json(body): Json<ContentsBody>,
) -> Response {
    let mut w = fake.world();
    let (repo, installation) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let current = w.repos[&repo].files.get(&path).cloned();
    if current != body.sha {
        return error(StatusCode::CONFLICT, "sha does not match");
    }
    let Ok(content) = base64::engine::general_purpose::STANDARD.decode(&body.content) else {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "bad base64");
    };
    let content = String::from_utf8(content).expect("text files in tests");
    let sha = blob_sha(&content);
    w.blobs.insert(sha.clone(), content);
    w.repos
        .get_mut(&repo)
        .expect("repo")
        .files
        .insert(path.clone(), sha.clone());
    let bot = match installation_app(&w, installation) {
        Some((READER_APP_ID, _)) => READER_APP_LOGIN,
        _ => STORAGE_APP_LOGIN,
    };
    record_commit(
        &mut w,
        repo,
        &path,
        &body.message,
        Some(installation),
        Some(bot.into()),
        true,
    );
    (StatusCode::OK, Json(json!({ "content": { "sha": sha } }))).into_response()
}

#[derive(Deserialize)]
struct CommitsQuery {
    path: Option<String>,
    page: Option<usize>,
    per_page: Option<usize>,
}

/// A branch's commits, newest first; with `path`, those that changed that file or a file under that directory.
async fn list_commits(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(query): Query<CommitsQuery>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let commits = &w.repos[&repo].commits;
    if commits.is_empty() {
        return error(StatusCode::CONFLICT, "Git Repository is empty.");
    }
    let touches = |c: &Commit| match &query.path {
        None => true,
        Some(path) => c.path == *path || c.path.starts_with(&format!("{path}/")),
    };
    let all: Vec<Value> = commits
        .iter()
        .rev()
        .filter(|c| touches(c))
        .map(|c| {
            let date = rfc3339(c.at);
            json!({
                "sha": c.sha,
                "commit": {
                    "message": c.message,
                    "author": { "name": c.author.as_deref().unwrap_or("A Developer"), "date": date },
                    "committer": { "name": "GitHub", "date": date },
                    "verification": { "verified": c.verified },
                },
                "author": c.author.as_ref().map(|login| json!({ "login": login, "id": 1 })),
            })
        })
        .collect();
    let page = Page {
        page: query.page,
        per_page: query.per_page,
    };
    Json(paginate(&all, &page)).into_response()
}

async fn list_releases(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Query(page): Query<Page>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut releases: Vec<&Release> = w.releases.values().filter(|r| r.repo == repo).collect();
    releases.sort_by_key(|r| std::cmp::Reverse(r.id));
    let all: Vec<Value> = releases
        .iter()
        .map(|r| release_json(&w, &fake.url, r))
        .collect();
    Json(paginate(&all, &page)).into_response()
}

#[derive(Deserialize)]
struct CreateRelease {
    tag_name: String,
    body: String,
    draft: bool,
}

async fn create_release(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Json(body): Json<CreateRelease>,
) -> Response {
    let mut w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let taken = w.repos[&repo].reserved_tags.contains(&body.tag_name)
        || w.releases
            .values()
            .any(|r| r.repo == repo && !r.draft && r.tag == body.tag_name);
    if taken {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "tag_name already_exists");
    }
    let id = w.id();
    let release = Release {
        id,
        repo,
        tag: body.tag_name,
        draft: body.draft,
        immutable: false,
        body: body.body,
        assets: Vec::new(),
    };
    w.releases.insert(id, release.clone());
    (
        StatusCode::CREATED,
        Json(release_json(&w, &fake.url, &release)),
    )
        .into_response()
}

async fn release_by_tag(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, tag)): Path<(String, String, String)>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    match w
        .releases
        .values()
        .find(|r| r.repo == repo && r.tag == tag && !r.draft)
    {
        Some(r) => Json(release_json(&w, &fake.url, r)).into_response(),
        None => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

#[derive(Deserialize)]
struct UpdateRelease {
    draft: bool,
}

async fn update_release(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, id)): Path<(String, String, u64)>,
    Json(body): Json<UpdateRelease>,
) -> Response {
    let mut w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let immutable_repo = w.repos[&repo].immutable_releases;
    let Some(release) = w.releases.get_mut(&id).filter(|r| r.repo == repo) else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    if release.immutable {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "immutable release");
    }
    if release.draft && !body.draft {
        release.draft = false;
        release.immutable = immutable_repo;
        let tag = release.tag.clone();
        if immutable_repo {
            w.repos
                .get_mut(&repo)
                .expect("repo")
                .reserved_tags
                .insert(tag);
        }
    }
    let release = w.releases[&id].clone();
    Json(release_json(&w, &fake.url, &release)).into_response()
}

async fn delete_release(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, id)): Path<(String, String, u64)>,
) -> Response {
    let mut w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    if w.releases.get(&id).is_none_or(|r| r.repo != repo) {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    w.releases.remove(&id);
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}

async fn upload_asset(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, id)): Path<(String, String, u64)>,
    Query(query): Query<UploadQuery>,
    body: Bytes,
) -> Response {
    let mut w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let Some(release) = w.releases.get(&id).filter(|r| r.repo == repo) else {
        return error(StatusCode::NOT_FOUND, "Not Found");
    };
    if release.immutable {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot modify an immutable release",
        );
    }
    let asset_id = w.id();
    let asset = Asset {
        id: asset_id,
        name: query.name,
        content: body,
    };
    let json = asset_json(&asset);
    w.assets.insert(asset_id, asset);
    w.releases
        .get_mut(&id)
        .expect("release")
        .assets
        .push(asset_id);
    (StatusCode::CREATED, Json(json)).into_response()
}

async fn download_asset(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Path((owner, name, id)): Path<(String, String, u64)>,
) -> Response {
    let w = fake.world();
    let (repo, _) = match storage_access(&w, &headers, &owner, &name) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let owned = w
        .releases
        .values()
        .any(|r| r.repo == repo && r.assets.contains(&id));
    if !owned
        || headers
            .get(header::ACCEPT)
            .is_none_or(|v| v != "application/octet-stream")
    {
        return error(StatusCode::NOT_FOUND, "Not Found");
    }
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            format!("{}/signed/{id}?sig=fake", fake.url),
        )],
    )
        .into_response()
}

async fn signed_download(
    State(fake): State<FakeGitHub>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Response {
    // Like GitHub's CDN: the signature authorises the download, and credentials must not be forwarded here.
    if headers.contains_key(header::AUTHORIZATION) {
        return error(
            StatusCode::BAD_REQUEST,
            "unexpected Authorization header on a signed URL",
        );
    }
    let w = fake.world();
    match w.assets.get(&id) {
        Some(a) => a.content.clone().into_response(),
        None => error(StatusCode::NOT_FOUND, "Not Found"),
    }
}

async fn jwks() -> Json<Value> {
    Json(oidc_jwks())
}

#[derive(Deserialize)]
struct ActionsTokenQuery {
    audience: String,
}

async fn actions_token(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Query(query): Query<ActionsTokenQuery>,
) -> Response {
    let authorised = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case(&format!("bearer {ACTIONS_REQUEST_TOKEN}")));
    if !authorised {
        return error(StatusCode::UNAUTHORIZED, "bad request token");
    }
    let claims = {
        let mut w = fake.world();
        if w.actions_token_failures > 0 {
            w.actions_token_failures -= 1;
            return error(StatusCode::GATEWAY_TIMEOUT, "Gateway Timeout");
        }
        w.actions_claims.clone()
    };
    Json(json!({ "value": fake.oidc_token(&query.audience, &claims) })).into_response()
}

async fn crates_io_crate(State(fake): State<FakeGitHub>, Path(name): Path<String>) -> Response {
    let w = fake.world();
    if w.crates_io_down {
        return error(StatusCode::SERVICE_UNAVAILABLE, "crates.io is down");
    }
    if w.crates_io.contains(&name.to_ascii_lowercase()) {
        Json(json!({ "crate": { "name": name } })).into_response()
    } else {
        error(StatusCode::NOT_FOUND, "Not Found")
    }
}

#[derive(Deserialize)]
struct DeviceCodeForm {
    client_id: String,
}

async fn device_code(
    State(fake): State<FakeGitHub>,
    axum::Form(form): axum::Form<DeviceCodeForm>,
) -> Response {
    if form.client_id != READER_CLIENT_ID {
        return error(StatusCode::NOT_FOUND, "unknown client");
    }
    let mut w = fake.world();
    let code = format!("dc-{}", w.id());
    w.device_codes.insert(code.clone(), 0);
    Json(json!({
        "device_code": code,
        "user_code": "WDJB-MJHT",
        "verification_uri": format!("{}/login/device", fake.url),
        "expires_in": 900,
        "interval": 0,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct AccessTokenForm {
    client_id: String,
    #[serde(default)]
    grant_type: String,
    client_secret: Option<String>,
    code: Option<String>,
    device_code: Option<String>,
    refresh_token: Option<String>,
}

async fn oauth_access_token(
    State(fake): State<FakeGitHub>,
    axum::Form(form): axum::Form<AccessTokenForm>,
) -> Response {
    let mut w = fake.world();
    if form.client_id != READER_CLIENT_ID {
        return Json(json!({ "error": "incorrect_client_credentials" })).into_response();
    }
    if let Some(code) = &form.code {
        return web_flow_token(&mut w, &form, code);
    }
    let user_token = match form.grant_type.as_str() {
        "urn:ietf:params:oauth:grant-type:device_code" => {
            let Some(polls) = form
                .device_code
                .as_ref()
                .and_then(|c| w.device_codes.get_mut(c))
            else {
                return Json(json!({ "error": "incorrect_device_code" })).into_response();
            };
            *polls += 1;
            // The user approves in their browser after the first poll.
            if *polls == 1 {
                return Json(json!({ "error": "authorization_pending" })).into_response();
            }
            let Some(token) = w.device_flow_token.clone() else {
                return Json(json!({ "error": "access_denied" })).into_response();
            };
            token
        }
        "refresh_token" => {
            match form
                .refresh_token
                .as_ref()
                .and_then(|r| w.refresh_tokens.remove(r))
            {
                Some(token) => token,
                None => return Json(json!({ "error": "bad_refresh_token" })).into_response(),
            }
        }
        _ => return Json(json!({ "error": "unsupported_grant_type" })).into_response(),
    };
    let refresh = format!("ghr_{}", w.id());
    w.refresh_tokens.insert(refresh.clone(), user_token.clone());
    Json(json!({
        "access_token": user_token,
        "expires_in": 28800,
        "refresh_token": refresh,
        "refresh_token_expires_in": 15_897_600,
        "token_type": "bearer",
    }))
    .into_response()
}

// --- Organisation roles and deleted repositories ---

impl FakeGitHub {
    /// The GitHub user ID behind a token.
    pub fn user_id(&self, token: &str) -> u64 {
        self.world().users.get(token).expect("known token").id
    }

    /// Deletes a repository, as its administrator could on GitHub.
    pub fn delete_repo(&self, repo: u64) {
        self.world().repos.remove(&repo);
    }
}

// --- crates.io search ---

impl FakeGitHub {
    /// Makes every crates.io request fail, or work again.
    pub fn set_crates_io_down(&self, down: bool) {
        self.world().crates_io_down = down;
    }
}

#[derive(Deserialize)]
struct CratesIoSearchQuery {
    #[serde(default)]
    q: String,
    per_page: Option<usize>,
}

/// crates.io's search, over the crates added with [`FakeGitHub::add_crates_io_crate`] whose names contain the
/// query.
async fn crates_io_search(
    State(fake): State<FakeGitHub>,
    headers: HeaderMap,
    Query(query): Query<CratesIoSearchQuery>,
) -> Response {
    // crates.io refuses clients that do not identify themselves.
    if !headers.contains_key(header::USER_AGENT) {
        return error(StatusCode::FORBIDDEN, "a User-Agent is required");
    }
    let w = fake.world();
    if w.crates_io_down {
        return error(StatusCode::SERVICE_UNAVAILABLE, "crates.io is down");
    }
    let q = query.q.to_ascii_lowercase().replace('-', "_");
    let mut names: Vec<&String> = w
        .crates_io
        .iter()
        .filter(|n| n.replace('-', "_").contains(&q))
        .collect();
    names.sort();
    let crates: Vec<Value> = names
        .iter()
        .take(query.per_page.unwrap_or(10))
        .map(|n| {
            json!({ "name": n, "max_version": "1.0.0", "description": format!("{n} from crates.io") })
        })
        .collect();
    Json(json!({ "crates": crates, "meta": { "total": names.len() } })).into_response()
}
