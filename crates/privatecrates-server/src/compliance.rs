//! The compliance dashboard (docs/trust-and-status.md §4): the integrity of every published version, who may publish
//! each crate, the registry's risks and its audit trail. Built from the storage repository with the storage App's
//! existing read access, for any member of the organisation.
//!
//! Integrity runs `privatecrates-verify`'s own per-version checks, on evidence fetched through our GitHub client.
//! The customer's verifier, run in their own repository, remains the independent check.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
use bytes::Bytes;
use jsonwebtoken::jwk::JwkSet;
use privatecrates_common::{
    index::{IndexFile, name_from_path},
    storage::{INDEX_DIR, OWNERS_DIR, SETTINGS_PATH, owner_path, release_tag},
};
use privatecrates_verify::{
    Options,
    check::{Digest, Evidence, Issue, Provenance, Published, VersionCheck, check_version},
    remote,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{sync::Semaphore, task::JoinSet};

use crate::{
    AppState,
    account::member,
    error::ApiError,
    github::{self, Commit},
    session::Session,
    tenant::Tenant,
};

/// A report is rebuilt at most this often; a push to the storage repository discards it at once.
pub const REPORT_TTL: Duration = Duration::from_secs(5 * 60);
/// Audit entries per page of `GET /api/orgs/{org}/compliance`.
pub const AUDIT_PAGE: usize = 50;
/// Versions checked at once when a report is built.
const CONCURRENCY: usize = 8;
/// Where workflows live; one that runs the verifier mentions it.
const WORKFLOWS_DIR: &str = ".github/workflows/";
use privatecrates_common::verifier::VERIFIER;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/orgs/{org}/compliance", get(compliance))
        .route("/api/orgs/{org}/compliance/audit.csv", get(audit_csv))
}

/// Reports per tenant, and the versions that passed.
pub struct Compliance {
    /// Storage repository ID → its latest report.
    reports: moka::future::Cache<u64, Arc<Report>>,
    /// (storage repository ID, path or "" for every commit) → its commits, newest first: each report asks GitHub
    /// only for those newer than the first. In memory only: after a restart the first report lists them all again.
    commits: moka::future::Cache<(u64, &'static str), Arc<Vec<Commit>>>,
}

impl Default for Compliance {
    fn default() -> Self {
        Self {
            reports: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(REPORT_TTL)
                .build(),
            commits: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_idle(Duration::from_secs(24 * 60 * 60))
                .build(),
        }
    }
}

impl Compliance {
    /// Discards a tenant's report, when its storage repository changed.
    pub async fn invalidate(&self, storage_repo_id: u64) {
        self.reports.invalidate(&storage_repo_id).await;
    }
}

#[derive(Serialize)]
struct Report {
    org: OrgRef,
    generated_at: String,
    integrity: Integrity,
    publishers: Vec<Publisher>,
    risks: Vec<Risk>,
    /// Newest first; served a page at a time.
    #[serde(skip)]
    audit: Vec<AuditEntry>,
}

#[derive(Serialize)]
struct OrgRef {
    id: u64,
    login: String,
}

#[derive(Serialize, Default)]
struct Integrity {
    versions: usize,
    /// Releases that are published and immutable.
    immutable: usize,
    digest_matches: usize,
    /// Versions with valid provenance.
    provenance: usize,
    /// Versions published manually, without provenance, by crates that allow it.
    manual: usize,
    /// Every issue the checks found, manual publishes included.
    problems: Vec<Problem>,
}

#[derive(Serialize)]
struct Problem {
    code: &'static str,
    #[serde(rename = "crate")]
    krate: String,
    version: Option<String>,
    detail: String,
}

#[derive(Serialize)]
struct Publisher {
    #[serde(rename = "crate")]
    krate: String,
    repository: String,
    workflows: Vec<String>,
    environment: Option<String>,
    manual_publish: bool,
}

#[derive(Serialize)]
struct Risk {
    code: &'static str,
    #[serde(rename = "crate", skip_serializing_if = "Option::is_none")]
    krate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    /// For a repository's own setting in privatecrates.toml.
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Publish,
    Yank,
    Unyank,
    OwnersChange,
    SettingsChange,
    /// A person changed the index directly, which only the storage App should do.
    IndexChange,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Yank => "yank",
            Self::Unyank => "unyank",
            Self::OwnersChange => "owners_change",
            Self::SettingsChange => "settings_change",
            Self::IndexChange => "index_change",
        }
    }
}

