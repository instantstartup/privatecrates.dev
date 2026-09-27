//! The storage repository's history, from a local clone (`actions/checkout` with `fetch-depth: 0`).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, thiserror::Error)]
#[error("git {args}: {message}")]
pub struct GitError {
    args: String,
    message: String,
}

pub struct Git {
    dir: PathBuf,
}

impl Git {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_owned(),
        }
    }

    fn run(&self, args: &[&str]) -> Result<String, GitError> {
        let error = |message: String| GitError {
            args: args.join(" "),
            message,
        };
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .map_err(|e| error(e.to_string()))?;
        if !output.status.success() {
            return Err(error(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| error(e.to_string()))
    }

    /// Commits on the current branch's first-parent line, oldest first.
    pub fn first_parent_commits(&self) -> Result<Vec<String>, GitError> {
        Ok(self
            .run(&["log", "--first-parent", "--reverse", "--format=%H", "HEAD"])?
            .lines()
            .map(str::to_owned)
            .collect())
    }

    /// Paths a commit changed relative to its first parent.
    pub fn changed_paths(&self, commit: &str) -> Result<Vec<String>, GitError> {
        Ok(self
            .run(&[
                "diff-tree",
                "-r",
                "--root",
                "--no-commit-id",
                "--name-only",
                "-m",
                "--first-parent",
                commit,
            ])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// A file's content at a commit. An error if the file does not exist there.
    pub fn show(&self, commit: &str, path: &str) -> Result<String, GitError> {
        self.run(&["show", &format!("{commit}:{path}")])
    }
}
