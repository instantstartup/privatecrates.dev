//! Checks that a `.crate` is what its publish metadata says it is (SPEC §4.4, validation 5).

use std::io::Read;

use apollo_errors::Error;
use flate2::read::GzDecoder;
use miette::Diagnostic;

/// Upper bound on the decompressed size we are willing to read, against decompression bombs.
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST: u64 = 1024 * 1024;

#[derive(Debug, PartialEq, Eq, Error, Diagnostic)]
pub enum CrateFileError {
    #[error("the .crate file is not a valid gzip tar archive: {reason}")]
    #[diagnostic(code(crate_file::archive))]
    Archive { reason: String },
    #[error("the .crate file contains `{path}`, outside the `{prefix}` directory")]
    #[diagnostic(code(crate_file::outside_prefix))]
    OutsidePrefix { path: String, prefix: String },
    #[error("the .crate file has no {path}")]
    #[diagnostic(code(crate_file::no_manifest))]
    NoManifest { path: String },
    #[error("the .crate file's Cargo.toml is invalid: {reason}")]
    #[diagnostic(code(crate_file::manifest))]
    Manifest { reason: String },
    #[error("the .crate file's Cargo.toml says {found}, but the upload is for {expected}")]
    #[diagnostic(code(crate_file::mismatch))]
    Mismatch { found: String, expected: String },
}

/// What a `.crate`'s `Cargo.toml` says about it, for search (SPEC §9.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub description: Option<String>,
    pub keywords: Vec<String>,
}

pub fn check(bytes: &[u8], name: &str, version: &str) -> Result<(), CrateFileError> {
    read(bytes, name, version).map(drop)
}

/// Where Cargo says a `.crate` was packaged from: `.cargo_vcs_info.json`, which `cargo package` writes when the crate
/// is in a git checkout. It is the packager's claim, not proof; for a manual publish it is what we can record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vcs {
    /// The commit checked out, when Cargo recorded one.
    pub commit: Option<String>,
    /// Whether the checkout had uncommitted changes (`cargo publish --allow-dirty`).
    pub dirty: bool,
}

pub fn vcs(bytes: &[u8], name: &str, version: &str) -> Result<Vcs, CrateFileError> {
    #[derive(serde::Deserialize)]
    struct Info {
        git: Option<Git>,
    }
    #[derive(serde::Deserialize)]
    struct Git {
        sha1: String,
        #[serde(default)]
        dirty: bool,
    }
    let (_, info) = read(bytes, name, version)?;
    let git = info
        .and_then(|text| serde_json::from_str::<Info>(&text).ok())
        .and_then(|info| info.git)
        .filter(|git| git.sha1.len() == 40 && git.sha1.bytes().all(|b| b.is_ascii_hexdigit()));
    Ok(match git {
        Some(git) => Vcs {
            commit: Some(git.sha1.to_ascii_lowercase()),
            dirty: git.dirty,
        },
        None => Vcs::default(),
    })
}

/// Reads the description and keywords of a `.crate`, checking it as [`check`] does.
pub fn metadata(bytes: &[u8], name: &str, version: &str) -> Result<Metadata, CrateFileError> {
    let (package, _) = read(bytes, name, version)?;
    Ok(Metadata {
        description: package
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        keywords: package
            .get("keywords")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect(),
    })
}

