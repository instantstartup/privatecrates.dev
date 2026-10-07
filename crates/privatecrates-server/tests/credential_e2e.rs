//! Real `cargo` with this credential provider against a real server and a fake GitHub.

mod common;

use std::{path::Path, process::Output};

use base64::Engine as _;
use common::Harness;
use privatecrates_testkit::{ACTIONS_REQUEST_TOKEN, FakeGitHub};
use serde_json::Value;

static PROVIDER: std::sync::LazyLock<std::path::PathBuf> =
    std::sync::LazyLock::new(|| common::client_binary("cargo-credential-privatecrates"));

/// Where cargo runs: a simulated GitHub Actions job, or a developer's machine.
enum Env<'a> {
    Actions {
        fake: &'a FakeGitHub,
    },
    ActionsWithoutIdToken,
    Developer {
        config: &'a Path,
    },
    /// A developer's machine where Cargo runs with no terminal, as inside an editor.
    Editor {
        config: &'a Path,
    },
}

async fn cargo(home: &Path, dir: &Path, env: Env<'_>, args: &[&str]) -> Output {
    let mut command = tokio::process::Command::new(env!("CARGO"));
    command
        .args(args)
        .current_dir(dir)
        .env("CARGO_HOME", home)
        .env("CARGO_TERM_COLOR", "never")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("CARGO_REGISTRIES_ACME_TOKEN")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
        .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
    match env {
        Env::Actions { fake } => {
            command
                .env("GITHUB_ACTIONS", "true")
                .env("CI", "true")
                .env("ACTIONS_ID_TOKEN_REQUEST_URL", fake.actions_token_url())
                .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", ACTIONS_REQUEST_TOKEN);
        }
        Env::ActionsWithoutIdToken => {
            command.env("GITHUB_ACTIONS", "true").env("CI", "true");
        }
        Env::Developer { config } => {
            // The test's pipes are not a terminal; a person at one would see the sign-in prompt.
            command
                .env("PRIVATECRATES_CREDENTIAL_STORE", "file")
                .env("PRIVATECRATES_CONFIG_DIR", config)
                .env("PRIVATECRATES_TRUST_REGISTRY_GITHUB_URL", "1")
                .env("PRIVATECRATES_INTERACTIVE", "1");
        }
        Env::Editor { config } => {
            command
                .env("PRIVATECRATES_CREDENTIAL_STORE", "file")
                .env("PRIVATECRATES_CONFIG_DIR", config)
                .env("PRIVATECRATES_TRUST_REGISTRY_GITHUB_URL", "1")
                .env_remove("PRIVATECRATES_INTERACTIVE");
        }
    }
    command.output().await.unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Writes a project whose `.cargo/config.toml` uses this credential provider.
fn project(h: &Harness, dir: &Path, manifest: &str) {
    std::fs::create_dir_all(dir.join(".cargo")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join(".cargo/config.toml"),
        format!(
            "[registries.acme]\nindex = \"sparse+{}/index/\"\ncredential-provider = [{:?}]\n",
            h.base(),
            PROVIDER.to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "").unwrap();
}

fn package(name: &str, version: &str, repository: &str, deps: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\nlicense = \"MIT\"\n\
         description = \"test\"\nrepository = \"https://github.com/{repository}\"\n\n[dependencies]\n{deps}"
    )
}

fn jwt_claims(jwt: &[u8]) -> Value {
    let jwt = std::str::from_utf8(jwt).unwrap();
    let payload = jwt.split('.').nth(1).unwrap();
    serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn publish_and_read_in_github_actions() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let site = h.repo("website");
    let home = tempfile::tempdir().unwrap();

    // The publish job runs in the crate's own repository.
    h.fake.set_actions_claims(h.fake.actions_claims(
        &h.org,
        "acme/story-engine",
        repo,
        "release.yml",
    ));
    let krate = tempfile::tempdir().unwrap();
    project(
        &h,
        krate.path(),
        &package("story_engine", "0.1.0", "acme/story-engine", ""),
    );
    let output = cargo(
        home.path(),
        krate.path(),
        Env::Actions { fake: &h.fake },
        &[
            "publish",
            "--registry",
            "acme",
            "--allow-dirty",
            "--no-verify",
        ],
    )
    .await;
    assert!(output.status.success(), "{}", stderr(&output));

    // The provenance is GitHub's token, naming exactly the bytes in the index.
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    let line: Value = serde_json::from_str(index.trim()).unwrap();
    let provenance = h
        .fake
        .asset(
            h.org.storage_repo,
            "story_engine-0.1.0",
            "story_engine-0.1.0.provenance.jwt",
        )
        .unwrap();
    let claims = jwt_claims(&provenance);
    assert_eq!(
        claims["aud"],
        format!(
            "{}/publish/story_engine/0.1.0/{}",
            h.base(),
            line["cksum"].as_str().unwrap()
        )
    );
    assert_eq!(
        claims["job_workflow_ref"],
        "acme/story-engine/.github/workflows/release.yml@refs/tags/v1"
    );
    let crate_bytes = h
        .fake
        .asset(
            h.org.storage_repo,
            "story_engine-0.1.0",
            "story_engine-0.1.0.crate",
        )
        .unwrap();
    assert_eq!(
        privatecrates_common::sha256_hex(&crate_bytes),
        line["cksum"].as_str().unwrap()
    );

    // A build job in another repository reads it, through the OIDC exchange.
    h.fake.set_actions_claims(
        h.fake
            .actions_claims(&h.org, "acme/website", site, "ci.yml"),
    );
    let consumer = tempfile::tempdir().unwrap();
    project(
        &h,
        consumer.path(),
        &package(
            "website",
            "0.1.0",
            "acme/website",
            "story_engine = { version = \"0.1\", registry = \"acme\" }\n",
        ),
    );
    for args in [&["generate-lockfile"][..], &["fetch"][..]] {
        let output = cargo(
            home.path(),
            consumer.path(),
            Env::Actions { fake: &h.fake },
            args,
        )
        .await;
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    assert!(
        h.fake.calls().iter().any(|c| c == "GET /actions/token"),
        "the job requested OIDC tokens"
    );
}

#[tokio::test]
async fn publish_a_workspace_in_github_actions() {
    let h = Harness::start().await;
    let repo = h.repo("story");
    h.fake.set_actions_claims(
        h.fake
            .actions_claims(&h.org, "acme/story", repo, "release.yml"),
    );
    let home = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    project(
        &h,
        ws.path(),
        "[workspace]\nresolver = \"3\"\nmembers = [\"story_core\", \"story_engine\"]\n",
    );
    std::fs::remove_dir_all(ws.path().join("src")).unwrap();
    for (name, deps) in [
        ("story_core", String::new()),
        (
            "story_engine",
            "story_core = { path = \"../story_core\", version = \"0.1\", registry = \"acme\" }\n"
                .to_owned(),
        ),
    ] {
        let dir = ws.path().join(name);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            package(name, "0.1.0", "acme/story", &deps),
        )
        .unwrap();
    }
    let output = cargo(
        home.path(),
        ws.path(),
        Env::Actions { fake: &h.fake },
        &[
            "publish",
            "--workspace",
            "--registry",
            "acme",
            "--allow-dirty",
            "--no-verify",
        ],
    )
    .await;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        h.fake
            .file(h.org.storage_repo, "index/st/or/story_core")
            .is_some()
    );
    let engine = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(engine.contains("\"name\":\"story_core\""), "{engine}");
}