#[derive(Serialize)]
struct AuditEntry {
    at: Option<String>,
    action: Action,
    #[serde(rename = "crate")]
    krate: Option<String>,
    version: Option<String>,
    by: String,
    /// For a publish: whether the release has a provenance asset.
    provenance: Option<bool>,
    commit: String,
}

#[derive(Deserialize)]
struct AuditQuery {
    /// A commit sha from the audit: the page starts after it.
    before: Option<String>,
}

async fn compliance(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Value>, ApiError> {
    let (report, visible) = report(&state, &session, &org).await?;
    let audit = visible.audit(&report.audit);
    let entries = after(&audit, query.before.as_deref());
    let page = &entries[..entries.len().min(AUDIT_PAGE)];
    let next = (entries.len() > page.len())
        .then(|| page.last().map(|e| e.commit.clone()))
        .flatten();
    let mut doc = serde_json::to_value(&*report).map_err(|e| ApiError::internal(e.to_string()))?;
    visible.filter(&mut doc);
    doc["audit"] = serde_json::to_value(page).map_err(|e| ApiError::internal(e.to_string()))?;
    doc["audit_next_before"] = next.into();
    Ok(Json(doc))
}

/// The whole audit trail (or what is older than `before`), for auditors' spreadsheets.
async fn audit_csv(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(org): Path<String>,
    Query(query): Query<AuditQuery>,
) -> Result<Response, ApiError> {
    let (report, visible) = report(&state, &session, &org).await?;
    let audit = visible.audit(&report.audit);
    let entries = after(&audit, query.before.as_deref());
    let disposition = format!(
        "attachment; filename=\"{}-audit.csv\"",
        report.org.login.to_ascii_lowercase()
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv(entries),
    )
        .into_response())
}

/// The entries after the one for commit `before`, or all of them; none if `before` is not in the audit.
fn after<'a, 'b>(entries: &'b [&'a AuditEntry], before: Option<&str>) -> &'b [&'a AuditEntry] {
    match before {
        None => entries,
        Some(sha) => entries
            .iter()
            .position(|e| e.commit == sha)
            .map_or(&[][..], |i| &entries[i + 1..]),
    }
}

fn csv(entries: &[&AuditEntry]) -> String {
    let mut out = String::from("at,action,crate,version,by,provenance,commit\r\n");
    for e in entries {
        let provenance = e.provenance.map(|p| p.to_string()).unwrap_or_default();
        let fields = [
            e.at.as_deref().unwrap_or_default(),
            e.action.as_str(),
            e.krate.as_deref().unwrap_or_default(),
            e.version.as_deref().unwrap_or_default(),
            &e.by,
            &provenance,
            &e.commit,
        ];
        let row: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

/// Quotes a field when it needs it (RFC 4180), and defuses one a spreadsheet would run as a formula: author names
/// and commit messages come from whoever made the commit.
fn csv_field(field: &str) -> String {
    let field = if field.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{field}")
    } else {
        field.to_owned()
    };
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field
    }
}

/// The crates a member may see in the report: those whose repository they can read, as the registry decides for
/// the index. Entries about no crate (settings changes, say) are shown to every member.
struct Visible(HashSet<String>);

impl Visible {
    fn shows(&self, krate: Option<&str>) -> bool {
        krate.is_none_or(|k| self.0.contains(&k.to_ascii_lowercase()))
    }

    fn audit<'a>(&self, entries: &'a [AuditEntry]) -> Vec<&'a AuditEntry> {
        entries
            .iter()
            .filter(|e| self.shows(e.krate.as_deref()))
            .collect()
    }

    /// Drops publishers, problems and risks about crates the member cannot read. The totals stay the registry's.
    fn filter(&self, doc: &mut Value) {
        let keep = |items: &mut Value| {
            if let Some(items) = items.as_array_mut() {
                items.retain(|item| self.shows(item["crate"].as_str()));
            }
        };
        keep(&mut doc["publishers"]);
        keep(&mut doc["risks"]);
        keep(&mut doc["integrity"]["problems"]);
    }
}

