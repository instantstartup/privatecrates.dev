//! Types and rules shared by the PrivateCrates server, credential provider and verifier.

pub mod audience;
pub mod index;
pub mod install;
pub mod name;
pub mod storage;
pub mod trigger;
pub mod verifier;

use sha2::{Digest, Sha256};

/// The canonical index URL Cargo sends for dependencies from crates.io.
pub const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

/// The sparse form of the crates.io index URL, which Cargo may also send.
pub const CRATES_IO_SPARSE_INDEX: &str = "sparse+https://index.crates.io/";

/// The version of the PrivateCrates terms an organisation admin accepts now (docs/preview.md §2). The account API
/// publishes it with the terms' URL; nothing accepts it on anyone's behalf.
pub const TERMS_VERSION: &str = "private-preview-2026-09-28";

/// Whether a dependency's `registry` value refers to crates.io.
pub fn is_crates_io(registry: &str) -> bool {
    registry == CRATES_IO_INDEX || registry == CRATES_IO_SPARSE_INDEX
}

/// A registry name ("slug"): its hostname label and its name in Cargo. 1 to 63 lowercase letters, digits or
/// hyphens, not starting or ending with a hyphen.
pub fn slug_is_valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 63
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// Lowercase hex sha256 of `bytes`, the format Cargo uses for `cksum`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
