//! Signing a developer in to PrivateCrates, shared by the credential provider (`cargo-credential-privatecrates`)
//! and `cargo privatecrates` (SPEC §3.1).
//!
//! - [`device`]: GitHub's device flow for the registry's reader App, and refreshing its tokens.
//! - [`store`]: where the tokens are kept: the operating system's keyring, or a file readable only by the user.

pub mod device;
pub mod store;

use std::time::{SystemTime, UNIX_EPOCH};

/// Why signing in, or reading or writing the stored tokens, failed: a message saying what to do.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}
