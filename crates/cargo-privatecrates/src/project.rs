//! The crate or workspace being configured: its packages (from `cargo metadata`) and its GitHub repository (from
//! the `origin` remote).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;

use crate::error::Error;

/// A crate repository or workspace.
pub struct Project {
    /// The workspace root, or the crate's directory.
    pub root: PathBuf,
    /// Whether the root manifest has a `[workspace]`.
    pub workspace: bool,
    pub packages: Vec<Package>,
}

#[derive(Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub manifest_path: PathBuf,
    /// As Cargo resolves it, so inherited from the workspace where the manifest says so.
    pub repository: Option<String>,
    /// The registries it may be published to: `None` for any, empty for none.
    pub publish: Option<Vec<String>>,
}

impl Project {
    /// The project containing `dir`, from `cargo metadata`.
    pub fn load(dir: &Path) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Metadata {
            packages: Vec<Package>,
            workspace_root: PathBuf,
        }
        if !dir.join("Cargo.toml").is_file() {
            return Err(Error::Invalid(format!(
                "{} has no Cargo.toml; run this in a crate or workspace",
                dir.display()
            )));
        }
        // Cargo reads `.cargo/config.toml` from the current directory, so run it there.
        let output = run(
            Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["metadata", "--no-deps", "--format-version", "1"])
                .current_dir(dir),
            "cargo metadata",
        )?;
        let metadata: Metadata = serde_json::from_str(&output).map_err(|e| Error::Command {
            command: "cargo metadata".into(),
            detail: e.to_string(),
        })?;
        let manifest = metadata.workspace_root.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(Error::io("read", &manifest))?;
        let workspace = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|source| Error::Toml {
                path: manifest,
                source,
            })?
            .contains_key("workspace");
        Ok(Self {
            root: metadata.workspace_root,
            workspace,
            packages: metadata.packages,
        })
    }
}

/// The repository the `origin` remote points at, as `https://github.com/owner/repo`. `None` when the directory is
/// not in a Git repository, or `origin` is missing or not on GitHub.
pub fn github_remote(dir: &Path) -> Option<String> {
    let url = run(
        Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(dir),
        "git remote get-url origin",
    )
    .ok()?;
    normalise_github_url(url.trim())
}

/// The top of the Git repository containing `dir`, where `.github/workflows` lives.
pub fn git_root(dir: &Path) -> Option<PathBuf> {
    run(
        Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(dir),
        "git rev-parse --show-toplevel",
    )
    .ok()
    .map(|root| PathBuf::from(root.trim()))
}

/// A GitHub repository URL in any of Git's forms (`https://github.com/o/r.git`, `git@github.com:o/r.git`,
/// `ssh://git@github.com/o/r`), as `https://github.com/o/r`.
pub fn normalise_github_url(url: &str) -> Option<String> {
    let path = if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else {
        let (scheme, rest) = url.split_once("://")?;
        if !matches!(scheme, "https" | "http" | "ssh" | "git") {
            return None;
        }
        // Drop any user (`git@`, `user:token@`) in the authority.
        let rest = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
        let rest = rest
            .strip_prefix("github.com")?
            .trim_start_matches(":443")
            .trim_start_matches(":22");
        rest.strip_prefix('/')?
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    (valid(owner) && valid(repo)).then(|| format!("https://github.com/{owner}/{repo}"))
}

/// Whether a package's `repository` names the GitHub repository `remote`: the repository itself, or a directory in
/// it (as a monorepo crate may link to).
pub fn repository_matches(repository: &str, remote: &str) -> bool {
    let repository = repository.trim_end_matches('/');
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    let (repository, remote) = (repository.to_ascii_lowercase(), remote.to_ascii_lowercase());
    repository == remote
        || repository
            .strip_prefix(&remote)
            .is_some_and(|rest| rest.starts_with("/tree/"))
}

/// Runs a command and returns its standard output.
fn run(command: &mut Command, name: &str) -> Result<String, Error> {
    let output = command.output().map_err(|e| Error::Command {
        command: name.into(),
        detail: e.to_string(),
    })?;
    if !output.status.success() {
        return Err(Error::Command {
            command: name.into(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_remotes_are_normalised() {
        let expected = Some("https://github.com/acme/story-engine".to_owned());
        for url in [
            "https://github.com/acme/story-engine",
            "https://github.com/acme/story-engine.git",
            "https://github.com/acme/story-engine/",
            "http://github.com/acme/story-engine",
            "https://alice:token@github.com/acme/story-engine.git",
            "git@github.com:acme/story-engine.git",
            "git@github.com:acme/story-engine",
            "ssh://git@github.com/acme/story-engine.git",
            "ssh://git@github.com:22/acme/story-engine.git",
            "git://github.com/acme/story-engine.git",
        ] {
            assert_eq!(normalise_github_url(url), expected, "{url}");
        }
        for url in [
            "https://gitlab.com/acme/story-engine",
            "https://github.com.evil.example/acme/story-engine",
            "git@gitlab.com:acme/story-engine.git",
            "https://github.com/acme",
            "https://github.com/acme/story-engine/extra",
            "/home/alice/story-engine",
            "",
        ] {
            assert_eq!(normalise_github_url(url), None, "{url}");
        }
    }

    #[test]
    fn repositories_match_their_remote() {
        let remote = "https://github.com/acme/story";
        assert!(repository_matches("https://github.com/acme/story", remote));
        assert!(repository_matches(
            "https://github.com/Acme/story.git",
            remote
        ));
        assert!(repository_matches(
            "https://github.com/acme/story/tree/main/story_core",
            remote
        ));
        assert!(!repository_matches(
            "https://github.com/acme/story-engine",
            remote
        ));
        assert!(!repository_matches(
            "https://github.com/other/story",
            remote
        ));
    }
}