/// The organisation's report (cached, or built now), and what of it this member may see.
async fn report(
    state: &Arc<AppState>,
    session: &Session,
    org: &str,
) -> Result<(Arc<Report>, Visible), ApiError> {
    let membership = member(state, session, org).await?;
    let tenant = state
        .tenants
        .by_org(membership.organization.id)
        .ok_or_else(|| ApiError::ComplianceNotSetUp {
            org: membership.organization.login.clone(),
            account_url: state.config.account_url(),
        })?;
    let visible = visible(state, session, &tenant).await?;
    if let Some(report) = state.compliance.reports.get(&tenant.storage_repo_id).await {
        return Ok((report, visible));
    }
    let report = Arc::new(build(state, &tenant).await?);
    state
        .compliance
        .reports
        .insert(tenant.storage_repo_id, report.clone())
        .await;
    Ok((report, visible))
}

async fn visible(
    state: &AppState,
    session: &Session,
    tenant: &Tenant,
) -> Result<Visible, ApiError> {
    let resolver = crate::routes::resolver(state, tenant);
    let caller = resolver
        .caller(crate::auth::Credential::AppUser(session.token.clone()))
        .await?;
    let mut crates = HashSet::new();
    for (name, owner) in tenant.owners() {
        if resolver.can_read(&caller, owner.repository_id).await? {
            crates.insert(name);
        }
    }
    Ok(Visible(crates))
}

/// A version in the index.
#[derive(Clone)]
struct Version {
    name: String,
    version: String,
    cksum: String,
    /// The first line of its crate's index file.
    first: bool,
}

async fn build(state: &Arc<AppState>, tenant: &Arc<Tenant>) -> Result<Report, ApiError> {
    let token = tenant.storage_token(&state.gh).await?;
    let mut integrity = Integrity::default();
    let versions = versions(state, tenant, &mut integrity.problems).await?;
    let checks = checks(state, tenant, &token, &versions).await?;

    let mut risks = Vec::new();
    // Lowercase name → the name as published.
    let mut names = HashMap::new();
    for (v, check) in versions.iter().zip(&checks) {
        names.insert(v.name.to_ascii_lowercase(), v.name.clone());
        integrity.versions += 1;
        integrity.immutable += usize::from(check.release && check.immutable);
        integrity.digest_matches += usize::from(check.digest == Digest::Matches);
        integrity.provenance += usize::from(check.provenance == Provenance::Valid);
        integrity.manual += usize::from(check.provenance == Provenance::Manual);
        integrity
            .problems
            .extend(check.issues().into_iter().map(|issue| Problem {
                code: problem_code(&issue),
                krate: v.name.clone(),
                version: Some(v.version.clone()),
                detail: issue.message(),
            }));
        if matches!(
            check.provenance,
            Provenance::Manual | Provenance::Missing { .. }
        ) {
            risks.push(Risk {
                code: "missing_provenance",
                krate: Some(v.name.clone()),
                version: Some(v.version.clone()),
                repository: None,
                detail: "published without provenance, so nothing shows which workflow built it"
                    .into(),
            });
        }
    }

    let mut owners = tenant.owners();
    owners.sort_by(|a, b| a.0.cmp(&b.0));
    let display = |lower: &str| {
        names
            .get(lower)
            .cloned()
            .unwrap_or_else(|| lower.to_owned())
    };
    let publishers: Vec<Publisher> = owners
        .iter()
        .map(|(name, owner)| Publisher {
            krate: display(name),
            repository: owner.repository.clone(),
            workflows: owner.publish_workflows.clone(),
            environment: owner.publish_environment.clone(),
            manual_publish: owner.allow_manual_publish
                || tenant.settings.allows_manual_publish(&[&owner.repository]),
        })
        .collect();
    let settings = &tenant.settings;
    if settings.allow_manual_publish {
        risks.push(Risk {
            code: "manual_publish_allowed",
            krate: None,
            version: None,
            repository: None,
            detail: "privatecrates.toml allows crates to be published from a developer's machine by default, \
                     first versions included, without provenance"
                .into(),
        });
    } else {
        risks.extend(
            settings
                .repositories
                .iter()
                .filter(|(_, r)| r.allow_manual_publish == Some(true))
                .map(|(name, _)| Risk {
                    code: "manual_publish_allowed",
                    krate: None,
                    version: None,
                    repository: Some(name.clone()),
                    detail: format!(
                        "privatecrates.toml allows crates in {name} to be published from a developer's machine, \
                         first versions included, without provenance"
                    ),
                }),
        );
    }
    risks.extend(
        owners
            .iter()
            .filter(|(_, owner)| {
                owner.allow_manual_publish && !settings.allows_manual_publish(&[&owner.repository])
            })
            .map(|(name, _)| Risk {
                code: "manual_publish_allowed",
                krate: Some(display(name)),
                version: None,
                repository: None,
                detail: "versions may be published from a developer's machine, without provenance"
                    .into(),
            }),
    );
    let mut sorted: Vec<&String> = names.values().collect();
    sorted.sort();
    for name in sorted {
        match state.crates_io.exists(name).await {
            Ok(true) => risks.push(Risk {
                code: "name_clash",
                krate: Some(name.clone()),
                version: None,
                repository: None,
                detail: "a crate with this name exists on crates.io; a dependency that omits `registry` would get \
                         that one"
                    .into(),
            }),
            Ok(false) => {}
            // The dashboard still answers; the next report asks again.
            Err(e) => log::warn!(error:% = e; "checking a name on crates.io failed"),
        }
    }
    if !has_verify_workflow(state, tenant).await? {
        risks.push(Risk {
            code: "no_verify_workflow",
            krate: None,
            version: None,
            repository: None,
            detail: format!(
                "no workflow in {} runs {VERIFIER}, the independent check of everything this service writes; see \
                 {}/docs/verify",
                tenant.storage_repo,
                state.config.apex_url()
            ),
        });
    }

    let provenance: HashMap<(String, String), bool> = versions
        .iter()
        .zip(&checks)
        .map(|(v, check)| {
            (
                (v.name.to_ascii_lowercase(), v.version.clone()),
                check.has_provenance(),
            )
        })
        .collect();
    let audit = audit(state, tenant, &token, &provenance).await?;
    Ok(Report {
        org: OrgRef {
            id: tenant.org_id,
            login: tenant.org_login.clone(),
        },
        generated_at: OffsetDateTime::now_utc()
            .replace_nanosecond(0)
            .ok()
            .and_then(|t| t.format(&Rfc3339).ok())
            .unwrap_or_default(),
        integrity,
        publishers,
        risks,
        audit,
    })
}

