//! Verifier checks against real git histories and an in-memory GitHub.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};

use jsonwebtoken::jwk::JwkSet;
use privatecrates_common::{audience, sha256_hex};
use privatecrates_testkit::{forge_oidc, oidc_jwks, sign_oidc};
use serde_json::{Value, json};

use super::*;
use crate::remote::{Asset, CommitInfo, Release, RemoteError};

const APP: &str = "privatecrates-storage[bot]";
const BASE: &str = "https://acme.privatecrates.dev";
const ISSUER: &str = "https://token.actions.githubusercontent.com";
const REPO_ID: u64 = 5;

#[derive(Default)]
struct Mem {
    commits: HashMap<String, (String, bool)>,
    releases: HashMap<String, Release>,
    assets: HashMap<u64, Vec<u8>>,
    crates_io: HashSet<String>,
    commit_lookups: RefCell<usize>,
    next_id: RefCell<u64>,
}

impl Remote for Mem {
    fn commit(&self, sha: &str) -> Result<CommitInfo, RemoteError> {
        *self.commit_lookups.borrow_mut() += 1;
        let (login, verified) = self
            .commits
            .get(sha)
            .cloned()
            .unwrap_or(("alice".into(), true));
        Ok(CommitInfo {
            login: Some(login),
            verified,
        })
    }
    fn release(&self, tag: &str) -> Result<Option<Release>, RemoteError> {
        Ok(self.releases.get(tag).cloned())
    }
    fn asset(&self, id: u64) -> Result<Vec<u8>, RemoteError> {
        Ok(self.assets[&id].clone())
    }
    fn jwks(&self) -> Result<JwkSet, RemoteError> {
        Ok(serde_json::from_value(oidc_jwks()).unwrap())
    }
    fn crates_io_exists(&self, name: &str) -> Result<bool, RemoteError> {
        Ok(self.crates_io.contains(name))
    }
}

impl Mem {
    fn id(&self) -> u64 {
        let mut id = self.next_id.borrow_mut();
        *id += 1;
        *id
    }

    /// An immutable release for a version, with its `.crate` and optional provenance.
    fn add_release(&mut self, name: &str, version: &str, krate: &[u8], provenance: Option<String>) {
        let crate_id = self.id();
        self.assets.insert(crate_id, krate.to_vec());
        let mut assets = vec![Asset {
            id: crate_id,
            name: format!("{name}-{version}.crate"),
            digest: Some(format!("sha256:{}", sha256_hex(krate))),
        }];
        if let Some(jwt) = provenance {
            let id = self.id();
            self.assets.insert(id, jwt.into_bytes());
            assets.push(Asset {
                id,
                name: format!("{name}-{version}.provenance.jwt"),
                digest: None,
            });
        }
        self.releases.insert(
            format!("{name}-{version}"),
            Release {
                draft: false,
                immutable: true,
                assets,
            },
        );
    }
}

fn claims(workflow: &str) -> Value {
    json!({
        "repository": "acme/story-engine",
        "repository_id": REPO_ID.to_string(),
        "repository_owner_id": "100",
        "job_workflow_ref": format!("acme/story-engine/.github/workflows/{workflow}@refs/tags/v1"),
        "event_name": "push",
    })
}

fn provenance(name: &str, version: &str, krate: &[u8], workflow: &str) -> String {
    sign_oidc(
        ISSUER,
        &audience::publish(BASE, name, version, &sha256_hex(krate)),
        &claims(workflow),
    )
}

fn line(name: &str, version: &str, krate: &[u8], yanked: bool) -> String {
    format!(
        r#"{{"name":"{name}","vers":"{version}","deps":[],"cksum":"{}","features":{{}},"yanked":{yanked},"links":null}}"#,
        sha256_hex(krate)
    )
}

const OWNERS: &str = "repository_id = 5\nrepository = \"acme/story-engine\"\npublish_workflows = [\"release.yml\"]\n";
const INDEX: &str = "index/st/or/story_engine";

struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        let repo = Self { dir };
        repo.commit(&[("privatecrates.toml", Some("slug = \"acme\"\n"))]);
        repo
    }

    /// Commits file changes and returns the commit's sha.
    fn commit(&self, files: &[(&str, Option<&str>)]) -> String {
        for (path, content) in files {
            let full = self.dir.path().join(path);
            match content {
                Some(c) => {
                    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                    std::fs::write(&full, c).unwrap();
                }
                None => std::fs::remove_file(&full).unwrap(),
            }
        }
        git(self.dir.path(), &["add", "-A"]);
        git(
            self.dir.path(),
            &["commit", "-q", "--allow-empty", "-m", "change"],
        );
        git(self.dir.path(), &["rev-parse", "HEAD"])
            .trim()
            .to_owned()
    }

    /// A commit made by the storage App.
    fn app_commit(&self, mem: &mut Mem, files: &[(&str, Option<&str>)]) -> String {
        let sha = self.commit(files);
        mem.commits.insert(sha.clone(), (APP.into(), true));
        sha
    }

    fn git(&self) -> Git {
        Git::new(self.dir.path())
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn options() -> Options {
    Options {
        base_url: BASE.into(),
        storage_app_login: APP.into(),
        oidc_issuer: ISSUER.into(),
    }
}

fn run(repo: &Repo, mem: &Mem, state: &State) -> Report {
    verify(&repo.git(), mem, &options(), state).unwrap()
}

fn errors(report: &Report) -> Vec<String> {
    report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{}: {}", f.subject, f.message))
        .collect()
}

/// A history with two trusted publishes of `story_engine` and a yank, all as the service would write them.
fn published() -> (Repo, Mem) {
    let repo = Repo::new();
    let mut mem = Mem::default();
    let (v1, v2) = (b"one".as_slice(), b"two".as_slice());
    mem.add_release(
        "story_engine",
        "0.1.0",
        v1,
        Some(provenance("story_engine", "0.1.0", v1, "release.yml")),
    );
    repo.app_commit(&mut mem, &[("owners/story_engine.toml", Some(OWNERS))]);
    let first = format!("{}\n", line("story_engine", "0.1.0", v1, false));
    repo.app_commit(&mut mem, &[(INDEX, Some(&first))]);
    mem.add_release(
        "story_engine",
        "0.2.0",
        v2,
        Some(provenance("story_engine", "0.2.0", v2, "release.yml")),
    );
    let both = format!("{first}{}\n", line("story_engine", "0.2.0", v2, false));
    repo.app_commit(&mut mem, &[(INDEX, Some(&both))]);
    let yanked = both.replacen("\"yanked\":false", "\"yanked\":true", 1);
    repo.app_commit(&mut mem, &[(INDEX, Some(&yanked))]);
    (repo, mem)
}

#[test]
fn a_clean_history_verifies() {
    let (repo, mem) = published();
    let report = run(&repo, &mem, &State::default());
    assert!(errors(&report).is_empty(), "{:?}", errors(&report));
    assert!(
        report.findings.iter().all(|f| f.severity == Severity::Info),
        "{:?}",
        report.findings
    );
    assert_eq!(report.state.versions.len(), 2);
    assert!(report.state.commit.is_some());
}

#[test]
fn later_runs_check_only_what_is_new() {
    let (repo, mem) = published();
    let first = run(&repo, &mem, &State::default());
    *mem.commit_lookups.borrow_mut() = 0;
    let second = run(&repo, &mem, &first.state);
    assert!(errors(&second).is_empty());
    assert_eq!(*mem.commit_lookups.borrow(), 0);
    assert_eq!(second.state, first.state);
}

#[test]
fn a_person_editing_the_index_is_reported() {
    let (repo, mem) = published();
    let current = std::fs::read_to_string(repo.dir.path().join(INDEX)).unwrap();
    // Not an App commit: `Mem` attributes unknown commits to a person.
    repo.commit(&[(
        INDEX,
        Some(&current.replace("\"yanked\":true", "\"yanked\":false")),
    )]);
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter().any(|e| e.contains("not made by the storage App")),
        "{e:?}"
    );
}

