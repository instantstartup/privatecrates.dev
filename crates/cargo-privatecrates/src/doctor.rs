//! `cargo privatecrates doctor`: checks that a crate repository can use and publish to its registries, and says
//! how to fix what is missing.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

use privatecrates_auth::{device, store::Store};
use privatecrates_common::{audience, index};
use reqwest::{StatusCode, blocking::Client};
use serde::Serialize;
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::{
    init::{PROVIDER, is_privatecrates_provider, publishes_to},
    project::{self, Package, Project},
    target::{self, Domain},
};

#[derive(Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
    /// No check failed (warnings allowed).
    pub ok: bool,
}

#[derive(Serialize)]
pub struct Check {
    pub check: &'static str,
    /// The registry, package or file checked, where there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub status: Status,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

impl Check {
    fn new(check: &'static str, subject: Option<&str>, status: Status, detail: String) -> Self {
        Self {
            check,
            subject: subject.map(str::to_owned),
            status,
            detail,
            fix: None,
        }
    }

    fn fix(self, fix: impl Into<String>) -> Self {
        Self {
            fix: Some(fix.into()),
            ..self
        }
    }
}

/// A registry configured with the credential provider.
pub struct Registry {
    pub name: String,
    /// The base URL, e.g. `https://acme.privatecrates.dev`.
    pub base: Option<String>,
    /// The provider command, when it names a path rather than a program on `PATH`.
    pub provider_path: Option<PathBuf>,
}

pub struct Options<'a> {
    /// The registries to check; all those using the credential provider when empty.
    pub registries: &'a [String],
    /// For suggested commands.
    pub domain: &'a Domain,
    /// `PATH`, where the provider is looked for.
    pub path: Option<OsString>,
}

pub fn run(dir: &Path, options: &Options<'_>) -> Report {
    let mut checks = Vec::new();
    let project = match Project::load(dir) {
        Ok(project) => Some(project),
        Err(e) => {
            checks.push(
                Check::new("project", None, Status::Fail, e.to_string())
                    .fix("run this in a crate repository or workspace"),
            );
            None
        }
    };
    let root = project.as_ref().map_or(dir, |p| p.root.as_path());
    let init = |name: &str| {
        format!(
            "cargo privatecrates init --registry {name}{}",
            options.domain.flag()
        )
    };

    let config_path = root.join(".cargo/config.toml");
    let configured = match read_config(&config_path) {
        Ok(registries) => registries,
        Err(detail) => {
            checks.push(Check::new("config", None, Status::Fail, detail));
            Vec::new()
        }
    };
    let registries: Vec<&Registry> = if options.registries.is_empty() {
        configured.iter().collect()
    } else {
        options
            .registries
            .iter()
            .filter_map(|name| {
                let found = configured.iter().find(|r| &r.name == name);
                if found.is_none() {
                    checks.push(
                        Check::new(
                            "config",
                            Some(name),
                            Status::Fail,
                            format!(
                                "{} does not configure the registry {name} with {PROVIDER}",
                                config_path.display()
                            ),
                        )
                        .fix(init(name)),
                    );
                }
                found
            })
            .collect()
    };
    if registries.is_empty() && options.registries.is_empty() {
        checks.push(
            Check::new(
                "config",
                None,
                Status::Fail,
                format!(
                    "{} configures no registry with {PROVIDER}",
                    config_path.display()
                ),
            )
            .fix(init("<name>")),
        );
    }

    checks.push(provider(options.path.as_deref(), &registries));

    let remote = project::github_remote(root);
    let git_root = project::git_root(root).unwrap_or_else(|| root.to_owned());
    for registry in &registries {
        let name = registry.name.as_str();
        let Some(base) = &registry.base else {
            checks.push(
                Check::new(
                    "config",
                    Some(name),
                    Status::Fail,
                    "its index is not a sparse registry URL ending in /index/".into(),
                )
                .fix(format!("{} --force", init(name))),
            );
            continue;
        };
        checks.push(Check::new(
            "config",
            Some(name),
            Status::Pass,
            format!("{} uses {base}", config_path.display()),
        ));
        let http = match target::http(base) {
            Ok(http) => http,
            Err(e) => {
                checks.push(Check::new(
                    "registry",
                    Some(name),
                    Status::Fail,
                    e.to_string(),
                ));
                continue;
            }
        };
        let answers = answers(&http, base, name);
        let reachable = answers.status == Status::Pass;
        checks.push(answers);
        let token = token(&http, base);
        if reachable {
            checks.push(access(&http, base, name, token.as_deref()));
        }
        checks.push(workflow(&git_root, name).unwrap_or_else(|| {
            Check::new(
                "workflow",
                Some(name),
                Status::Fail,
                format!(
                    "no workflow in {} runs `cargo publish --registry {name}`",
                    git_root.join(".github/workflows").display()
                ),
            )
            .fix(init(name))
        }));
        for package in publishable(project.as_ref()) {
            checks.push(publish_restricted(package, name));
            if let (true, Some(token)) = (reachable, &token) {
                checks.push(published(&http, base, token, package));
            }
        }
    }
    let registry = registries.first().map_or("<name>", |r| r.name.as_str());
    for package in publishable(project.as_ref()) {
        checks.push(repository(package, remote.as_deref(), &init(registry)));
    }
    let ok = checks.iter().all(|c| c.status != Status::Fail);
    Report { checks, ok }
}

