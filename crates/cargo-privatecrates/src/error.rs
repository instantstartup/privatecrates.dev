use std::path::PathBuf;

/// Why a command failed. Each message says what to do next.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Auth(#[from] privatecrates_auth::Error),

    #[error("{0}")]
    Invalid(String),

    #[error(
        "setting up {org}'s registry needs an admin of {org} to accept the PrivateCrates terms ({version}). Read \
         them at {url}; to accept them on behalf of {org}, run the command again with `--accept-terms {version}`"
    )]
    TermsRequired {
        org: String,
        version: String,
        url: String,
    },

    #[error("not signed in to {apex}; run `cargo privatecrates login{domain_flag}` first")]
    NotSignedIn { apex: String, domain_flag: String },

    #[error("could not reach {url}: {source}")]
    Unreachable {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("{url} answered {status}: {detail}")]
    Api {
        url: String,
        status: u16,
        code: Option<String>,
        detail: String,
    },

    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{} is not valid TOML: {source}", path.display())]
    Toml {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },

    #[error("`{command}` failed: {detail}")]
    Command { command: String, detail: String },

    #[error("{} exists and differs; {detail}. Run again with --force to replace it", path.display())]
    Conflict { path: PathBuf, detail: String },
}

impl Error {
    /// The account API's error code, such as `account::admin_required`, for `--json` output.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Api { code, .. } => code.as_deref(),
            Self::TermsRequired { .. } => Some("account::terms_not_accepted"),
            _ => None,
        }
    }

    /// The terms to accept, for `--json` output, when that is what stopped the command.
    pub fn terms(&self) -> Option<serde_json::Value> {
        match self {
            Self::TermsRequired { version, url, .. } => Some(serde_json::json!({
                "version": version, "url": url, "accepted": false,
            })),
            _ => None,
        }
    }

    pub fn io(
        action: &'static str,
        path: impl Into<PathBuf>,
    ) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io {
            action,
            path,
            source,
        }
    }
}