#[test]
fn rewriting_a_published_line_is_reported_even_from_the_app() {
    let (repo, mut mem) = published();
    let current = std::fs::read_to_string(repo.dir.path().join(INDEX)).unwrap();
    let evil = current.replace(&sha256_hex(b"one"), &sha256_hex(b"evil"));
    repo.app_commit(&mut mem, &[(INDEX, Some(&evil))]);
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter()
            .any(|e| e.contains("rewrote published index lines")),
        "{e:?}"
    );
    // And the rewritten checksum no longer matches the immutable release.
    assert!(
        e.iter()
            .any(|e| e.contains("does not match the index checksum")),
        "{e:?}"
    );
}

#[test]
fn an_unsigned_app_commit_is_not_trusted() {
    let (repo, mut mem) = published();
    let current = std::fs::read_to_string(repo.dir.path().join(INDEX)).unwrap();
    let sha = repo.commit(&[(
        INDEX,
        Some(&current.replace("\"yanked\":true", "\"yanked\":false")),
    )]);
    mem.commits.insert(sha, (APP.into(), false));
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter().any(|e| e.contains("not made by the storage App")),
        "{e:?}"
    );
}

#[test]
fn the_app_may_not_change_owners_or_settings() {
    let (repo, mut mem) = published();
    let hijack = OWNERS.replace("repository_id = 5", "repository_id = 666");
    repo.app_commit(&mut mem, &[("owners/story_engine.toml", Some(&hijack))]);
    repo.app_commit(
        &mut mem,
        &[("privatecrates.toml", Some("slug = \"evil\"\n"))],
    );
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter().any(|e| e.starts_with("owners/story_engine.toml")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|e| e.starts_with("privatecrates.toml")),
        "{e:?}"
    );
}

#[test]
fn the_app_may_create_settings_at_sign_up() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let repo = Repo { dir };
    repo.commit(&[("README.md", Some("crates\n"))]);
    let mut mem = Mem::default();
    repo.app_commit(
        &mut mem,
        &[("privatecrates.toml", Some("slug = \"acme\"\n"))],
    );
    assert_eq!(
        errors(&run(&repo, &mem, &State::default())),
        Vec::<String>::new()
    );
    repo.app_commit(
        &mut mem,
        &[("privatecrates.toml", Some("slug = \"evil\"\n"))],
    );
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter().any(|e| e.starts_with("privatecrates.toml")),
        "{e:?}"
    );
}

#[test]
fn a_person_may_change_owners() {
    let (repo, mem) = published();
    repo.commit(&[(
        "owners/story_engine.toml",
        Some(&OWNERS.replace("release.yml", "publish.yml")),
    )]);
    let report = run(&repo, &mem, &State::default());
    assert!(errors(&report).is_empty(), "{:?}", errors(&report));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.subject == "owners/story_engine.toml")
    );
}

/// Appends 0.3.0 with the given provenance and owners.
fn append_v3(repo: &Repo, mem: &mut Mem, provenance: Option<String>, owners: Option<&str>) {
    let v3 = b"three".as_slice();
    mem.add_release("story_engine", "0.3.0", v3, provenance);
    if let Some(owners) = owners {
        repo.commit(&[("owners/story_engine.toml", Some(owners))]);
    }
    let current = std::fs::read_to_string(repo.dir.path().join(INDEX)).unwrap();
    let next = format!("{current}{}\n", line("story_engine", "0.3.0", v3, false));
    repo.app_commit(mem, &[(INDEX, Some(&next))]);
}

#[test]
fn provenance_from_each_allowed_trigger_verifies() {
    let v3 = b"three".as_slice();
    let audience_v3 = audience::publish(BASE, "story_engine", "0.3.0", &sha256_hex(v3));
    for event in ["push", "release", "workflow_dispatch"] {
        let mut claims = claims("release.yml");
        claims["event_name"] = event.into();
        let (repo, mut mem) = published();
        append_v3(
            &repo,
            &mut mem,
            Some(sign_oidc(ISSUER, &audience_v3, &claims)),
            None,
        );
        let e = errors(&run(&repo, &mem, &State::default()));
        assert!(e.is_empty(), "{event}: {e:?}");
    }
}

#[test]
fn a_version_without_provenance_is_an_error_unless_manual_publishing_is_allowed() {
    let (repo, mut mem) = published();
    append_v3(&repo, &mut mem, None, None);
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter()
            .any(|e| e.contains("did not allow manual publishing")),
        "{e:?}"
    );

    let (repo, mut mem) = published();
    let manual = format!("{OWNERS}allow_manual_publish = true\n");
    append_v3(&repo, &mut mem, None, Some(&manual));
    let report = run(&repo, &mem, &State::default());
    assert!(errors(&report).is_empty(), "{:?}", errors(&report));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("manually"))
    );
}

