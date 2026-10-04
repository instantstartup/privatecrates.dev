//! The whole flow for a crate repository: `cargo privatecrates init`, a publish from (simulated) GitHub Actions
//! with the credential provider, then `cargo privatecrates doctor`.

#[path = "../../privatecrates-server/tests/common/mod.rs"]
mod common;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Output,
};

use common::Harness;
use privatecrates_testkit::{ACTIONS_REQUEST_TOKEN, FakeGitHub};
use serde_json::Value;

const CLI: &str = env!("CARGO_BIN_EXE_cargo-privatecrates");
const PROVIDER: &str = "cargo-credential-privatecrates";

/// The directory holding the credential provider's binary, next to this crate's: built with the workspace, or
/// built here when only this crate's tests are run.
fn provider_dir() -> PathBuf {
    let dir = Path::new(CLI).parent().unwrap().to_owned();
    let exe = dir.join(format!("{PROVIDER}{}", std::env::consts::EXE_SUFFIX));
    if !exe.is_file() {
        let profile = match dir.file_name().and_then(|n| n.to_str()) {
            Some("debug") | None => "dev",
            Some(other) => other,
        };
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--profile", profile, "-p", PROVIDER])
            .status()
            .unwrap();
        assert!(status.success());
    }
    assert!(exe.is_file(), "{} is missing", exe.display());
    dir
}

/// `PATH` with the credential provider on it.
fn path_with_provider() -> OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(provider_dir()).chain(std::env::split_paths(&path)))
        .unwrap()
}

struct Env<'a> {
    home: &'a Path,
    config: &'a Path,
    path: OsString,
}

impl Env<'_> {
    fn command(&self, program: &str, dir: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(program);
        command
            .current_dir(dir)
            .env("PATH", &self.path)
            .env("CARGO_HOME", self.home)
            .env("CARGO_TERM_COLOR", "never")
            .env("PRIVATECRATES_CREDENTIAL_STORE", "file")
            .env("PRIVATECRATES_CONFIG_DIR", self.config)
            .env("PRIVATECRATES_TRUST_REGISTRY_GITHUB_URL", "1")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("CARGO_REGISTRIES_ACME_TOKEN")
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
            .env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");
        command
    }

    async fn cli(&self, dir: &Path, args: &[&str]) -> Output {
        self.command(CLI, dir)
            .arg("privatecrates")
            .args(args)
            .output()
            .await
            .unwrap()
    }

    /// `cargo` in a GitHub Actions job with `id-token: write`.
    async fn cargo_in_actions(&self, fake: &FakeGitHub, dir: &Path, args: &[&str]) -> Output {
        self.command(env!("CARGO"), dir)
            .args(args)
            .env("GITHUB_ACTIONS", "true")
            .env("CI", "true")
            .env("ACTIONS_ID_TOKEN_REQUEST_URL", fake.actions_token_url())
            .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", ACTIONS_REQUEST_TOKEN)
            .output()
            .await
            .unwrap()
    }
}

fn describe(output: &Output) -> String {
    format!(
        "{}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {}", describe(output)))
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// Each check's name, subject and status.
fn checks(report: &Value) -> Vec<(String, String, String)> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["check"].as_str().unwrap().to_owned(),
                c["subject"].as_str().unwrap_or_default().to_owned(),
                c["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn status_of<'a>(checks: &'a [(String, String, String)], check: &str) -> Vec<&'a str> {
    checks
        .iter()
        .filter(|(c, _, _)| c == check)
        .map(|(_, _, s)| s.as_str())
        .collect()
}