/// The contract's code for an issue the verifier's checks found (docs/website-api.md).
fn problem_code(issue: &Issue) -> &'static str {
    match issue {
        Issue::ReleaseMissing => "release_missing",
        Issue::ReleaseMutable => "release_mutable",
        Issue::DigestMismatch => "digest_mismatch",
        Issue::CrateMissing => "crate_missing",
        // Only the verifier's replay can miss a version; the server always passes how it was published.
        Issue::NotInHistory => "not_in_history",
        Issue::ProvenanceInvalid(_) => "provenance_invalid",
        Issue::ProvenanceMissing { .. } => "provenance_missing",
        Issue::ManualPublish => "manual_publish",
    }
}

/// Every version in the index, in path and then line order. An index file that cannot be read is a problem.
async fn versions(
    state: &AppState,
    tenant: &Tenant,
    problems: &mut Vec<Problem>,
) -> Result<Vec<Version>, ApiError> {
    let mut versions = Vec::new();
    for path in tenant.paths(INDEX_DIR) {
        let Some((_, content)) = tenant.file(&state.gh, &state.blobs, &path).await? else {
            continue;
        };
        let krate = path
            .strip_prefix(INDEX_DIR)
            .and_then(name_from_path)
            .unwrap_or(&path)
            .to_owned();
        let Some(file) = std::str::from_utf8(&content)
            .ok()
            .and_then(|text| IndexFile::parse(text).ok())
        else {
            problems.push(Problem {
                code: "index_invalid",
                krate,
                version: None,
                detail: "the index file is not valid".into(),
            });
            continue;
        };
        for (i, line) in file.lines().iter().enumerate() {
            let (Some(name), Some(version), Some(cksum)) = (
                line["name"].as_str(),
                line["vers"].as_str(),
                line["cksum"].as_str(),
            ) else {
                problems.push(Problem {
                    code: "index_invalid",
                    krate: krate.clone(),
                    version: None,
                    detail: "an index line lacks name, vers or cksum".into(),
                });
                continue;
            };
            versions.push(Version {
                name: name.to_owned(),
                version: version.to_owned(),
                cksum: cksum.to_owned(),
                first: i == 0,
            });
        }
    }
    Ok(versions)
}