/// GitHub's OIDC endpoint sometimes answers 504: the provider asks again, so one blip does not fail a publish
/// partway through a workspace.
#[tokio::test]
async fn a_failing_oidc_endpoint_is_retried() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let home = tempfile::tempdir().unwrap();
    h.fake.set_actions_claims(h.fake.actions_claims(
        &h.org,
        "acme/story-engine",
        repo,
        "release.yml",
    ));
    let krate = tempfile::tempdir().unwrap();
    project(
        &h,
        krate.path(),
        &package("story_engine", "0.1.0", "acme/story-engine", ""),
    );
    h.fake.fail_actions_tokens(2);
    let output = cargo(
        home.path(),
        krate.path(),
        Env::Actions { fake: &h.fake },
        &[
            "publish",
            "--registry",
            "acme",
            "--allow-dirty",
            "--no-verify",
        ],
    )
    .await;
    let err = stderr(&output);
    assert!(output.status.success(), "{err}");
    assert!(
        err.contains("504 Gateway Timeout); trying again in 1s"),
        "{err}"
    );
    assert!(
        h.fake
            .file(h.org.storage_repo, "index/st/or/story_engine")
            .is_some()
    );
}

#[tokio::test]
async fn a_job_without_id_token_permission_is_told_what_to_add() {
    let h = Harness::start().await;
    let home = tempfile::tempdir().unwrap();
    let consumer = tempfile::tempdir().unwrap();
    project(
        &h,
        consumer.path(),
        &package(
            "website",
            "0.1.0",
            "acme/website",
            "story_engine = { version = \"0.1\", registry = \"acme\" }\n",
        ),
    );
    let output = cargo(
        home.path(),
        consumer.path(),
        Env::ActionsWithoutIdToken,
        &["generate-lockfile"],
    )
    .await;
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("permissions: id-token: write"),
        "{}",
        stderr(&output)
    );
}