#[test]
fn the_first_version_must_have_provenance() {
    let repo = Repo::new();
    let mut mem = Mem::default();
    let v1 = b"one".as_slice();
    mem.add_release("story_engine", "0.1.0", v1, None);
    repo.app_commit(
        &mut mem,
        &[(
            "owners/story_engine.toml",
            Some(&format!("{OWNERS}allow_manual_publish = true\n")),
        )],
    );
    repo.app_commit(
        &mut mem,
        &[(
            INDEX,
            Some(&format!("{}\n", line("story_engine", "0.1.0", v1, false))),
        )],
    );
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(e.iter().any(|e| e.contains("first version")), "{e:?}");
}

#[test]
fn forged_or_misused_provenance_is_reported() {
    let v3 = b"three".as_slice();
    let audience_v3 = audience::publish(BASE, "story_engine", "0.3.0", &sha256_hex(v3));
    let cases = [
        (
            "forged",
            forge_oidc(ISSUER, &audience_v3, &claims("release.yml")),
            "InvalidSignature",
        ),
        (
            "reused",
            provenance("story_engine", "0.2.0", b"two", "release.yml"),
            "expected audience",
        ),
        (
            "workflow",
            provenance("story_engine", "0.3.0", v3, "sneaky.yml"),
            "sneaky.yml",
        ),
        (
            "trigger",
            sign_oidc(ISSUER, &audience_v3, &{
                let mut claims = claims("release.yml");
                claims["event_name"] = "pull_request_target".into();
                claims
            }),
            "triggered by `pull_request_target`",
        ),
        (
            "no trigger",
            sign_oidc(ISSUER, &audience_v3, &{
                let mut claims = claims("release.yml");
                claims.as_object_mut().unwrap().remove("event_name");
                claims
            }),
            "does not say what triggered",
        ),
    ];
    for (case, jwt, expected) in cases {
        let (repo, mut mem) = published();
        append_v3(&repo, &mut mem, Some(jwt), None);
        let e = errors(&run(&repo, &mem, &State::default()));
        assert!(e.iter().any(|e| e.contains(expected)), "{case}: {e:?}");
    }
}

#[test]
fn release_problems_are_reported() {
    let (repo, mut mem) = published();
    mem.releases
        .get_mut("story_engine-0.1.0")
        .unwrap()
        .immutable = false;
    mem.releases.remove("story_engine-0.2.0");
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(
        e.iter()
            .any(|e| e.starts_with("story_engine 0.1.0") && e.contains("not immutable")),
        "{e:?}"
    );
    assert!(
        e.iter()
            .any(|e| e.starts_with("story_engine 0.2.0") && e.contains("missing")),
        "{e:?}"
    );
}

#[test]
fn without_a_digest_the_crate_is_checked_from_its_bytes() {
    let (repo, mut mem) = published();
    let crate_id = {
        let asset = &mut mem.releases.get_mut("story_engine-0.1.0").unwrap().assets[0];
        asset.digest = None;
        asset.id
    };
    let e = errors(&run(&repo, &mem, &State::default()));
    assert!(e.is_empty(), "{e:?}");
    mem.assets.insert(crate_id, b"tampered".to_vec());
    let e = errors(&run(&repo, &mem, &State::default()));
    assert_eq!(
        e,
        ["story_engine 0.1.0: the .crate file does not match the index checksum"]
    );
}

#[test]
fn rewritten_history_is_reported() {
    let (repo, mem) = published();
    let state = State {
        commit: Some("0".repeat(40)),
        versions: Default::default(),
    };
    let e = errors(&run(&repo, &mem, &state));
    assert!(e.iter().any(|e| e.contains("rewritten")), "{e:?}");
}

#[test]
fn crates_io_clashes_are_warnings() {
    let (repo, mut mem) = published();
    mem.crates_io.insert("story_engine".into());
    let report = run(&repo, &mem, &State::default());
    assert!(errors(&report).is_empty());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("crates.io"))
    );
}
