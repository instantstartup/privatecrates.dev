//! `cargo privatecrates init` shows its plan and writes nothing until it is agreed to.

use std::{path::Path, process::Command};

fn init(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-privatecrates"))
        .args(["privatecrates", "init", "--registry", "acme"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn crate_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-q"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/acme/story-engine.git",
    ]);
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"story_engine\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n\
         helper = { version = \"1\", registry = \"acme\" }\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    dir
}

#[test]
fn without_a_person_to_ask_it_only_shows_the_plan() {
    let dir = crate_repo();
    let manifest = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();

    // No terminal and no --yes: the plan, nothing written, and a non-zero exit.
    let output = init(dir.path(), &[]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("init will:"), "{text}");
    assert!(text.contains("create    .cargo/config.toml"), "{text}");
    assert!(
        text.contains("create    .github/workflows/publish.yml"),
        "{text}"
    );
    assert!(text.contains("Run it again with --yes"), "{text}");
    assert!(!dir.path().join(".cargo").exists());
    assert!(!dir.path().join(".github").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
        manifest
    );

    // --dry-run: the same plan, successfully.
    let output = init(dir.path(), &["--dry-run"]);
    assert!(output.status.success());
    assert!(!dir.path().join(".cargo").exists());

    // --yes --no-workflow: configures Cargo, leaves GitHub Actions alone.
    let output = init(dir.path(), &["--yes", "--no-workflow"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join(".cargo/config.toml").is_file());
    assert!(!dir.path().join(".github").exists());
    let manifest = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(manifest.contains("publish = [\"acme\"]"), "{manifest}");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("run init again without --no-workflow"),
        "{text}"
    );

    // Later, the workflow on its own.
    let output = init(dir.path(), &["--yes"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("created   .github/workflows/publish.yml"),
        "{text}"
    );
    assert!(text.contains("unchanged .cargo/config.toml"), "{text}");
}
