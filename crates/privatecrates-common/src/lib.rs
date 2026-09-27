//! Types and rules shared by the PrivateCrates server, credential provider and verifier.

pub mod audience;
pub mod index;
pub mod name;
pub mod storage;

use sha2::{Digest, Sha256};

/// The canonical index URL Cargo sends for dependencies from crates.io.
pub const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

/// The sparse form of the crates.io index URL, which Cargo may also send.
pub const CRATES_IO_SPARSE_INDEX: &str = "sparse+https://index.crates.io/";

/// Whether a dependency's `registry` value refers to crates.io.
pub fn is_crates_io(registry: &str) -> bool {
    registry == CRATES_IO_INDEX || registry == CRATES_IO_SPARSE_INDEX
}

/// Lowercase hex sha256 of `bytes`, the format Cargo uses for `cksum`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
