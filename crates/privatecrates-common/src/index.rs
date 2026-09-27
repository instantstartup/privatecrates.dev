//! The sparse registry index: path layout, line format, and conversion from Cargo's publish metadata.
//!
//! See the Cargo Book, "Registry Index" and "Registry Web API".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The index path of a crate, relative to the index root, following Cargo's layout. Names are lowercased.
pub fn path(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// The crate name an index path refers to, if the path is laid out correctly for that name.
pub fn name_from_path(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    if name.is_empty() || !name.is_ascii() {
        return None;
    }
    (self::path(name) == path.to_ascii_lowercase()).then_some(name)
}

/// One version of a crate in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexLine {
    pub name: String,
    pub vers: String,
    pub deps: Vec<IndexDep>,
    pub cksum: String,
    pub features: BTreeMap<String, Vec<String>>,
    pub yanked: bool,
    #[serde(default)]
    pub links: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features2: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexDep {
    /// The name used in the dependent's code: the rename if there is one.
    pub name: String,
    pub req: String,
    pub features: Vec<String>,
    pub optional: bool,
    pub default_features: bool,
    pub target: Option<String>,
    pub kind: String,
    /// The index URL of the dependency's registry; absent for this registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// The real crate name, when the dependency is renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// The JSON metadata Cargo sends with `cargo publish`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishMetadata {
    pub name: String,
    pub vers: String,
    pub deps: Vec<PublishDep>,
    pub features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub links: Option<String>,
    #[serde(default)]
    pub rust_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishDep {
    /// The real crate name.
    pub name: String,
    pub version_req: String,
    pub features: Vec<String>,
    pub optional: bool,
    pub default_features: bool,
    pub target: Option<String>,
    pub kind: String,
    /// The index URL of the dependency's registry; absent or null for the registry being published to.
    #[serde(default)]
    pub registry: Option<String>,
    /// The name used in `Cargo.toml`, when the dependency is renamed.
    #[serde(default)]
    pub explicit_name_in_toml: Option<String>,
}

impl IndexLine {
    /// The index line for a publish. Feature values using `dep:` or `?/` syntax go in `features2`, with `v: 2`, so
    /// that older Cargo versions ignore them rather than fail.
    pub fn from_publish(meta: &PublishMetadata, cksum: &str) -> Self {
        let (features, features2): (BTreeMap<_, _>, BTreeMap<_, _>) =
            meta.features.clone().into_iter().partition(|(_, values)| {
                !values
                    .iter()
                    .any(|v| v.starts_with("dep:") || v.contains("?/"))
            });
        let features2 = (!features2.is_empty()).then_some(features2);
        Self {
            name: meta.name.clone(),
            vers: meta.vers.clone(),
            deps: meta.deps.iter().map(IndexDep::from_publish).collect(),
            cksum: cksum.to_owned(),
            features,
            yanked: false,
            links: meta.links.clone(),
            v: features2.as_ref().map(|_| 2),
            features2,
            rust_version: meta.rust_version.clone(),
        }
    }
}

impl IndexDep {
    fn from_publish(dep: &PublishDep) -> Self {
        let (name, package) = match &dep.explicit_name_in_toml {
            Some(rename) => (rename.clone(), Some(dep.name.clone())),
            None => (dep.name.clone(), None),
        };
        Self {
            name,
            req: dep.version_req.clone(),
            features: dep.features.clone(),
            optional: dep.optional,
            default_features: dep.default_features,
            target: dep.target.clone(),
            kind: dep.kind.clone(),
            registry: dep.registry.clone(),
            package,
        }
    }
}

/// An index file: one JSON value per line. Lines are kept as raw JSON values so that rewriting a `yanked` flag
/// changes nothing else, including fields this code does not know about.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexFile {
    lines: Vec<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexFileError {
    #[error("index line {line} is not valid JSON: {source}")]
    Json {
        line: usize,
        source: serde_json::Error,
    },
    #[error("index line {line} has no string `vers`")]
    NoVersion { line: usize },
}

impl IndexFile {
    pub fn parse(text: &str) -> Result<Self, IndexFileError> {
        let lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, l)| {
                let value: serde_json::Value =
                    serde_json::from_str(l).map_err(|source| IndexFileError::Json {
                        line: i + 1,
                        source,
                    })?;
                if !value.get("vers").is_some_and(|v| v.is_string()) {
                    return Err(IndexFileError::NoVersion { line: i + 1 });
                }
                Ok(value)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { lines })
    }

    pub fn versions(&self) -> impl Iterator<Item = &str> {
        self.lines
            .iter()
            .filter_map(|l| l.get("vers").and_then(|v| v.as_str()))
    }

    pub fn contains_version(&self, version: &str) -> bool {
        self.versions().any(|v| same_version(v, version))
    }

    pub fn line(&self, version: &str) -> Option<&serde_json::Value> {
        self.lines.iter().find(|l| {
            l.get("vers")
                .and_then(|v| v.as_str())
                .is_some_and(|v| same_version(v, version))
        })
    }

    pub fn lines(&self) -> &[serde_json::Value] {
        &self.lines
    }

    pub fn append(&mut self, line: &IndexLine) {
        self.lines
            .push(serde_json::to_value(line).expect("index lines always serialise"));
    }

    /// Sets `yanked` on a version. Returns whether the version exists.
    pub fn set_yanked(&mut self, version: &str, yanked: bool) -> bool {
        let Some(line) = self.lines.iter_mut().find(|l| {
            l.get("vers")
                .and_then(|v| v.as_str())
                .is_some_and(|v| same_version(v, version))
        }) else {
            return false;
        };
        line["yanked"] = serde_json::Value::Bool(yanked);
        true
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&serde_json::to_string(line).expect("JSON values always serialise"));
            out.push('\n');
        }
        out
    }
}

