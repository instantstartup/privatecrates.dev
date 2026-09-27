//! The real `cargo` binary against the server: consuming, manual publishing and yanking.

mod common;

use std::path::Path;

use common::{Crate, Harness};

struct Cargo<'a> {
    h: &'a Harness,
    home: tempfile::TempDir,
}

impl<'a> Cargo<'a> {
    fn new(h: &'a Harness) -> Self {
        Self {
            h,
            home: tempfile::tempdir().unwrap(),
        }
    }

    /// Runs cargo in `dir` with `token` as the registry token. Returns (success, stderr).
    async fn run(&self, dir: &Path, token: &str, args: &[&str]) -> (bool, String) {
        let output = tokio::process::Command::new(env!("CARGO"))
            .args(args)
            .current_dir(dir)
            .env("CARGO_HOME", self.home.path())
            .env("CARGO_REGISTRIES_ACME_TOKEN", token)
            .env("CARGO_TERM_COLOR", "never")
            .env_remove("RUSTC_WRAPPER")
            .output()
            .await
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// A project directory whose `.cargo/config.toml` points at the registry.
    fn project(&self, name: &str, manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".cargo")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join(".cargo/config.toml"),
            format!(
                "[registries.acme]\nindex = \"sparse+{}/index/\"\ncredential-provider = \"cargo:token\"\n",
                self.h.base()
            ),
        )
        .unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), format!("//! {name}\n")).unwrap();
        dir
    }
}

#[tokio::test]
async fn cargo_consumes_publishes_and_yanks() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    h.publish_from_ci(
        "acme/story-engine",
        repo,
        &Crate::new("story_engine", "0.1.0", "acme/story-engine"),
    )
    .await;
    let pusher = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    let outsider = h
        .fake
        .add_user("mallory", "ghu_", &[(h.repo("other"), true)]);
    let cargo = Cargo::new(&h);

    // A consumer resolves and downloads the crate.
    let consumer = cargo.project(
        "site",
        "[package]\nname = \"site\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nstory_engine = { version = \"0.1\", registry = \"acme\" }\n",
    );
    let (ok, stderr) = cargo
        .run(consumer.path(), &pusher, &["generate-lockfile"])
        .await;
    assert!(ok, "generate-lockfile failed:\n{stderr}");
    let (ok, stderr) = cargo.run(consumer.path(), &pusher, &["fetch"]).await;
    assert!(ok, "fetch failed:\n{stderr}");
    let lock = std::fs::read_to_string(consumer.path().join("Cargo.lock")).unwrap();
    assert!(
        lock.contains("name = \"story_engine\"\nversion = \"0.1.0\""),
        "{lock}"
    );

    // Someone without access cannot resolve it.
    let other_home = Cargo::new(&h);
    let (ok, stderr) = other_home
        .run(consumer.path(), &outsider, &["generate-lockfile"])
        .await;
    assert!(!ok);
    assert!(stderr.contains("story_engine"), "{stderr}");

    // With manual publishing allowed, `cargo publish` from a machine works.
    h.fake.write_file(
        h.org.storage_repo,
        "owners/story_engine.toml",
        &format!(
            "repository_id = {repo}\nrepository = \"acme/story-engine\"\npublish_workflows = [\"release.yml\"]\nallow_manual_publish = true\n"
        ),
    );
    h.refresh().await;
    let krate = cargo.project(
        "story_engine",
        "[package]\nname = \"story_engine\"\nversion = \"0.2.0\"\nedition = \"2024\"\nlicense = \"MIT\"\ndescription = \"Stories\"\nrepository = \"https://github.com/acme/story-engine\"\n",
    );
    let (ok, stderr) = cargo
        .run(
            krate.path(),
            &pusher,
            &[
                "publish",
                "--registry",
                "acme",
                "--allow-dirty",
                "--no-verify",
            ],
        )
        .await;
    assert!(ok, "publish failed:\n{stderr}");
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(index.contains("\"vers\":\"0.2.0\""), "{index}");

    // The consumer picks up the new version.
    std::fs::write(
        consumer.path().join("Cargo.toml"),
        "[package]\nname = \"site\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nstory_engine = { version = \"0.2\", registry = \"acme\" }\n",
    )
    .unwrap();
    let (ok, stderr) = cargo.run(consumer.path(), &pusher, &["fetch"]).await;
    assert!(ok, "fetch of 0.2.0 failed:\n{stderr}");

    // `cargo yank` works with push access.
    let (ok, stderr) = cargo
        .run(
            krate.path(),
            &pusher,
            &["yank", "--registry", "acme", "story_engine@0.1.0"],
        )
        .await;
    assert!(ok, "yank failed:\n{stderr}");
    let index = h
        .fake
        .file(h.org.storage_repo, "index/st/or/story_engine")
        .unwrap();
    assert!(index.lines().next().unwrap().contains("\"yanked\":true"));
}

#[tokio::test]
async fn cargo_publish_of_a_ci_only_crate_explains_what_to_do() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let pusher = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    let cargo = Cargo::new(&h);
    let krate = cargo.project(
        "story_engine",
        "[package]\nname = \"story_engine\"\nversion = \"0.1.0\"\nedition = \"2024\"\nlicense = \"MIT\"\ndescription = \"Stories\"\nrepository = \"https://github.com/acme/story-engine\"\n",
    );
    let (ok, stderr) = cargo
        .run(
            krate.path(),
            &pusher,
            &[
                "publish",
                "--registry",
                "acme",
                "--allow-dirty",
                "--no-verify",
            ],
        )
        .await;
    assert!(!ok);
    assert!(
        stderr.contains("published from GitHub Actions only"),
        "{stderr}"
    );
    assert!(stderr.contains("/login#publish"), "{stderr}");
}