#[tokio::test]
async fn init_publish_from_ci_then_doctor() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let env = Env {
        home: home.path(),
        config: config.path(),
        path: path_with_provider(),
    };

    // A crate repository with no registry configuration and no `repository`.
    let krate = tempfile::tempdir().unwrap();
    let dir = krate.path();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"story_engine\"\nversion = \"0.1.0\"\nedition = \"2024\"\nlicense = \"MIT\"\n\
         description = \"test\"\npublish = [\"acme\"]\n",
    )
    .unwrap();
    git(dir, &["init", "-q"]);
    git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/story-engine.git",
        ],
    );

    let output = env
        .cli(
            dir,
            &[
                "init",
                "--yes",
                "--registry",
                "acme",
                "--url",
                &h.base(),
                "--json",
            ],
        )
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    let report = json(&output);
    let actions: Vec<_> = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["action"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actions, ["created", "updated", "created"], "{report:#}");
    assert_eq!(report["repository"], "https://github.com/acme/story-engine");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("repository = \"https://github.com/acme/story-engine\"\n"),
        "{manifest}"
    );
    let config_toml = std::fs::read_to_string(dir.join(".cargo/config.toml")).unwrap();
    assert!(
        config_toml.contains(&format!("index = \"sparse+{}/index/\"", h.base())),
        "{config_toml}"
    );
    let workflow = std::fs::read_to_string(dir.join(".github/workflows/publish.yml")).unwrap();
    assert!(workflow.contains("id-token: write"));

    // Idempotent.
    let output = env
        .cli(
            dir,
            &["init", "--yes", "--registry", "acme", "--url", &h.base()],
        )
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Nothing to change."),
        "{}",
        describe(&output)
    );

    // Before publishing, doctor finds everything in place but the version, and the developer is not signed in.
    let dev = h.fake.add_user("alice", "ghu_", &[(repo, false)]);
    h.fake.set_device_flow_user(&dev);
    let output = env.cli(dir, &["doctor", "--json"]).await;
    assert!(output.status.success(), "{}", describe(&output));
    let found = checks(&json(&output));
    assert_eq!(status_of(&found, "provider"), ["pass"]);
    assert_eq!(status_of(&found, "config"), ["pass"]);
    assert_eq!(status_of(&found, "registry"), ["pass"]);
    assert_eq!(status_of(&found, "access"), ["warn"]);
    assert_eq!(status_of(&found, "workflow"), ["pass"]);
    assert_eq!(status_of(&found, "repository"), ["pass"]);
    assert_eq!(status_of(&found, "publish"), ["pass"]);

    // CI publishes, from the workflow init wrote.
    h.fake.set_actions_claims(h.fake.actions_claims(
        &h.org,
        "acme/story-engine",
        repo,
        "publish.yml",
    ));
    let output = env
        .cargo_in_actions(
            &h.fake,
            dir,
            &[
                "publish",
                "--registry",
                "acme",
                "--allow-dirty",
                "--no-verify",
            ],
        )
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    let owners = h
        .fake
        .file(h.org.storage_repo, "owners/story_engine.toml")
        .unwrap();
    assert!(owners.contains("publish.yml"), "{owners}");

    // Signed in (to the deployment, which also reads the registry), everything passes.
    let output = env.cli(dir, &["login", "--url", &h.base()]).await;
    assert!(output.status.success(), "{}", describe(&output));
    let output = env
        .cli(dir, &["doctor", "--registry", "acme", "--json"])
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    let report = json(&output);
    assert_eq!(report["ok"], true);
    let found = checks(&report);
    assert!(
        found.iter().all(|(_, _, status)| status == "pass"),
        "{report:#}"
    );
    assert!(found.contains(&("index".into(), "story_engine".into(), "pass".into())));
    let output = env.cli(dir, &["doctor"]).await;
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("[pass] index (story_engine): story_engine 0.1.0 is published"),
        "{}",
        describe(&output)
    );

    // As a developer depending on crates: one it can see, and one it cannot (or that does not exist).
    let output = env
        .cli(dir, &["doctor", "--crate", "story-engine", "--json"])
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    let report = json(&output);
    assert_eq!(status_of(&checks(&report), "crate"), ["pass"], "{report:#}");
    assert_eq!(
        status_of(&checks(&report), "workflow"),
        Vec::<String>::new()
    );
    let output = env.cli(dir, &["doctor", "--crate", "secret_thing"]).await;
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("[FAIL] crate (secret_thing)"), "{text}");
    assert!(text.contains("ask them for read access"), "{text}");

    // Without the workflow's permission, or without the provider, doctor fails and says what to fix.
    std::fs::write(
        dir.join(".github/workflows/publish.yml"),
        workflow.replace("  id-token: write\n", ""),
    )
    .unwrap();
    let output = env.cli(dir, &["doctor"]).await;
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("[FAIL] workflow (publish.yml)"), "{text}");
    assert!(
        text.contains("fix: add `permissions: id-token: write`"),
        "{text}"
    );

    let without_provider = Env {
        path: std::env::var_os("PATH").unwrap_or_default(),
        ..env
    };
    let output = without_provider.cli(dir, &["doctor", "--json"]).await;
    assert!(!output.status.success());
    assert_eq!(status_of(&checks(&json(&output)), "provider"), ["fail"]);

    // A registry that is not configured.
    let output = env
        .cli(dir, &["doctor", "--registry", "globex", "--json"])
        .await;
    assert!(!output.status.success());
    let report = json(&output);
    assert_eq!(report["checks"][0]["check"], "config");
    assert_eq!(report["checks"][0]["status"], "fail");
}