/// Versions compare as semver where possible, so that `1.0.0+build` and `1.0.0` count as the same version, as they
/// do on crates.io.
fn same_version(a: &str, b: &str) -> bool {
    match (semver::Version::parse(a), semver::Version::parse(b)) {
        (Ok(a), Ok(b)) => {
            a.major == b.major && a.minor == b.minor && a.patch == b.patch && a.pre == b.pre
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_layout() {
        assert_eq!(path("a"), "1/a");
        assert_eq!(path("ab"), "2/ab");
        assert_eq!(path("abc"), "3/a/abc");
        assert_eq!(path("Story_Engine"), "st/or/story_engine");
        assert_eq!(path("serd"), "se/rd/serd");
    }

    #[test]
    fn name_from_path_requires_the_right_layout() {
        assert_eq!(name_from_path("st/or/story_engine"), Some("story_engine"));
        assert_eq!(name_from_path("3/a/abc"), Some("abc"));
        assert_eq!(name_from_path("1/a"), Some("a"));
        assert_eq!(name_from_path("xx/or/story_engine"), None);
        assert_eq!(name_from_path("story_engine"), None);
        assert_eq!(name_from_path("st/or/"), None);
    }

    /// The example line from the Cargo Book's "Registry Index" chapter round-trips unchanged.
    #[test]
    fn cargo_book_example_round_trips() {
        let example = r#"{"name":"foo","vers":"0.1.0","deps":[{"name":"rand","req":"^0.6","features":["i128_support"],"optional":false,"default_features":true,"target":null,"kind":"normal","registry":null,"package":null}],"cksum":"d867001db0e2b6e0496f9fac96930e2d42233ecd3ca0413e0753d4c7695d289c","features":{"extras":["rand/simd_support"]},"yanked":false,"links":null,"v":2,"features2":{"serde":["dep:serde"]},"rust_version":"1.60"}"#;
        let line: IndexLine = serde_json::from_str(example).unwrap();
        assert_eq!(line.features2.as_ref().unwrap()["serde"], vec!["dep:serde"]);
        let again: IndexLine =
            serde_json::from_str(&serde_json::to_string(&line).unwrap()).unwrap();
        assert_eq!(line, again);
    }

    fn metadata() -> PublishMetadata {
        serde_json::from_value(serde_json::json!({
            "name": "story_engine",
            "vers": "0.2.0",
            "deps": [
                {
                    "name": "serde", "version_req": "^1", "features": ["derive"], "optional": true,
                    "default_features": true, "target": null, "kind": "normal",
                    "registry": "https://github.com/rust-lang/crates.io-index", "explicit_name_in_toml": null
                },
                {
                    "name": "story_core", "version_req": "^0.1", "features": [], "optional": false,
                    "default_features": true, "target": null, "kind": "normal",
                    "explicit_name_in_toml": "core_api"
                }
            ],
            "features": { "default": ["std"], "std": [], "serde": ["dep:serde"] },
            "authors": [], "description": "Stories", "documentation": null, "homepage": null,
            "readme": null, "readme_file": null, "keywords": [], "categories": [],
            "license": "MIT", "license_file": null, "repository": "https://github.com/acme/story-engine",
            "badges": {}, "links": null, "rust_version": "1.80"
        }))
        .unwrap()
    }

    #[test]
    fn publish_metadata_becomes_an_index_line() {
        let line = IndexLine::from_publish(&metadata(), "abc");
        assert_eq!(line.cksum, "abc");
        assert_eq!(line.v, Some(2));
        assert!(line.features.contains_key("std"));
        assert!(!line.features.contains_key("serde"));
        assert_eq!(line.features2.as_ref().unwrap()["serde"], vec!["dep:serde"]);
        assert_eq!(
            line.deps[0].registry.as_deref(),
            Some(crate::CRATES_IO_INDEX)
        );
        assert_eq!(line.deps[0].req, "^1");
        assert_eq!(line.deps[1].name, "core_api");
        assert_eq!(line.deps[1].package.as_deref(), Some("story_core"));
        assert_eq!(line.deps[1].registry, None);
        assert_eq!(line.rust_version.as_deref(), Some("1.80"));
    }

    #[test]
    fn no_features2_means_no_v() {
        let mut meta = metadata();
        meta.features.remove("serde");
        let line = IndexLine::from_publish(&meta, "abc");
        assert_eq!(line.v, None);
        assert_eq!(line.features2, None);
        let json = serde_json::to_string(&line).unwrap();
        assert!(!json.contains("features2"));
    }

    #[test]
    fn yank_changes_only_the_flag() {
        let text = concat!(
            r#"{"name":"a","vers":"0.1.0","deps":[],"cksum":"x","features":{},"yanked":false,"links":null,"future":1}"#,
            "\n",
            r#"{"name":"a","vers":"0.2.0","deps":[],"cksum":"y","features":{},"yanked":false,"links":null}"#,
            "\n"
        );
        let mut file = IndexFile::parse(text).unwrap();
        assert!(file.set_yanked("0.1.0", true));
        assert!(!file.set_yanked("9.9.9", true));
        let rendered = file.render();
        assert!(rendered.contains(r#""future":1"#));
        let again = IndexFile::parse(&rendered).unwrap();
        assert_eq!(again.line("0.1.0").unwrap()["yanked"], true);
        assert_eq!(again.line("0.2.0").unwrap()["yanked"], false);
    }

    #[test]
    fn build_metadata_does_not_make_a_new_version() {
        let file = IndexFile::parse(
            r#"{"name":"a","vers":"1.0.0+one","deps":[],"cksum":"x","features":{},"yanked":false,"links":null}"#,
        )
        .unwrap();
        assert!(file.contains_version("1.0.0"));
        assert!(file.contains_version("1.0.0+two"));
        assert!(!file.contains_version("1.0.1"));
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(IndexFile::parse("not json").is_err());
        assert!(IndexFile::parse(r#"{"name":"a"}"#).is_err());
    }
}
