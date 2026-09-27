//! Verifies everything PrivateCrates wrote to a storage repository (SPEC §10.3), so that customers need not trust
//! the service: "don't trust us, verify us".
//!
//! History comes from a local clone; releases, commit signatures and provenance come from GitHub. The checks of each
//! version are in [`check`], free of I/O, so that the PrivateCrates server runs the same ones for its compliance
//! dashboard.

pub mod check;
pub mod git;
pub mod provenance;
pub mod remote;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use privatecrates_common::{
    index::IndexFile,
    storage::{INDEX_DIR, OWNERS_DIR, Owner, SETTINGS_PATH, owner_name, release_tag},
};
use serde::{Deserialize, Serialize};

use crate::{
    check::{Evidence, Published, check_version},
    git::Git,
    remote::Remote,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Something that should never happen: possible tampering.
    Error,
    /// Worth a person's attention, such as a version published without provenance.
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub subject: String,
    pub message: String,
}

/// What has been verified already, so later runs check only what is new.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct State {
    /// The last commit whose changes were verified.
    pub commit: Option<String>,
    /// `name@version` for every version whose release and provenance were verified.
    pub versions: BTreeSet<String>,
}

pub struct Options {
    /// The registry's base URL, e.g. `https://acme.privatecrates.dev`; provenance audiences name it.
    pub base_url: String,
    /// The login of the storage App's bot, e.g. `privatecrates-storage[bot]`.
    pub storage_app_login: String,
    /// The Actions OIDC issuer.
    pub oidc_issuer: String,
}

pub struct Report {
    pub findings: Vec<Finding>,
    pub state: State,
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error(transparent)]
    Git(#[from] git::GitError),
    #[error(transparent)]
    Remote(#[from] remote::RemoteError),
}

/// One file changed by one commit.
struct Change<'a> {
    /// Whether the commit is new since the last run, so its changes must be checked.
    checking: bool,
    by_app: bool,
    commit: &'a str,
    path: &'a str,
    old: Option<&'a str>,
    new: Option<&'a str>,
}

/// The owners of a crate when a version was appended, and whether it was the crate's first version.
struct Appended {
    owner: Option<Owner>,
    first: bool,
}

struct Verifier<'a> {
    remote: &'a dyn Remote,
    options: &'a Options,
    findings: Vec<Finding>,
}

impl Verifier<'_> {
    fn report(
        &mut self,
        severity: Severity,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.findings.push(Finding {
            severity,
            subject: subject.into(),
            message: message.into(),
        });
    }
}

pub fn verify(
    git: &Git,
    remote: &dyn Remote,
    options: &Options,
    state: &State,
) -> Result<Report, VerifyError> {
    let mut v = Verifier {
        remote,
        options,
        findings: Vec::new(),
    };
    let commits = git.first_parent_commits()?;
    if let Some(previous) = &state.commit
        && !commits.contains(previous)
    {
        v.report(
            Severity::Error,
            "history",
            format!("the previously verified commit {previous} is no longer in the history: it was rewritten"),
        );
    }
    let mut checking = state.commit.as_ref().is_none_or(|c| !commits.contains(c));
    // Current content of the files that matter, as the history is replayed.
    let mut files: HashMap<String, String> = HashMap::new();
    let mut appended: BTreeMap<String, Appended> = BTreeMap::new();

    for commit in &commits {
        let mut changes: Vec<(String, Option<String>)> = git
            .changed_paths(commit)?
            .into_iter()
            .filter(|p| p.starts_with(INDEX_DIR) || p.starts_with(OWNERS_DIR) || p == SETTINGS_PATH)
            .map(|p| {
                let content = git.show(commit, &p).ok();
                (p, content)
            })
            .collect();
        // Owners files first, so an index line is attributed to the owners in force after its commit.
        changes.sort_by_key(|(p, _)| !p.starts_with(OWNERS_DIR));
        let by_app = if checking && !changes.is_empty() {
            let info = remote.commit(commit)?;
            info.verified && info.login.as_deref() == Some(&options.storage_app_login)
        } else {
            false
        };
        let short = &commit[..commit.len().min(12)];
        for (path, new) in changes {
            let old = files.get(&path).cloned();
            if path.starts_with(INDEX_DIR) {
                let change = Change {
                    checking,
                    by_app,
                    commit: short,
                    path: &path,
                    old: old.as_deref(),
                    new: new.as_deref(),
                };
                v.index_change(&change, &files, &mut appended);
            } else if checking {
                v.config_change(by_app, short, &path, old.is_some(), new.is_some());
            }
            match new {
                Some(content) => files.insert(path, content),
                None => files.remove(&path),
            };
        }
        if state.commit.as_ref() == Some(commit) {
            checking = true;
        }
    }

    let mut versions = state.versions.clone();
    let mut names = BTreeSet::new();
    for (path, content) in files.iter().filter(|(p, _)| p.starts_with(INDEX_DIR)) {
        let Ok(index) = IndexFile::parse(content) else {
            v.report(Severity::Error, path.clone(), "the index file is not valid");
            continue;
        };
        for line in index.lines() {
            let (Some(name), Some(version), Some(cksum)) = (
                line["name"].as_str(),
                line["vers"].as_str(),
                line["cksum"].as_str(),
            ) else {
                v.report(
                    Severity::Error,
                    path.clone(),
                    "an index line lacks name, vers or cksum",
                );
                continue;
            };
            names.insert(name.to_owned());
            let key = format!("{name}@{version}");
            if versions.contains(&key) {
                continue;
            }
            let errors_before = v.errors();
            let when = appended.get(&key);
            v.version(name, version, cksum, when)?;
            if v.errors() == errors_before {
                versions.insert(key);
            }
        }
    }
    for name in names {
        if remote.crates_io_exists(&name)? {
            v.report(
                Severity::Warning,
                name.clone(),
                format!(
                    "a crate named {name} exists on crates.io; a dependency that omits `registry` would get that one"
                ),
            );
        }
    }
    let mut findings = v.findings;
    findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.subject.cmp(&b.subject)));
    Ok(Report {
        findings,
        state: State {
            commit: commits.last().cloned(),
            versions,
        },
    })
}