#[tokio::test]
async fn init_configures_a_workspace() {
    let h = Harness::start().await;
    let home = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let env = Env {
        home: home.path(),
        config: config.path(),
        path: std::env::var_os("PATH").unwrap_or_default(),
    };
    let ws = tempfile::tempdir().unwrap();
    let dir = ws.path();
    std::fs::write(
        dir.join("Cargo.toml"),
        "# The story workspace.\n[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    for (name, extra) in [
        ("story_core", ""),
        (
            "story_engine",
            "repository = \"https://github.com/acme/story/tree/main/crates/story_engine\"\n",
        ),
    ] {
        let crate_dir = dir.join("crates").join(name);
        std::fs::create_dir_all(crate_dir.join("src")).unwrap();
        std::fs::write(crate_dir.join("src/lib.rs"), "").unwrap();
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n{extra}"
            ),
        )
        .unwrap();
    }
    // An unrelated workflow of the same name.
    std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    std::fs::write(dir.join(".github/workflows/publish.yml"), "name: docs\n").unwrap();
    git(dir, &["init", "-q"]);
    git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/story.git",
        ],
    );

    let output = env
        .cli(
            dir,
            &[
                "init",
                "--yes",
                "--registry",
                "acme",
                "--domain",
                &h.apex(""),
                "--json",
            ],
        )
        .await;
    assert!(!output.status.success());
    let error = json(&output);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--force"),
        "{error}"
    );
    std::fs::read_to_string(dir.join(".github/workflows/publish.yml"))
        .unwrap()
        .contains("name: docs")
        .then_some(())
        .unwrap();

    let output = env
        .cli(
            dir,
            &[
                "init",
                "--yes",
                "--registry",
                "acme",
                "--domain",
                &h.apex(""),
                "--force",
            ],
        )
        .await;
    assert!(output.status.success(), "{}", describe(&output));
    let root = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(root.starts_with("# The story workspace.\n"), "{root}");
    assert!(
        root.ends_with("[workspace.package]\nrepository = \"https://github.com/acme/story\"\n"),
        "{root}"
    );
    let core = std::fs::read_to_string(dir.join("crates/story_core/Cargo.toml")).unwrap();
    // Members inherit the repository, and may only publish to the registry.
    assert!(
        core.ends_with("repository.workspace = true\npublish = [\"acme\"]\n"),
        "{core}"
    );
    let engine = std::fs::read_to_string(dir.join("crates/story_engine/Cargo.toml")).unwrap();
    assert!(!engine.contains("workspace = true"), "{engine}");
    assert!(engine.contains("publish = [\"acme\"]"), "{engine}");
    let workflow = std::fs::read_to_string(dir.join(".github/workflows/publish.yml")).unwrap();
    assert!(
        workflow.contains("cargo publish --workspace --registry acme"),
        "{workflow}"
    );
    let config_toml = std::fs::read_to_string(dir.join(".cargo/config.toml")).unwrap();
    assert!(
        config_toml.contains(&format!("sparse+http://acme.localhost:{}/index/", h.port)),
        "{config_toml}"
    );

    // Run again from a member's directory: the workspace is found, and nothing changes.
    let output = env
        .cli(
            &dir.join("crates/story_core"),
            &[
                "init",
                "--yes",
                "--registry",
                "acme",
                "--domain",
                &h.apex(""),
            ],
        )
        .await;
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Nothing to change."),
        "{}",
        describe(&output)
    );
}