/// The project's packages, except those with `publish = false`.
fn publishable(project: Option<&Project>) -> impl Iterator<Item = &Package> {
    project
        .into_iter()
        .flat_map(|p| &p.packages)
        .filter(|p| !p.publish.as_ref().is_some_and(Vec::is_empty))
}

/// The registries in `.cargo/config.toml` that use the credential provider.
pub fn read_config(path: &Path) -> Result<Vec<Registry>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;
    let Some(registries) = doc.get("registries").and_then(|r| r.as_table_like()) else {
        return Ok(Vec::new());
    };
    Ok(registries
        .iter()
        .filter_map(|(name, entry)| {
            let entry = entry.as_table_like()?;
            let provider = entry.get("credential-provider")?.as_value();
            if !is_privatecrates_provider(provider) {
                return None;
            }
            let command = provider
                .and_then(|p| p.as_array())
                .and_then(|a| a.get(0))
                .and_then(|v| v.as_str());
            Some(Registry {
                name: name.to_owned(),
                base: entry
                    .get("index")
                    .and_then(|i| i.as_str())
                    .and_then(audience::base_url_from_index),
                provider_path: command
                    .filter(|c| c.contains(['/', '\\']))
                    .map(PathBuf::from),
            })
        })
        .collect())
}

/// The credential provider is installed: on `PATH`, or where the configuration names it.
pub fn provider(path: Option<&std::ffi::OsStr>, registries: &[&Registry]) -> Check {
    let file = format!("{PROVIDER}{}", std::env::consts::EXE_SUFFIX);
    let found = match registries.iter().find_map(|r| r.provider_path.as_ref()) {
        Some(configured) => configured.is_file().then(|| configured.clone()),
        None => path
            .into_iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join(&file))
            .find(|p| p.is_file()),
    };
    match found {
        Some(at) => Check::new(
            "provider",
            None,
            Status::Pass,
            format!("{PROVIDER} is installed at {}", at.display()),
        ),
        None => Check::new(
            "provider",
            None,
            Status::Fail,
            format!("{PROVIDER} is not installed (not found on PATH)"),
        )
        .fix(format!(
            "cargo binstall {PROVIDER}, or cargo install {PROVIDER} --locked"
        )),
    }
}

