//! Crate name rules.

use std::fmt;

const MAX_LEN: usize = 64;

/// Names that cannot be published: Rust's own crates, and names that are special files on Windows.
const RESERVED: &[&str] = &[
    "alloc",
    "core",
    "proc_macro",
    "proc-macro",
    "std",
    "test",
    "con",
    "prn",
    "aux",
    "nul",
    "com1",
    "com2",
    "com3",
    "com4",
    "com5",
    "com6",
    "com7",
    "com8",
    "com9",
    "lpt1",
    "lpt2",
    "lpt3",
    "lpt4",
    "lpt5",
    "lpt6",
    "lpt7",
    "lpt8",
    "lpt9",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("crate names cannot be empty")]
    Empty,
    #[error("crate names can be at most {MAX_LEN} characters long")]
    TooLong,
    #[error("crate names must start with an ASCII letter")]
    BadStart,
    #[error("crate names may contain only ASCII letters, digits, `-` and `_`")]
    BadChar,
    #[error("`{0}` is a reserved name")]
    Reserved(String),
}

/// A valid crate name, as published (case preserved).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateName(String);

impl CrateName {
    pub fn parse(name: &str) -> Result<Self, NameError> {
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if name.len() > MAX_LEN {
            return Err(NameError::TooLong);
        }
        if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(NameError::BadStart);
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(NameError::BadChar);
        }
        if RESERVED.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(NameError::Reserved(name.to_owned()));
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name as it appears in index paths and release tags: lowercase.
    pub fn normalized(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl fmt::Display for CrateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_names() {
        for name in ["a", "story_engine", "story-engine", "Serde2"] {
            assert!(CrateName::parse(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn rejects_bad_names() {
        assert_eq!(CrateName::parse(""), Err(NameError::Empty));
        assert_eq!(CrateName::parse("1abc"), Err(NameError::BadStart));
        assert_eq!(CrateName::parse("_abc"), Err(NameError::BadStart));
        assert_eq!(CrateName::parse("a/b"), Err(NameError::BadChar));
        assert_eq!(CrateName::parse("a.b"), Err(NameError::BadChar));
        assert_eq!(CrateName::parse(&"a".repeat(65)), Err(NameError::TooLong));
        assert!(matches!(
            CrateName::parse("STD"),
            Err(NameError::Reserved(_))
        ));
        assert!(matches!(
            CrateName::parse("nul"),
            Err(NameError::Reserved(_))
        ));
    }
}