/// Checks each version, in the order of `versions`. Every report sees GitHub's releases afresh, listed 100 at a
/// time, so a deleted or changed release is always reported; provenance, the one check that needs a download per
/// version, is verified once per version and remembered (see [`provenance_fingerprint`]).
async fn checks(
    state: &Arc<AppState>,
    tenant: &Arc<Tenant>,
    token: &str,
    versions: &[Version],
) -> Result<Vec<Arc<VersionCheck>>, ApiError> {
    let releases: Arc<HashMap<String, github::Release>> = Arc::new(
        state
            .gh
            .releases(token, &tenant.storage_repo)
            .await?
            .into_iter()
            // A draft has no tag yet: it is not a published version's release.
            .filter(|r| !r.draft)
            .map(|r| (r.tag_name.clone(), r))
            .collect(),
    );
    let jwks = Arc::new(state.oidc.key_set().await?);
    let options = Arc::new(Options {
        base_url: state.config.tenant_base_url(&tenant.slug),
        storage_app_login: format!("{}[bot]", state.config.storage_app_slug),
        oidc_issuer: state.config.oidc_issuer.clone(),
    });
    let limit = Arc::new(Semaphore::new(CONCURRENCY));
    let mut tasks = JoinSet::new();
    for (i, v) in versions.iter().enumerate() {
        let (state, tenant, token) = (state.clone(), tenant.clone(), token.to_owned());
        let (jwks, options, limit, v) = (jwks.clone(), options.clone(), limit.clone(), v.clone());
        let releases = releases.clone();
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await;
            let check = check(&state, &tenant, &token, &releases, &jwks, &options, &v).await?;
            Ok::<_, ApiError>((i, check))
        });
    }
    let mut results: Vec<Option<Arc<VersionCheck>>> = vec![None; versions.len()];
    while let Some(joined) = tasks.join_next().await {
        let (i, check) = joined.map_err(|e| ApiError::internal(e.to_string()))??;
        results[i] = Some(Arc::new(check));
    }
    Ok(results.into_iter().flatten().collect())
}

