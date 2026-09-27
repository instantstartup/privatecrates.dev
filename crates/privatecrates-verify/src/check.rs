//! The checks of one published version, on what the caller fetched from GitHub: its release is present and
//! immutable, its `.crate` matches the index checksum, and its provenance was signed by GitHub for the right
//! repository, workflow, environment and triggering event. Whether the run's actor could create releases is not
//! checked: GitHub answers for current permissions only, not for when the version was published.
//!
//! No I/O happens here, so the verifier and the PrivateCrates server (for its compliance dashboard) run exactly the
//! same checks, each fetching the evidence its own way.

use jsonwebtoken::jwk::JwkSet;
use privatecrates_common::{sha256_hex, storage::Owner};

use crate::{Finding, Options, Severity, provenance, remote::Release};

/// How a crate stood when a version was appended to its index.
#[derive(Debug, Clone, Copy)]
pub struct Published<'a> {
    /// The crate's owners file at the time, if it had one.
    pub owner: Option<&'a Owner>,
    /// Whether the version was the crate's first.
    pub first: bool,
}

/// What GitHub holds for a version, fetched by the caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct Evidence<'a> {
    /// The version's published release, if there is one.
    pub release: Option<&'a Release>,
    /// The content of the release's provenance asset, when it has one (see [`Release::provenance_asset`]), and
    /// GitHub's OIDC keys to check it with.
    pub provenance: Option<(&'a [u8], &'a JwkSet)>,
    /// The `.crate` file's bytes. Needed only when GitHub reports no digest for it (see [`Release::crate_asset`]).
    pub crate_bytes: Option<&'a [u8]>,
}

/// Whether the `.crate` file matches the index checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    Matches,
    Mismatch,
    /// The `.crate` file is missing, or GitHub reports no digest for it and its bytes were not supplied.
    Missing,
}

/// What the version's provenance shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Signed by GitHub for the owning repository, an allowed workflow and trigger, and the required environment.
    Valid,
    Invalid(String),
    /// None, and the crate allowed manual publishing.
    Manual,
    /// None, although it was required: the crate's first version always needs it.
    Missing {
        first: bool,
    },
    /// Not checked: the release is missing, or the version's history is unknown.
    Unchecked,
}

/// The outcome of checking one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCheck {
    pub release: bool,
    /// The release is published and immutable.
    pub immutable: bool,
    pub digest: Digest,
    pub provenance: Provenance,
    /// Whether the version's history was known; without it provenance is not checked.
    pub history: bool,
}

impl VersionCheck {
    /// Whether the release carries a provenance asset, valid or not.
    pub fn has_provenance(&self) -> bool {
        matches!(self.provenance, Provenance::Valid | Provenance::Invalid(_))
    }

    /// What is wrong with the version, or worth a person's attention, in the order the checks run.
    pub fn issues(&self) -> Vec<Issue> {
        if !self.release {
            return vec![Issue::ReleaseMissing];
        }
        let mut issues = Vec::new();
        if !self.immutable {
            issues.push(Issue::ReleaseMutable);
        }
        match self.digest {
            Digest::Matches => {}
            Digest::Mismatch => issues.push(Issue::DigestMismatch),
            Digest::Missing => issues.push(Issue::CrateMissing),
        }
        if !self.history {
            issues.push(Issue::NotInHistory);
        }
        match &self.provenance {
            Provenance::Valid | Provenance::Unchecked => {}
            Provenance::Invalid(reason) => issues.push(Issue::ProvenanceInvalid(reason.clone())),
            Provenance::Manual => issues.push(Issue::ManualPublish),
            Provenance::Missing { first } => {
                issues.push(Issue::ProvenanceMissing { first: *first })
            }
        }
        issues
    }

    /// The issues as the verifier reports them.
    pub fn findings(&self, name: &str, version: &str) -> Vec<Finding> {
        self.issues()
            .into_iter()
            .map(|issue| Finding {
                severity: issue.severity(),
                subject: format!("{name} {version}"),
                message: issue.message(),
            })
            .collect()
    }

    /// Whether nothing is wrong with the version. A manual publish, where allowed, is not wrong.
    pub fn passed(&self) -> bool {
        self.issues()
            .iter()
            .all(|i| i.severity() != Severity::Error)
    }
}

/// One thing a version check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    ReleaseMissing,
    ReleaseMutable,
    DigestMismatch,
    /// The `.crate` file, or GitHub's digest of it, is missing.
    CrateMissing,
    /// The verifier's replay of the history did not reach the version.
    NotInHistory,
    ProvenanceInvalid(String),
    /// No provenance, although it was required; the crate's first version always needs it.
    ProvenanceMissing {
        first: bool,
    },
    /// No provenance, as the crate allows: published from a developer's machine.
    ManualPublish,
}

impl Issue {
    pub fn severity(&self) -> Severity {
        match self {
            Self::ManualPublish => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::ReleaseMissing => "its release is missing".into(),
            Self::ReleaseMutable => {
                "its release is not immutable, so its files could have been changed".into()
            }
            Self::DigestMismatch => "the .crate file does not match the index checksum".into(),
            Self::CrateMissing => "the .crate file or its digest is missing".into(),
            Self::NotInHistory => "the version is not in the replayed history".into(),
            Self::ProvenanceInvalid(reason) => format!("its provenance is invalid: {reason}"),
            Self::ProvenanceMissing { first: true } => {
                "the first version of a crate must have provenance, and this one has none".into()
            }
            Self::ProvenanceMissing { first: false } => {
                "it has no provenance, and its crate did not allow manual publishing".into()
            }
            Self::ManualPublish => {
                "published manually, without provenance; check that its publisher meant to".into()
            }
        }
    }
}

/// Checks one version, given its index checksum, how it was published and what GitHub holds for it.
pub fn check_version(
    name: &str,
    version: &str,
    cksum: &str,
    published: Option<Published<'_>>,
    evidence: &Evidence<'_>,
    options: &Options,
) -> VersionCheck {
    let Some(release) = evidence.release else {
        return VersionCheck {
            release: false,
            immutable: false,
            digest: Digest::Missing,
            provenance: Provenance::Unchecked,
            history: published.is_some(),
        };
    };
    let digest = match release.crate_asset(name, version) {
        None => Digest::Missing,
        Some(asset) => match (&asset.digest, evidence.crate_bytes) {
            (Some(digest), _) if *digest == format!("sha256:{cksum}") => Digest::Matches,
            (Some(_), _) => Digest::Mismatch,
            // The checksum itself is recomputed from the downloaded bytes only when GitHub reports no digest.
            (None, Some(bytes)) if sha256_hex(bytes) == cksum => Digest::Matches,
            (None, Some(_)) => Digest::Mismatch,
            (None, None) => Digest::Missing,
        },
    };
    let provenance = match (published, evidence.provenance) {
        (None, _) => Provenance::Unchecked,
        (Some(published), Some((jwt, jwks))) => {
            let expected = provenance::Expected {
                issuer: &options.oidc_issuer,
                base_url: &options.base_url,
                name,
                version,
                cksum,
                owner: published.owner,
            };
            match provenance::check(jwt, jwks, &expected) {
                Ok(()) => Provenance::Valid,
                Err(reason) => Provenance::Invalid(reason),
            }
        }
        (Some(published), None)
            if !published.first && published.owner.is_some_and(|o| o.allow_manual_publish) =>
        {
            Provenance::Manual
        }
        (Some(published), None) => Provenance::Missing {
            first: published.first,
        },
    };
    VersionCheck {
        release: true,
        immutable: !release.draft && release.immutable,
        digest,
        provenance,
        history: published.is_some(),
    }
}