#[tokio::test]
async fn a_developer_signs_in_once() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &common::Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    let token = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    h.fake.set_device_flow_user(&token);
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let consumer = tempfile::tempdir().unwrap();
    project(
        &h,
        consumer.path(),
        &package(
            "website",
            "0.1.0",
            "acme/website",
            "story_engine = { version = \"0.1\", registry = \"acme\" }\n",
        ),
    );
    let dev = || Env::Developer {
        config: config.path(),
    };

    let output = cargo(home.path(), consumer.path(), dev(), &["generate-lockfile"]).await;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("enter the code WDJB-MJHT"),
        "{}",
        stderr(&output)
    );

    // The token is stored readable only by the user, and reused.
    let credentials = config.path().join("credentials.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&credentials)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let output = cargo(home.path(), consumer.path(), dev(), &["fetch"]).await;
    assert!(output.status.success(), "{}", stderr(&output));
    let device_codes = |calls: Vec<String>| {
        calls
            .iter()
            .filter(|c| *c == "POST /login/device/code")
            .count()
    };
    assert_eq!(device_codes(h.fake.calls()), 1);

    // An expired token is refreshed with GitHub, without signing in again.
    let mut stored: Value =
        serde_json::from_str(&std::fs::read_to_string(&credentials).unwrap()).unwrap();
    for entry in stored.as_object_mut().unwrap().values_mut() {
        entry["expires_at"] = 0.into();
    }
    std::fs::write(&credentials, stored.to_string()).unwrap();
    std::fs::remove_file(consumer.path().join("Cargo.lock")).unwrap();
    let output = cargo(home.path(), consumer.path(), dev(), &["generate-lockfile"]).await;
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(device_codes(h.fake.calls()), 1);

    // Logging out forgets the token.
    let output = cargo(
        home.path(),
        consumer.path(),
        dev(),
        &["logout", "--registry", "acme"],
    )
    .await;
    assert!(output.status.success(), "{}", stderr(&output));
    let stored = std::fs::read_to_string(&credentials).unwrap();
    assert!(!stored.contains("ghu_"), "{stored}");
}

#[tokio::test]
async fn a_domain_wide_sign_in_serves_every_registry() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &common::Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    let token = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let consumer = tempfile::tempdir().unwrap();
    project(
        &h,
        consumer.path(),
        &package(
            "website",
            "0.1.0",
            "acme/website",
            "story_engine = { version = \"0.1\", registry = \"acme\" }\n",
        ),
    );
    // What `cargo privatecrates login` stores: a token for the whole domain, not for one registry.
    let stored = serde_json::json!({
        h.apex(""): {
            "access_token": token,
            "expires_at": 4_000_000_000_i64,
            "refresh_token": null,
            "refresh_expires_at": null,
        }
    });
    std::fs::write(config.path().join("credentials.json"), stored.to_string()).unwrap();

    let output = cargo(
        home.path(),
        consumer.path(),
        Env::Developer {
            config: config.path(),
        },
        &["generate-lockfile"],
    )
    .await;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !h.fake
            .calls()
            .iter()
            .any(|c| c == "POST /login/device/code"),
        "no second sign-in"
    );
}

#[tokio::test]
async fn without_a_terminal_it_asks_for_cargo_login_instead_of_waiting() {
    let h = Harness::start().await;
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.set_device_flow_user(&token);
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let consumer = tempfile::tempdir().unwrap();
    project(
        &h,
        consumer.path(),
        &package(
            "website",
            "0.1.0",
            "acme/website",
            "story_engine = { version = \"0.1\", registry = \"acme\" }\n",
        ),
    );
    let started = std::time::Instant::now();
    let output = cargo(
        home.path(),
        consumer.path(),
        Env::Editor {
            config: config.path(),
        },
        &["generate-lockfile"],
    )
    .await;
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("cargo login --registry acme"), "{message}");
    assert!(
        !h.fake
            .calls()
            .iter()
            .any(|c| c == "POST /login/device/code"),
        "no sign-in was started"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}