/// The `[package]` table of a `.crate`'s `Cargo.toml`, once the archive's layout and the name and version check,
/// and its `.cargo_vcs_info.json`, if it has one.
fn read(
    bytes: &[u8],
    name: &str,
    version: &str,
) -> Result<(toml::Table, Option<String>), CrateFileError> {
    let prefix = format!("{name}-{version}/");
    let manifest_path = format!("{prefix}Cargo.toml");
    let vcs_path = format!("{prefix}.cargo_vcs_info.json");
    let mut vcs = None;
    let decoder = GzDecoder::new(bytes).take(MAX_UNPACKED);
    let mut archive = tar::Archive::new(decoder);
    let mut manifest = None;
    let entries = archive.entries().map_err(|e| CrateFileError::Archive {
        reason: e.to_string(),
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|e| CrateFileError::Archive {
            reason: e.to_string(),
        })?;
        let path = entry
            .path()
            .map_err(|e| CrateFileError::Archive {
                reason: e.to_string(),
            })?
            .to_string_lossy()
            .into_owned();
        let safe =
            path.starts_with(&prefix) && !path.split('/').any(|part| part == ".." || part == ".");
        if !safe {
            return Err(CrateFileError::OutsidePrefix { path, prefix });
        }
        if path == manifest_path {
            let mut text = String::new();
            entry
                .by_ref()
                .take(MAX_MANIFEST)
                .read_to_string(&mut text)
                .map_err(|e| CrateFileError::Manifest {
                    reason: e.to_string(),
                })?;
            manifest = Some(text);
        } else if path == vcs_path {
            let mut text = String::new();
            // Unreadable is the same as absent: the caller decides what that means.
            if entry
                .by_ref()
                .take(MAX_MANIFEST)
                .read_to_string(&mut text)
                .is_ok()
            {
                vcs = Some(text);
            }
        }
    }
    let manifest = manifest.ok_or(CrateFileError::NoManifest {
        path: manifest_path,
    })?;
    let mut manifest: toml::Table =
        toml::from_str(&manifest).map_err(|e| CrateFileError::Manifest {
            reason: e.to_string(),
        })?;
    let Some(toml::Value::Table(package)) = manifest.remove("package") else {
        return Err(CrateFileError::Manifest {
            reason: "no [package] table".into(),
        });
    };
    let found_name = package.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let found_version = package
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if found_name != name || found_version != version {
        return Err(CrateFileError::Mismatch {
            found: format!("{found_name} {found_version}"),
            expected: format!("{name} {version}"),
        });
    }
    Ok((package, vcs))
}

/// Builds a `.crate` for tests.
#[cfg(test)]
pub(crate) fn build(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, path, content.as_bytes())
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"story_engine\"\nversion = \"0.2.0\"\n";

    #[test]
    fn accepts_a_matching_crate() {
        let bytes = build(&[
            ("story_engine-0.2.0/Cargo.toml", MANIFEST),
            ("story_engine-0.2.0/src/lib.rs", ""),
        ]);
        assert_eq!(check(&bytes, "story_engine", "0.2.0"), Ok(()));
    }

    #[test]
    fn reads_description_and_keywords() {
        let bytes = build(&[(
            "story_engine-0.2.0/Cargo.toml",
            "[package]\nname = \"story_engine\"\nversion = \"0.2.0\"\ndescription = \"Stories\"\nkeywords = [\"narrative\", \"games\"]\n",
        )]);
        assert_eq!(
            metadata(&bytes, "story_engine", "0.2.0"),
            Ok(Metadata {
                description: Some("Stories".into()),
                keywords: vec!["narrative".into(), "games".into()],
            })
        );
        let bytes = build(&[("story_engine-0.2.0/Cargo.toml", MANIFEST)]);
        assert_eq!(
            metadata(&bytes, "story_engine", "0.2.0"),
            Ok(Metadata::default())
        );
    }

    #[test]
    fn rejects_a_mismatch() {
        let bytes = build(&[("story_engine-0.2.0/Cargo.toml", MANIFEST)]);
        assert!(matches!(
            check(&bytes, "story_engine", "0.3.0"),
            Err(CrateFileError::OutsidePrefix { .. })
        ));
        let bytes = build(&[(
            "story_engine-0.3.0/Cargo.toml",
            "[package]\nname = \"story_engine\"\nversion = \"0.2.0\"\n",
        )]);
        assert!(matches!(
            check(&bytes, "story_engine", "0.3.0"),
            Err(CrateFileError::Mismatch { .. })
        ));
    }

    #[test]
    fn rejects_paths_outside_the_crate_directory() {
        let bytes = build(&[
            ("story_engine-0.2.0/Cargo.toml", MANIFEST),
            ("other/evil.rs", ""),
        ]);
        assert!(matches!(
            check(&bytes, "story_engine", "0.2.0"),
            Err(CrateFileError::OutsidePrefix { .. })
        ));
    }

    #[test]
    fn rejects_garbage_and_missing_manifests() {
        assert!(matches!(
            check(b"not gzip", "a", "1.0.0"),
            Err(CrateFileError::Archive { .. })
        ));
        let bytes = build(&[("a-1.0.0/src/lib.rs", "")]);
        assert!(matches!(
            check(&bytes, "a", "1.0.0"),
            Err(CrateFileError::NoManifest { .. })
        ));
    }
}