/// The registry is there and wants a token.
fn answers(http: &Client, base: &str, name: &str) -> Check {
    let url = format!("{base}/index/config.json");
    match http.get(&url).send() {
        Ok(r) if r.status() == StatusCode::UNAUTHORIZED => Check::new(
            "registry",
            Some(name),
            Status::Pass,
            format!("{base} answers, and requires a token"),
        ),
        Ok(r) => Check::new(
            "registry",
            Some(name),
            Status::Fail,
            format!("{url} answered {} without a token, not 401", r.status()),
        )
        .fix("check the registry name and URL in .cargo/config.toml"),
        Err(e) => Check::new(
            "registry",
            Some(name),
            Status::Fail,
            format!("could not reach {base}: {e}"),
        ),
    }
}

/// The stored token for the registry, or for its deployment (the same reader App signs in to both).
fn token(http: &Client, base: &str) -> Option<String> {
    let store = Store::open().ok()?;
    let apex = Domain::of_registry(base).map(|d| d.apex());
    [Some(base.to_owned()), apex]
        .into_iter()
        .flatten()
        .find_map(|key| device::current(http, &key, &store).ok().flatten())
        .map(|stored| stored.access_token)
}

/// The signed-in user can read the registry.
fn access(http: &Client, base: &str, name: &str, token: Option<&str>) -> Check {
    let Some(token) = token else {
        return Check::new(
            "access",
            Some(name),
            Status::Warn,
            "not signed in, so access was not checked".into(),
        )
        .fix(format!("cargo login --registry {name}"));
    };
    let url = format!("{base}/index/config.json");
    match http.get(&url).header("Authorization", token).send() {
        Ok(r) if r.status().is_success() => Check::new(
            "access",
            Some(name),
            Status::Pass,
            "you can read the registry".into(),
        ),
        Ok(r) => {
            let status = r.status();
            let detail = r
                .json::<Value>()
                .ok()
                .and_then(|v| v["errors"][0]["detail"].as_str().map(str::to_owned))
                .unwrap_or_default();
            Check::new(
                "access",
                Some(name),
                Status::Fail,
                format!("{url} answered {status}: {detail}"),
            )
            .fix(format!(
                "sign in again with cargo login --registry {name}, as a member of the organisation"
            ))
        }
        Err(e) => Check::new(
            "access",
            Some(name),
            Status::Fail,
            format!("could not reach {base}: {e}"),
        ),
    }
}

/// A workflow that publishes to the registry, and whether it may request OIDC tokens. `None` when there is none.
pub fn workflow(git_root: &Path, name: &str) -> Option<Check> {
    let dir = git_root.join(".github/workflows");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .collect();
    files.sort();
    let (path, text) = files.into_iter().find_map(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        publishes_to(&text, name).then_some((path, text))
    })?;
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned());
    Some(if has_id_token_write(&text) {
        Check::new(
            "workflow",
            file.as_deref(),
            Status::Pass,
            format!(
                "{} publishes to {name} with `id-token: write`",
                path.display()
            ),
        )
    } else {
        Check::new(
            "workflow",
            file.as_deref(),
            Status::Fail,
            format!(
                "{} publishes to {name} but cannot request an OIDC token",
                path.display()
            ),
        )
        .fix("add `permissions: id-token: write` (and `contents: read`) to the workflow or its publish job")
    })
}

/// Whether a workflow grants `id-token: write`.
pub fn has_id_token_write(workflow: &str) -> bool {
    workflow.lines().any(|line| {
        let line = line.split('#').next().unwrap_or_default();
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        compact == "id-token:write"
            || compact.contains("id-token:write,")
            || compact.ends_with("id-token:write}")
    })
}

/// `package.repository` names the repository of the `origin` remote.
fn repository(package: &Package, remote: Option<&str>, init: &str) -> Check {
    let name = Some(package.name.as_str());
    match (&package.repository, remote) {
        (None, _) => Check::new(
            "repository",
            name,
            Status::Fail,
            "package.repository is missing; the registry needs it to know the crate's owning repository".into(),
        )
        .fix(init),
        (Some(repository), None) => Check::new(
            "repository",
            name,
            Status::Warn,
            format!("package.repository is {repository}, but the `origin` remote is not a GitHub repository"),
        ),
        (Some(repository), Some(remote)) if project::repository_matches(repository, remote) => {
            Check::new(
                "repository",
                name,
                Status::Pass,
                format!("package.repository is {repository}"),
            )
        }
        (Some(repository), Some(remote)) => Check::new(
            "repository",
            name,
            Status::Fail,
            format!("package.repository is {repository}, but the `origin` remote is {remote}"),
        )
        .fix(format!("set package.repository to {remote}")),
    }
}