/// Stands for one verified provenance file, without naming the crate: a SHA-256 of the storage repository, the
/// version, its checksum, the provenance file's digest (or asset ID) and the owners file's blob. A release's files
/// are immutable, so the verdict holds while all of these stay the same; a changed owners file means checking again.
fn provenance_fingerprint(
    repo_id: u64,
    v: &Version,
    asset: &remote::Asset,
    owners_blob: Option<&str>,
) -> [u8; 32] {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    for part in [
        repo_id.to_string().as_str(),
        &v.name.to_ascii_lowercase(),
        &v.version,
        &v.cksum,
        asset.digest.as_deref().unwrap_or(""),
        &asset.id.to_string(),
        owners_blob.unwrap_or(""),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize().into()
}

/// Runs the verifier's checks on what GitHub holds for one version. The owners in force are the crate's current
/// ones: owners files are created at a crate's first publish and changed only by people.
async fn check(
    state: &AppState,
    tenant: &Tenant,
    token: &str,
    releases: &HashMap<String, github::Release>,
    jwks: &JwkSet,
    options: &Options,
    v: &Version,
) -> Result<VersionCheck, ApiError> {
    let release = releases
        .get(&release_tag(&v.name, &v.version))
        .map(|r| remote::Release {
            draft: r.draft,
            immutable: r.immutable,
            assets: r
                .assets
                .iter()
                .map(|a| remote::Asset {
                    id: a.id,
                    name: a.name.clone(),
                    digest: a.digest.clone(),
                })
                .collect(),
        });
    let records = state.terms.records();
    let mut provenance = None;
    let mut crate_bytes = None;
    // The fingerprint of provenance verified before, which then needs no download; or of provenance to check now.
    let mut verified_before = false;
    let mut fingerprint = None;
    if let Some(release) = &release {
        if let Some(asset) = release.provenance_asset(&v.name, &v.version) {
            let owners_blob = tenant.file_sha(&owner_path(&v.name));
            let fp =
                provenance_fingerprint(tenant.storage_repo_id, v, asset, owners_blob.as_deref());
            verified_before = records.provenance_verified(&fp).await.unwrap_or_else(|e| {
                log::warn!(error:% = e; "looking up verified provenance failed; checking again");
                false
            });
            if !verified_before {
                provenance = Some(asset_bytes(state, tenant, token, asset.id).await?);
                fingerprint = Some(fp);
            }
        }
        if let Some(asset) = release
            .crate_asset(&v.name, &v.version)
            .filter(|a| a.digest.is_none())
        {
            crate_bytes = Some(asset_bytes(state, tenant, token, asset.id).await?);
        }
    }
    let owner = tenant.owner(&v.name);
    let published = Published {
        owner: owner.as_ref(),
        first: v.first,
        // As things stand now: the dashboard does not replay the settings' history, as the verifier does.
        manual_allowed: owner.as_ref().is_some_and(|o| {
            (!v.first && o.allow_manual_publish)
                || tenant.settings.allows_manual_publish(&[&o.repository])
        }),
    };
    let evidence = Evidence {
        release: release.as_ref(),
        provenance: provenance.as_deref().map(|jwt| (jwt, jwks)),
        crate_bytes: crate_bytes.as_deref(),
    };
    let mut result = check_version(
        &v.name,
        &v.version,
        &v.cksum,
        Some(published),
        &evidence,
        options,
    );
    if verified_before {
        // The same file, verified against the same owners: what the check would find again.
        result.provenance = Provenance::Valid;
    } else if let Some(fp) = fingerprint
        && result.provenance == Provenance::Valid
        && let Err(e) = records.record_provenance_verified(&fp).await
    {
        log::warn!(error:% = e; "remembering verified provenance failed");
    }
    Ok(result)
}

async fn asset_bytes(
    state: &AppState,
    tenant: &Tenant,
    token: &str,
    asset_id: u64,
) -> Result<Bytes, ApiError> {
    let url = state
        .gh
        .asset_download_url(token, &tenant.storage_repo, asset_id)
        .await?;
    Ok(state.gh.download(&url).await?)
}

/// Whether a workflow in the storage repository runs the verifier.
pub(crate) async fn has_verify_workflow(
    state: &AppState,
    tenant: &Tenant,
) -> Result<bool, ApiError> {
    for path in tenant.paths(WORKFLOWS_DIR) {
        if let Some((_, content)) = tenant.file(&state.gh, &state.blobs, &path).await?
            && String::from_utf8_lossy(&content).contains(VERIFIER)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What part of the storage repository a commit changed, most significant first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Area {
    Index,
    Owners,
    Settings,
}

impl Area {
    /// What a change to this area is, when its commit message does not say more.
    fn action(self) -> Action {
        match self {
            Self::Index => Action::IndexChange,
            Self::Owners => Action::OwnersChange,
            Self::Settings => Action::SettingsChange,
        }
    }
}

/// The storage repository's history of the index, owners and settings, newest first.
/// The storage repository's commits, all or those touching `path`, newest first: the ones seen before, plus any
/// newer from GitHub. A rewritten history, where the newest seen is gone, is listed whole again.
async fn commits(
    state: &AppState,
    tenant: &Tenant,
    token: &str,
    path: Option<&'static str>,
) -> Result<Arc<Vec<Commit>>, github::GitHubError> {
    let key = (tenant.storage_repo_id, path.unwrap_or(""));
    let seen = state.compliance.commits.get(&key).await;
    let newest = seen.as_ref().and_then(|c| c.first()).map(|c| c.sha.clone());
    let (newer, reached) = state
        .gh
        .commits_since(
            token,
            &tenant.storage_repo,
            &tenant.branch,
            path,
            newest.as_deref(),
        )
        .await?;
    let all = match seen {
        Some(seen) if reached && newer.is_empty() => seen,
        Some(seen) if reached => Arc::new(newer.into_iter().chain(seen.iter().cloned()).collect()),
        _ => Arc::new(newer),
    };
    state.compliance.commits.insert(key, all.clone()).await;
    Ok(all)
}

async fn audit(
    state: &AppState,
    tenant: &Tenant,
    token: &str,
    provenance: &HashMap<(String, String), bool>,
) -> Result<Vec<AuditEntry>, ApiError> {
    let touched = |path: &'static str| async move {
        let commits = commits(state, tenant, token, Some(path)).await?;
        Ok::<_, github::GitHubError>(
            commits
                .iter()
                .map(|c| c.sha.clone())
                .collect::<HashSet<_>>(),
        )
    };
    // The full history gives the order; the listings by path say what each commit changed.
    let (all, index, owners, settings) = tokio::try_join!(
        commits(state, tenant, token, None),
        touched(INDEX_DIR.trim_end_matches('/')),
        touched(OWNERS_DIR.trim_end_matches('/')),
        touched(SETTINGS_PATH),
    )?;
    let app = format!("{}[bot]", state.config.storage_app_slug);
    Ok(all
        .iter()
        .cloned()
        .filter_map(|commit| {
            let area = if index.contains(&commit.sha) {
                Area::Index
            } else if owners.contains(&commit.sha) {
                Area::Owners
            } else if settings.contains(&commit.sha) {
                Area::Settings
            } else {
                return None;
            };
            Some(entry(commit, area, &app, provenance))
        })
        .collect())
}

/// One audit entry. Our App's commit messages say what was done and by whom (see `publish.rs` and `account.rs`); a
/// person's commit is attributed to its author, and classified by what it changed.
fn entry(
    commit: Commit,
    area: Area,
    app: &str,
    provenance: &HashMap<(String, String), bool>,
) -> AuditEntry {
    let detail = &commit.commit;
    // Signed by GitHub for our App: an author email alone could be anyone's.
    let by_app = commit.author.as_ref().is_some_and(|a| a.login == app)
        && detail.verification.as_ref().is_some_and(|v| v.verified);
    let (subject, body) = detail
        .message
        .split_once('\n')
        .unwrap_or((&detail.message, ""));
    let field = |key: &str| {
        body.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_owned())
    };
    // `Published by alice via workflow …`; publishes before workflow actors were checked say `Publisher: workflow …`.
    let publisher = || field("Published by ").or_else(|| field("Publisher:"));
    let words: Vec<&str> = subject.split_whitespace().collect();
    let (action, krate, version, by) = match (by_app, words.as_slice(), area) {
        (true, ["Publish", name, version], Area::Index) => {
            (Action::Publish, Some(*name), Some(*version), publisher())
        }
        // A crate's first publish creates its owners file, in a commit of its own.
        (true, ["Publish", name, _], Area::Owners) => {
            (Action::OwnersChange, Some(*name), None, publisher())
        }
        (true, ["Yank", name, version], Area::Index) => {
            (Action::Yank, Some(*name), Some(*version), field("By:"))
        }
        (true, ["Unyank", name, version], Area::Index) => {
            (Action::Unyank, Some(*name), Some(*version), field("By:"))
        }
        (true, ["Create", "privatecrates.toml"], Area::Settings) => (
            Action::SettingsChange,
            None,
            None,
            body.split_once("requested by ")
                .map(|(_, rest)| rest.trim().trim_end_matches('.').to_owned()),
        ),
        _ => (area.action(), None, None, None),
    };
    let by = by.unwrap_or_else(|| match &commit.author {
        Some(author) => author.login.clone(),
        // An author email linked to no GitHub account: only the name git recorded.
        None => detail
            .author
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), |a| a.name.clone()),
    });
    let provenance = (action == Action::Publish).then(|| {
        krate
            .zip(version)
            .and_then(|(k, v)| {
                provenance
                    .get(&(k.to_ascii_lowercase(), v.to_owned()))
                    .copied()
            })
            .unwrap_or(false)
    });
    AuditEntry {
        at: detail
            .committer
            .as_ref()
            .or(detail.author.as_ref())
            .map(|s| s.date.clone()),
        action,
        krate: krate.map(str::to_owned),
        version: version.map(str::to_owned),
        by,
        provenance,
        commit: commit.sha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(message: &str, login: Option<&str>, verified: bool) -> Commit {
        serde_json::from_value(serde_json::json!({
            "sha": "abc123",
            "commit": {
                "message": message,
                "author": { "name": "Some Name", "date": "2026-09-27T14:20:01Z" },
                "committer": { "name": "GitHub", "date": "2026-09-27T14:20:02Z" },
                "verification": { "verified": verified },
            },
            "author": login.map(|l| serde_json::json!({ "login": l })),
        }))
        .unwrap()
    }

    const APP: &str = "privatecrates-storage[bot]";

    fn classify(message: &str, login: Option<&str>, verified: bool, area: Area) -> AuditEntry {
        let provenance = HashMap::from([(("story_engine".into(), "0.2.0".into()), true)]);
        entry(commit(message, login, verified), area, APP, &provenance)
    }

    #[test]
    fn our_apps_messages_say_what_and_who() {
        let publish = classify(
            "Publish story_engine 0.2.0\n\nPublished by alice via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42, attempt 1), triggered by push on refs/tags/v0.2.0\nChecksum: sha256:00\n",
            Some(APP),
            true,
            Area::Index,
        );
        assert_eq!(publish.action, Action::Publish);
        assert_eq!(publish.krate.as_deref(), Some("story_engine"));
        assert_eq!(publish.version.as_deref(), Some("0.2.0"));
        assert_eq!(
            publish.by,
            "alice via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42, attempt 1), \
             triggered by push on refs/tags/v0.2.0"
        );
        assert_eq!(publish.provenance, Some(true));
        assert_eq!(publish.at.as_deref(), Some("2026-09-27T14:20:02Z"));

        // Publishes from before workflow actors were checked keep their publisher.
        let earlier = classify(
            "Publish story_engine 0.2.0\n\nPublisher: workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42)\nChecksum: sha256:00\n",
            Some(APP),
            true,
            Area::Index,
        );
        assert_eq!(
            (earlier.action, earlier.by.as_str()),
            (
                Action::Publish,
                "workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42)"
            )
        );

        let manual = classify(
            "Publish story_engine 0.2.0\n\nPublished by alice (manual publish, no provenance)\nChecksum: sha256:00\n",
            Some(APP),
            true,
            Area::Index,
        );
        assert_eq!(manual.by, "alice (manual publish, no provenance)");

        let owners = classify(
            "Publish story_engine 0.1.0\n\nPublished by alice via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.1.0 (run 7), triggered by release on refs/tags/v0.1.0\n",
            Some(APP),
            true,
            Area::Owners,
        );
        assert_eq!(owners.action, Action::OwnersChange);
        assert_eq!(owners.version, None);
        assert_eq!(owners.provenance, None);
        assert!(
            owners.by.starts_with("alice via workflow "),
            "{}",
            owners.by
        );

        let yank = classify(
            "Yank story_engine 0.2.0\n\nBy: bob\n",
            Some(APP),
            true,
            Area::Index,
        );
        assert_eq!((yank.action, yank.by.as_str()), (Action::Yank, "bob"));
        let unyank = classify(
            "Unyank story_engine 0.2.0\n\nBy: bob\n",
            Some(APP),
            true,
            Area::Index,
        );
        assert_eq!(unyank.action, Action::Unyank);

        let settings = classify(
            "Create privatecrates.toml\n\nSet up the registry https://acme.privatecrates.dev, requested by alice.\n",
            Some(APP),
            true,
            Area::Settings,
        );
        assert_eq!(
            (settings.action, settings.by.as_str()),
            (Action::SettingsChange, "alice")
        );
    }

    #[test]
    fn peoples_commits_are_classified_by_what_they_changed() {
        let edit = classify(
            "Publish story_engine 9.9.9",
            Some("mallory"),
            true,
            Area::Index,
        );
        assert_eq!(edit.action, Action::IndexChange);
        assert_eq!((edit.krate, edit.by.as_str()), (None, "mallory"));
        // An unsigned commit claiming to be the App's is a person's.
        let forged = classify(
            "Yank story_engine 0.2.0\n\nBy: bob\n",
            Some(APP),
            false,
            Area::Index,
        );
        assert_eq!(
            (forged.action, forged.by.as_str()),
            (Action::IndexChange, APP)
        );
        let owners = classify("Allow manual publishing", None, false, Area::Owners);
        assert_eq!(
            (owners.action, owners.by.as_str()),
            (Action::OwnersChange, "Some Name")
        );
        let settings = classify("Warn on name clashes", Some("alice"), true, Area::Settings);
        assert_eq!(settings.action, Action::SettingsChange);
    }

    #[test]
    fn csv_fields_are_quoted_and_defused() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, b"), "\"a, b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_field("-1"), "'-1");
    }
}