impl Verifier<'_> {
    fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    fn index_change(
        &mut self,
        change: &Change<'_>,
        files: &HashMap<String, String>,
        appended: &mut BTreeMap<String, Appended>,
    ) {
        let &Change {
            checking,
            by_app,
            commit,
            path,
            old,
            new,
        } = change;
        let parse = |text: Option<&str>| text.map(IndexFile::parse).transpose();
        let (old_file, new_file) = match (parse(old), parse(new)) {
            (Ok(o), Ok(n)) => (o, n),
            _ => {
                if checking {
                    self.report(
                        Severity::Error,
                        path,
                        format!("commit {commit} left an invalid index file"),
                    );
                }
                return;
            }
        };
        let old_lines = old_file
            .as_ref()
            .map(|f| f.lines().to_vec())
            .unwrap_or_default();
        let Some(new_file) = new_file else {
            if checking {
                self.report(
                    Severity::Error,
                    path,
                    format!("commit {commit} deleted the index file"),
                );
            }
            return;
        };
        let new_lines = new_file.lines();
        if checking {
            if !by_app {
                self.report(
                    Severity::Error,
                    path,
                    format!(
                        "commit {commit} changed the index but was not made by the storage App"
                    ),
                );
            }
            let without_yank = |l: &serde_json::Value| {
                let mut l = l.clone();
                if let Some(o) = l.as_object_mut() {
                    o.remove("yanked");
                }
                l
            };
            let kept = new_lines.len() >= old_lines.len()
                && old_lines
                    .iter()
                    .zip(new_lines)
                    .all(|(o, n)| without_yank(o) == without_yank(n));
            if !kept {
                self.report(
                    Severity::Error,
                    path,
                    format!("commit {commit} removed or rewrote published index lines; only appends and yanks are allowed"),
                );
            }
        }
        for (i, line) in new_lines.iter().enumerate().skip(old_lines.len()) {
            let (Some(name), Some(version)) = (line["name"].as_str(), line["vers"].as_str()) else {
                continue;
            };
            let owner = files
                .get(&privatecrates_common::storage::owner_path(name))
                .and_then(|t| toml::from_str(t).ok());
            appended.insert(
                format!("{name}@{version}"),
                Appended {
                    owner,
                    first: i == 0,
                },
            );
        }
    }

    fn config_change(
        &mut self,
        by_app: bool,
        commit: &str,
        path: &str,
        existed: bool,
        exists: bool,
    ) {
        if !by_app {
            self.report(
                Severity::Info,
                path,
                format!("changed by a person in commit {commit}"),
            );
            return;
        }
        // The App creates an owners file at a crate's first publish, and `privatecrates.toml` when an organisation
        // is set up; it never changes or deletes either.
        let creating = !existed && exists && (owner_name(path).is_some() || path == SETTINGS_PATH);
        if !creating {
            self.report(
                Severity::Error,
                path,
                format!(
                    "the storage App changed {path} in commit {commit}; it may only create owners files at a crate's \
                     first publish and privatecrates.toml at sign-up"
                ),
            );
        }
    }

    fn version(
        &mut self,
        name: &str,
        version: &str,
        cksum: &str,
        appended: Option<&Appended>,
    ) -> Result<(), VerifyError> {
        let release = self.remote.release(&release_tag(name, version))?;
        let published = appended.map(|a| Published {
            owner: a.owner.as_ref(),
            first: a.first,
        });
        let mut provenance = None;
        let mut crate_bytes = None;
        if let Some(release) = &release {
            if published.is_some()
                && let Some(asset) = release.provenance_asset(name, version)
            {
                provenance = Some((self.remote.asset(asset.id)?, self.remote.jwks()?));
            }
            if let Some(asset) = release
                .crate_asset(name, version)
                .filter(|a| a.digest.is_none())
            {
                crate_bytes = Some(self.remote.asset(asset.id)?);
            }
        }
        let evidence = Evidence {
            release: release.as_ref(),
            provenance: provenance
                .as_ref()
                .map(|(jwt, jwks)| (jwt.as_slice(), jwks)),
            crate_bytes: crate_bytes.as_deref(),
        };
        let check = check_version(name, version, cksum, published, &evidence, self.options);
        self.findings.extend(check.findings(name, version));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