/// The package can only be published to the registry, never to crates.io by accident.
fn publish_restricted(package: &Package, registry: &str) -> Check {
    let name = Some(package.name.as_str());
    match &package.publish {
        Some(list) if list.iter().any(|r| r == registry) => {
            Check::new("publish", name, Status::Pass, format!("publish = {list:?}"))
        }
        _ => Check::new(
            "publish",
            name,
            Status::Warn,
            format!("{} may also be published to crates.io", package.name),
        )
        .fix(format!("add publish = [\"{registry}\"] to its [package]")),
    }
}

/// The package's current version is in the registry's index.
fn published(http: &Client, base: &str, token: &str, package: &Package) -> Check {
    let name = Some(package.name.as_str());
    let url = format!("{base}/index/{}", index::path(&package.name));
    let found = http
        .get(&url)
        .header("Authorization", token)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.text().ok())
        .and_then(|text| index::IndexFile::parse(&text).ok())
        .is_some_and(|file| file.contains_version(&package.version));
    if found {
        Check::new(
            "index",
            name,
            Status::Pass,
            format!("{} {} is published", package.name, package.version),
        )
    } else {
        Check::new(
            "index",
            name,
            Status::Warn,
            format!("{} {} is not published yet", package.name, package.version),
        )
        .fix(format!(
            "merge the workflow, then push a tag: git tag v{0} && git push origin v{0}",
            package.version
        ))
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for check in &self.checks {
            let status = match check.status {
                Status::Pass => "pass",
                Status::Warn => "warn",
                Status::Fail => "FAIL",
            };
            let subject = check
                .subject
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            writeln!(f, "[{status}] {}{subject}: {}", check.check, check.detail)?;
            if let Some(fix) = &check.fix {
                writeln!(f, "       fix: {fix}")?;
            }
        }
        let count = |s| self.checks.iter().filter(|c| c.status == s).count();
        write!(
            f,
            "\n{} passed, {} warnings, {} failed",
            count(Status::Pass),
            count(Status::Warn),
            count(Status::Fail)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_registries_using_the_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[registries.acme]\nindex = \"sparse+https://acme.privatecrates.dev/index/\"\n\
             credential-provider = [\"cargo-credential-privatecrates\"]\n\n\
             [registries.local]\nindex = \"sparse+http://acme.localhost:8080/index/\"\n\
             credential-provider = [\"/opt/bin/cargo-credential-privatecrates\"]\n\n\
             [registries.other]\nindex = \"sparse+https://other.example/index/\"\n\
             credential-provider = \"cargo:token\"\n",
        )
        .unwrap();
        let registries = read_config(&path).unwrap();
        let names: Vec<_> = registries.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["acme", "local"]);
        assert_eq!(
            registries[0].base.as_deref(),
            Some("https://acme.privatecrates.dev")
        );
        assert_eq!(registries[0].provider_path, None);
        assert_eq!(
            registries[1].provider_path.as_deref(),
            Some(Path::new("/opt/bin/cargo-credential-privatecrates"))
        );
        assert!(
            read_config(&dir.path().join("missing.toml"))
                .unwrap()
                .is_empty()
        );
        std::fs::write(&path, "[registries").unwrap();
        assert!(read_config(&path).is_err());
    }

    #[test]
    fn looks_for_the_provider_on_path() {
        let bin = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let path = std::env::join_paths([empty.path(), bin.path()]).unwrap();
        assert_eq!(provider(Some(&path), &[]).status, Status::Fail);
        let exe = bin
            .path()
            .join(format!("{PROVIDER}{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&exe, "").unwrap();
        let check = provider(Some(&path), &[]);
        assert_eq!(check.status, Status::Pass, "{}", check.detail);
        assert_eq!(provider(None, &[]).status, Status::Fail);

        // A provider configured by path is looked for there, not on PATH.
        let configured = Registry {
            name: "acme".into(),
            base: None,
            provider_path: Some(empty.path().join(PROVIDER)),
        };
        assert_eq!(provider(Some(&path), &[&configured]).status, Status::Fail);
        let configured = Registry {
            provider_path: Some(exe),
            ..configured
        };
        assert_eq!(provider(None, &[&configured]).status, Status::Pass);
    }

    #[test]
    fn checks_the_publish_workflow() {
        let root = tempfile::tempdir().unwrap();
        assert!(workflow(root.path(), "acme").is_none());
        let dir = root.path().join(".github/workflows");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ci.yml"),
            "jobs:\n  build:\n    steps:\n      - run: cargo build\n",
        )
        .unwrap();
        assert!(workflow(root.path(), "acme").is_none());

        std::fs::write(
            dir.join("release.yaml"),
            "permissions:\n  contents: read\njobs:\n  p:\n    steps:\n      - run: cargo publish --registry acme\n",
        )
        .unwrap();
        let check = workflow(root.path(), "acme").unwrap();
        assert_eq!(check.status, Status::Fail);
        assert_eq!(check.subject.as_deref(), Some("release.yaml"));
        assert!(check.fix.unwrap().contains("id-token: write"));

        std::fs::write(
            dir.join("release.yaml"),
            crate::init::workflow("acme", false, None),
        )
        .unwrap();
        assert_eq!(workflow(root.path(), "acme").unwrap().status, Status::Pass);
        assert!(workflow(root.path(), "globex").is_none());
    }

    #[test]
    fn recognises_id_token_write() {
        assert!(has_id_token_write(
            "permissions:\n  id-token: write   # OIDC\n"
        ));
        assert!(has_id_token_write(
            "permissions: { id-token: write, contents: read }\n"
        ));
        assert!(has_id_token_write(
            "permissions: {contents: read, id-token: write}\n"
        ));
        assert!(!has_id_token_write("permissions:\n  id-token: read\n"));
        assert!(!has_id_token_write("# id-token: write\npermissions: {}\n"));
    }

    fn package(repository: Option<&str>, publish: Option<&[&str]>) -> Package {
        Package {
            name: "story_engine".into(),
            version: "0.1.0".into(),
            manifest_path: PathBuf::from("Cargo.toml"),
            repository: repository.map(str::to_owned),
            publish: publish.map(|p| p.iter().map(|r| (*r).to_owned()).collect()),
        }
    }

    #[test]
    fn checks_the_repository_against_the_remote() {
        let remote = Some("https://github.com/acme/story-engine");
        let status = |repository, remote| repository_status(package(repository, None), remote);
        assert_eq!(
            status(Some("https://github.com/acme/story-engine"), remote),
            Status::Pass
        );
        assert_eq!(
            status(Some("https://github.com/acme/other"), remote),
            Status::Fail
        );
        assert_eq!(status(None, remote), Status::Fail);
        assert_eq!(
            status(Some("https://github.com/acme/story-engine"), None),
            Status::Warn
        );
    }

    fn repository_status(package: Package, remote: Option<&str>) -> Status {
        repository(&package, remote, "cargo privatecrates init --registry acme").status
    }

    #[test]
    fn checks_publishing_is_restricted() {
        assert_eq!(
            publish_restricted(&package(None, Some(&["acme"])), "acme").status,
            Status::Pass
        );
        assert_eq!(
            publish_restricted(&package(None, None), "acme").status,
            Status::Warn
        );
        assert_eq!(
            publish_restricted(&package(None, Some(&["crates-io"])), "acme").status,
            Status::Warn
        );
    }
}
