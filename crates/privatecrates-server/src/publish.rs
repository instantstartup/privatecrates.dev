//! Publishing (SPEC §3.4, §4.4): validation, authorisation, and the write order that keeps the index from ever
//! pointing at a missing or mutable file.

use std::sync::Arc;

use axum::Json;
use bytes::Bytes;
use privatecrates_common::{
    audience,
    index::{IndexFile, IndexLine, PublishMetadata},
    is_crates_io,
    name::CrateName,
    sha256_hex, trigger,
};
use time::OffsetDateTime;

use crate::{
    AppState,
    auth::{Caller, Credential, Resolver},
    billing::Standing,
    crate_file,
    error::ApiError,
    github::{AppKind, FileWrite, GitHubError, Release, now_secs},
    oidc::{ActionsClaims, OidcError},
    tenant::{NameClash, Owner, Tenant, index_path, owner_path},
};
use privatecrates_common::storage::{crate_asset_name, provenance_asset_name, release_tag};

const INDEX_RETRIES: usize = 5;

pub struct Upload {
    pub meta: PublishMetadata,
    pub crate_bytes: Bytes,
}

/// Parses Cargo's publish body: a u32 LE length, the JSON metadata, a u32 LE length, the `.crate` bytes.
pub fn parse_body(body: &Bytes, max_crate_bytes: usize) -> Result<Upload, ApiError> {
    let truncated = || ApiError::BodyTruncated;
    let read_len = |at: usize| -> Result<usize, ApiError> {
        let bytes: [u8; 4] = body
            .get(at..at + 4)
            .ok_or_else(truncated)?
            .try_into()
            .expect("slice of length 4");
        Ok(u32::from_le_bytes(bytes) as usize)
    };
    let json_len = read_len(0)?;
    let json_end = 4usize.checked_add(json_len).ok_or_else(truncated)?;
    let json = body.get(4..json_end).ok_or_else(truncated)?;
    let meta: PublishMetadata =
        serde_json::from_slice(json).map_err(|e| ApiError::MetadataInvalid {
            reason: e.to_string(),
        })?;
    let crate_len = read_len(json_end)?;
    if crate_len > max_crate_bytes {
        return Err(ApiError::CrateTooLarge {
            size: crate_len,
            limit: max_crate_bytes,
        });
    }
    let crate_start = json_end + 4;
    if body.len() != crate_start + crate_len {
        return Err(truncated());
    }
    Ok(Upload {
        meta,
        crate_bytes: body.slice(crate_start..),
    })
}

/// Who is publishing, as recorded in the audit trail.
enum Publisher {
    /// A trusted publish: the OIDC token is the provenance, stored with the release.
    Workflow {
        claims: Box<ActionsClaims>,
        token: String,
    },
    /// A manual publish, allowed only when the organisation or the crate opts in: from a clean git checkout, whose
    /// commit is recorded (the packager's claim, not proof).
    User {
        caller: Caller,
        login: String,
        commit: String,
    },
}

impl Publisher {
    /// Who published and how, for the commit message and release notes (`Published by …`), from which the
    /// compliance dashboard's audit trail reads it back (see `compliance.rs`).
    fn describe(&self) -> String {
        match self {
            Self::Workflow { claims, .. } => {
                let run_id = claims.run_id.as_deref().unwrap_or("unknown");
                let run = match &claims.run_attempt {
                    Some(attempt) => format!("run {run_id}, attempt {attempt}"),
                    None => format!("run {run_id}"),
                };
                format!(
                    "{} via workflow {} ({run}), triggered by {} on {}",
                    claims.actor, claims.job_workflow_ref, claims.event_name, claims.git_ref
                )
            }
            Self::User { login, commit, .. } => {
                format!("{login} (manual publish from commit {commit}, no provenance)")
            }
        }
    }
}

pub async fn publish(
    state: &Arc<AppState>,
    tenant: &Arc<Tenant>,
    credential: Credential,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let base_url = state.config.tenant_base_url(&tenant.slug);
    let upload = parse_body(&body, state.config.max_crate_bytes)?;
    let meta = &upload.meta;
    let name = CrateName::parse(&meta.name).map_err(|e| ApiError::NameInvalid {
        reason: e.to_string(),
    })?;
    semver::Version::parse(&meta.vers).map_err(|e| ApiError::VersionInvalid {
        version: meta.vers.clone(),
        reason: e.to_string(),
    })?;
    let cksum = sha256_hex(&upload.crate_bytes);

    let resolver = Resolver {
        gh: &state.gh,
        cache: &state.permissions,
        registry_tokens: &state.registry_tokens,
        tenant,
    };
    let existing_owner = tenant.owner(name.as_str());
    let target = Target {
        base_url: &base_url,
        name: &name,
        meta,
        crate_bytes: &upload.crate_bytes,
        cksum: &cksum,
        owner: existing_owner.as_ref(),
    };
    let (publisher, new_owner) = authorize(state, tenant, &resolver, credential, &target).await?;
    // Checked only once the publisher is known to belong here, so that it says nothing to anyone else.
    if state.standing(tenant).await != Standing::Active {
        return Err(crate::routes::subscription_inactive(state, tenant));
    }
    // Unpacked only now, for a caller allowed to publish here (an anonymous upload is never decompressed), and off
    // the async workers, since a large archive takes a while to read.
    let (bytes, crate_name, version) = (
        upload.crate_bytes.clone(),
        meta.name.clone(),
        meta.vers.clone(),
    );
    tokio::task::spawn_blocking(move || crate_file::check(&bytes, &crate_name, &version))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::CrateFileInvalid {
            reason: e.to_string(),
        })?;
    let mut warnings = Vec::new();

    if new_owner.is_some() {
        check_name_clash(state, tenant, &name, &mut warnings).await?;
    }
    check_dependencies(tenant, &resolver, &publisher, meta).await?;

    let _write = tenant.write_lock.lock().await;
    let index = current_index(state, tenant, &name).await?;
    if index
        .as_ref()
        .is_some_and(|(_, file)| file.contains_version(&meta.vers))
    {
        return Err(ApiError::VersionExists {
            name: name.to_string(),
            version: meta.vers.clone(),
        });
    }
    let message = format!(
        "Publish {name} {}\n\nPublished by {}\nChecksum: sha256:{cksum}\n",
        meta.vers,
        publisher.describe()
    );
    if let Some(owner) = &new_owner {
        create_owner(state, tenant, &name, owner, &message).await?;
    }
    store_release(state, tenant, &name, &upload, &cksum, &publisher, &message).await?;
    let line = IndexLine::from_publish(meta, &cksum);
    append_index(state, tenant, &name, &line, &message).await?;
    tracing::info!(tenant = %tenant.slug, krate = %name, version = %meta.vers, publisher = %publisher.describe(), "published");
    warnings.extend(trial_reminder(state, tenant).await);
    Ok(Json(serde_json::json!({
        "warnings": { "invalid_categories": [], "invalid_badges": [], "other": warnings }
    })))
}

/// In the last days of a trial with no card, a reminder for whoever publishes.
async fn trial_reminder(state: &AppState, tenant: &Tenant) -> Option<String> {
    let ends = state
        .plan(tenant.org_id, &tenant.org_login, tenant.personal)
        .await
        .trial_ending(now_secs())?;
    let date = OffsetDateTime::from_unix_timestamp(i64::try_from(ends).ok()?)
        .ok()?
        .date();
    Some(format!(
        "the PrivateCrates free trial for {} ends on {date}; add a card at {}",
        tenant.org_login,
        state.config.account_url()
    ))
}

/// What is being published, and the crate's current owner, if it has one.
struct Target<'a> {
    base_url: &'a str,
    name: &'a CrateName,
    meta: &'a PublishMetadata,
    crate_bytes: &'a [u8],
    cksum: &'a str,
    owner: Option<&'a Owner>,
}

async fn authorize(
    state: &Arc<AppState>,
    tenant: &Tenant,
    resolver: &Resolver<'_>,
    credential: Credential,
    target: &Target<'_>,
) -> Result<(Publisher, Option<Owner>), ApiError> {
    let Target {
        base_url,
        name,
        meta,
        crate_bytes,
        cksum,
        owner,
    } = *target;
    let ci_only = || ApiError::CiOnly {
        name: name.to_string(),
        version: meta.vers.clone(),
        slug: tenant.slug.clone(),
        base_url: base_url.to_owned(),
    };
    match credential {
        Credential::Oidc(token) => {
            let expected = audience::publish(base_url, name.as_str(), &meta.vers, cksum);
            let claims = match state.oidc.validate(&token, &expected).await {
                Ok(claims) => claims,
                Err(OidcError::Invalid { reason }) => {
                    return Err(ApiError::PublishTokenNotBound {
                        reason,
                        audience: expected,
                    });
                }
                Err(e) => return Err(e.into()),
            };
            if claims.owner_id() != Some(tenant.org_id) {
                return Err(ApiError::WorkflowOutsideOrganisation);
            }
            let repository_id = claims
                .repository_id()
                .ok_or(ApiError::OidcRepositoryMissing)?;
            if !trigger::may_publish(&claims.event_name) {
                return Err(ApiError::TriggerNotAllowed {
                    name: name.to_string(),
                    event: claims.event_name.clone(),
                });
            }
            let workflow = claims
                .own_workflow_file()
                .ok_or(ApiError::WorkflowElsewhere)?;
            let new_owner = match owner {
                Some(owner) => {
                    if owner.repository_id != repository_id {
                        return Err(ApiError::NotOwningRepository {
                            name: name.to_string(),
                            owner: owner.repository.clone(),
                            repository: claims.repository.clone(),
                        });
                    }
                    // An owner created by a manual first publish names no workflow: any workflow in the owning
                    // repository may publish, still subject to the trigger and actor checks.
                    if !owner.allows_workflow(workflow) {
                        return Err(ApiError::WorkflowNotAllowed {
                            workflow: workflow.to_owned(),
                            name: name.to_string(),
                            allowed: owner.publish_workflows.join(", "),
                            owners_file: owner_path(name.as_str()),
                        });
                    }
                    if let Some(env) = &owner.publish_environment
                        && claims.environment.as_ref() != Some(env)
                    {
                        return Err(ApiError::EnvironmentRequired {
                            name: name.to_string(),
                            environment: env.clone(),
                        });
                    }
                    None
                }
                None => {
                    // First publish (SPEC §6.2): `package.repository` must be the workflow's own repository.
                    let declared = meta.repository.as_deref().and_then(github_repository);
                    if declared.as_deref() != Some(&claims.repository.to_ascii_lowercase()) {
                        return Err(ApiError::FirstPublishRepository {
                            name: name.to_string(),
                            declared: meta.repository.clone().unwrap_or_else(|| "not set".into()),
                            repository: claims.repository.clone(),
                        });
                    }
                    Some(Owner {
                        repository_id,
                        repository: claims.repository.clone(),
                        publish_workflows: vec![workflow.to_owned()],
                        publish_environment: None,
                        allow_manual_publish: false,
                        publish_bots: Vec::new(),
                    })
                }
            };
            check_actor(resolver, name, owner, &claims, repository_id).await?;
            Ok((
                Publisher::Workflow {
                    claims: Box::new(claims),
                    token,
                },
                new_owner,
            ))
        }
        Credential::Registry(_) => Err(ApiError::RegistryTokenReadOnly),
        credential @ (Credential::AppUser(_) | Credential::GitHub(_)) => {
            let caller = resolver.caller(credential).await?;
            // Before anything about this registry's settings or organisation: an outsider learns nothing more.
            if !resolver.can_use_registry(&caller).await? {
                return Err(ApiError::NoAccess);
            }
            let settings = &tenant.settings;
            let (repository_id, repository, new_owner) = match owner {
                Some(owner) => {
                    if !resolver.can_read(&caller, owner.repository_id).await? {
                        return Err(ApiError::NotFound);
                    }
                    let allowed = owner.allow_manual_publish
                        || if settings.repositories.is_empty() {
                            settings.allow_manual_publish
                        } else {
                            // Per-repository settings name the repository as it is called now, which may not be
                            // what it was called at the crate's first publish.
                            let token = state
                                .gh
                                .installation_token(AppKind::Reader, tenant.reader_installation)
                                .await?;
                            let current = state
                                .gh
                                .repository_by_id(&token, owner.repository_id)
                                .await?
                                .map_or_else(|| owner.repository.clone(), |repo| repo.full_name);
                            settings.allows_manual_publish(&[&current, &owner.repository])
                        };
                    if !allowed {
                        return Err(ci_only());
                    }
                    (owner.repository_id, owner.repository.clone(), None)
                }
                // A first publish from a developer's machine (SPEC §6.2): only where the organisation allows it, and
                // the owning repository is the one `package.repository` names, in the organisation.
                None => {
                    if !settings.any_manual_publish() {
                        return Err(ci_only());
                    }
                    let repository_required = || ApiError::ManualPublishRepository {
                        name: name.to_string(),
                        org: tenant.org_login.clone(),
                        declared: meta.repository.clone().unwrap_or_else(|| "not set".into()),
                    };
                    let declared = meta
                        .repository
                        .as_deref()
                        .and_then(github_repository)
                        .filter(|full_name| {
                            full_name
                                .split_once('/')
                                .is_some_and(|(org, _)| org.eq_ignore_ascii_case(&tenant.org_login))
                        })
                        .ok_or_else(repository_required)?;
                    let token = state
                        .gh
                        .installation_token(AppKind::Reader, tenant.reader_installation)
                        .await?;
                    // Unknown to the reader App, or unreadable by the caller: the same answer, so private names stay
                    // private.
                    let repo = state
                        .gh
                        .repository(&token, &declared)
                        .await?
                        .ok_or(ApiError::NotFound)?;
                    if !resolver.can_read(&caller, repo.id).await? {
                        return Err(ApiError::NotFound);
                    }
                    if !settings.allows_manual_publish(&[&repo.full_name]) {
                        return Err(ci_only());
                    }
                    let owner = Owner {
                        repository_id: repo.id,
                        repository: repo.full_name.clone(),
                        publish_workflows: Vec::new(),
                        publish_environment: None,
                        allow_manual_publish: false,
                        publish_bots: Vec::new(),
                    };
                    (repo.id, repo.full_name, Some(owner))
                }
            };
            if !resolver.can_push(&caller, repository_id).await? {
                return Err(ApiError::PushRequired {
                    action: format!("publishing {name}"),
                    repository,
                });
            }
            let vcs = crate_file::vcs(crate_bytes, name.as_str(), &meta.vers).map_err(|e| {
                ApiError::CrateFileInvalid {
                    reason: e.to_string(),
                }
            })?;
            let commit = match vcs.commit {
                Some(commit) if !vcs.dirty => commit,
                _ => {
                    return Err(ApiError::ManualPublishNotClean {
                        name: name.to_string(),
                        version: meta.vers.clone(),
                    });
                }
            };
            let login = resolver.login(&caller).await?;
            Ok((
                Publisher::User {
                    caller,
                    login,
                    commit,
                },
                new_owner,
            ))
        }
    }
}

/// Only people who can create releases in the owning repository may publish its crates, from CI too (SPEC §6.4):
/// the workflow run's actor must have Write access to it now, unless they are a bot the owners file lists.
async fn check_actor(
    resolver: &Resolver<'_>,
    name: &CrateName,
    owner: Option<&Owner>,
    claims: &ActionsClaims,
    repository_id: u64,
) -> Result<(), ApiError> {
    let actor = &claims.actor;
    if owner.is_some_and(|o| o.allows_bot(actor)) {
        return Ok(());
    }
    let refuse = |advice: String| ApiError::ActorCannotRelease {
        actor: actor.clone(),
        name: name.to_string(),
        repository: claims.repository.clone(),
        advice,
    };
    // A GitHub App's bot cannot be given Write access, so there is nothing to look up.
    if actor.ends_with("[bot]") {
        return Err(refuse(format!(
            "A bot may publish only if an administrator lists it in publish_bots in {} in the storage \
             repository, and a crate's first version must be published by a person",
            owner_path(name.as_str())
        )));
    }
    let actor_id = claims
        .actor_id()
        .ok_or_else(|| ApiError::OidcTokenInvalid {
            reason: "the token has no valid actor_id".into(),
        })?;
    if resolver
        .actor_can_release(&claims.repository, repository_id, actor, actor_id)
        .await?
    {
        return Ok(());
    }
    Err(refuse(format!(
        "Ask an administrator of {} for Write access, or have someone who has it run the workflow",
        claims.repository
    )))
}

/// `owner/repo`, lowercased, from a GitHub repository URL.
/// A crate in a monorepo may link to its directory (`https://github.com/acme/mono/tree/main/crates/foo`); anything
/// after `/tree/` or `/blob/` is ignored.
fn github_repository(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("http://github.com/"))?;
    let rest = rest.trim_end_matches('/');
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let tail_ok = match parts.next() {
        None => true,
        Some("tree" | "blob") => parts.next().is_some_and(|git_ref| !git_ref.is_empty()),
        Some(_) => false,
    };
    (tail_ok && !owner.is_empty() && !repo.is_empty())
        .then(|| format!("{owner}/{repo}").to_ascii_lowercase())
}

async fn check_name_clash(
    state: &AppState,
    tenant: &Tenant,
    name: &CrateName,
    warnings: &mut Vec<String>,
) -> Result<(), ApiError> {
    let clash = match state.crates_io.exists(name.as_str()).await {
        Ok(clash) => clash,
        Err(e) => {
            tracing::warn!(error = %e, "crates.io lookup failed");
            return match tenant.settings.name_clash {
                NameClash::Refuse => Err(ApiError::CratesIoUnavailable),
                NameClash::Warn => {
                    warnings.push("could not check whether this name exists on crates.io".into());
                    Ok(())
                }
            };
        }
    };
    if !clash {
        return Ok(());
    }
    match tenant.settings.name_clash {
        NameClash::Refuse => Err(ApiError::NameOnCratesIo {
            name: name.to_string(),
            slug: tenant.slug.clone(),
        }),
        NameClash::Warn => {
            warnings.push(format!(
                "a crate named {name} exists on crates.io. A dependency that forgets `registry = \"{}\"` would \
                 get that crate instead of yours",
                tenant.slug
            ));
            Ok(())
        }
    }
}

async fn check_dependencies(
    tenant: &Tenant,
    resolver: &Resolver<'_>,
    publisher: &Publisher,
    meta: &PublishMetadata,
) -> Result<(), ApiError> {
    for dep in &meta.deps {
        match dep.registry.as_deref() {
            Some(registry) if is_crates_io(registry) => {}
            Some(registry) => {
                return Err(ApiError::DependencyRegistry {
                    name: dep.name.clone(),
                    registry: registry.to_owned(),
                });
            }
            None => {
                let missing = || ApiError::DependencyNotFound {
                    name: dep.name.clone(),
                };
                let owner = tenant.owner(&dep.name).ok_or_else(missing)?;
                if tenant.file_sha(&index_path(&dep.name)).is_none() {
                    return Err(missing());
                }
                let readable = match publisher {
                    // CI reads follow the tenant's `ci_read` policy.
                    Publisher::Workflow { .. } => true,
                    Publisher::User { caller, .. } => {
                        resolver.can_read(caller, owner.repository_id).await?
                    }
                };
                if !readable {
                    return Err(missing());
                }
            }
        }
    }
    Ok(())
}

async fn current_index(
    state: &AppState,
    tenant: &Tenant,
    name: &CrateName,
) -> Result<Option<(String, IndexFile)>, ApiError> {
    let Some((sha, content)) = tenant
        .file(&state.gh, &state.blobs, &index_path(name.as_str()))
        .await?
    else {
        return Ok(None);
    };
    let text = std::str::from_utf8(&content)
        .map_err(|e| ApiError::internal(format!("index file for {name} is not UTF-8: {e}")))?;
    let file = IndexFile::parse(text)
        .map_err(|e| ApiError::internal(format!("index file for {name} is corrupt: {e}")))?;
    Ok(Some((sha, file)))
}

async fn create_owner(
    state: &AppState,
    tenant: &Tenant,
    name: &CrateName,
    owner: &Owner,
    message: &str,
) -> Result<(), ApiError> {
    let path = owner_path(name.as_str());
    let content = Bytes::from(
        toml::to_string(owner).map_err(|e| ApiError::internal(format!("owners file: {e}")))?,
    );
    let token = tenant.storage_token(&state.gh).await?;
    match state
        .gh
        .put_file(
            &token,
            &tenant.storage_repo,
            Some(&tenant.branch),
            FileWrite {
                path: &path,
                content: &content,
                sha: None,
                message,
            },
        )
        .await
    {
        Ok(sha) => {
            tenant.record_write(&state.blobs, &path, sha, content).await;
            Ok(())
        }
        Err(GitHubError::Conflict) => Err(ApiError::CreatedConcurrently {
            name: name.to_string(),
        }),
        Err(e) => Err(e.into()),
    }
}

/// Creates the immutable release holding the `.crate` and its provenance, or reuses one left by a crash between
/// publishing the release and appending the index (SPEC §4.4).
async fn store_release(
    state: &AppState,
    tenant: &Tenant,
    name: &CrateName,
    upload: &Upload,
    cksum: &str,
    publisher: &Publisher,
    message: &str,
) -> Result<(), ApiError> {
    let gh = &state.gh;
    let repo = &tenant.storage_repo;
    let token = tenant.storage_token(gh).await?;
    let tag = release_tag(name.as_str(), &upload.meta.vers);
    let asset_name = crate_asset_name(name.as_str(), &upload.meta.vers);
    let digest = format!("sha256:{cksum}");

    if let Some(existing) = gh.release_by_tag(&token, repo, &tag).await? {
        let matches = existing
            .assets
            .iter()
            .any(|a| a.name == asset_name && a.digest.as_deref() == Some(&digest));
        if existing.immutable && matches {
            return Ok(());
        }
        if existing.immutable {
            return Err(ApiError::ReleaseConflict {
                name: name.to_string(),
                version: upload.meta.vers.clone(),
            });
        }
        // A mutable leftover means immutable releases were off; it is not trusted, so replace it.
        gh.delete_release(&token, repo, existing.id).await?;
    }
    for draft in gh.drafts_for_tag(&token, repo, &tag).await? {
        gh.delete_release(&token, repo, draft.id).await?;
    }

    let draft = gh
        .create_draft_release(&token, repo, &tag, &tenant.branch, message)
        .await?;
    let result = async {
        let asset = gh
            .upload_asset(&token, &draft, &asset_name, upload.crate_bytes.clone())
            .await?;
        if asset.digest.as_deref() != Some(&digest) {
            tracing::error!(expected = %digest, got = ?asset.digest, "uploaded asset digest mismatch");
            return Err(ApiError::DigestMismatch);
        }
        if let Publisher::Workflow { token: oidc, .. } = publisher {
            gh.upload_asset(
                &token,
                &draft,
                &provenance_asset_name(name.as_str(), &upload.meta.vers),
                Bytes::from(oidc.clone()),
            )
            .await?;
        }
        let released: Release = gh.publish_release(&token, repo, draft.id).await?;
        if !released.immutable {
            gh.delete_release(&token, repo, released.id).await?;
            return Err(ApiError::ImmutableReleasesDisabled {
                repository: repo.clone(),
            });
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        // Best effort: a draft left behind is deleted by the next attempt anyway.
        let _ = gh.delete_release(&token, repo, draft.id).await;
    }
    result
}

async fn append_index(
    state: &AppState,
    tenant: &Tenant,
    name: &CrateName,
    line: &IndexLine,
    message: &str,
) -> Result<(), ApiError> {
    let path = index_path(name.as_str());
    let token = tenant.storage_token(&state.gh).await?;
    for _ in 0..INDEX_RETRIES {
        let current = current_index(state, tenant, name).await?;
        let (sha, mut file) = match current {
            Some((sha, file)) => (Some(sha), file),
            None => (None, IndexFile::default()),
        };
        if file.contains_version(&line.vers) {
            return Err(ApiError::PublishedConcurrently {
                name: name.to_string(),
                version: line.vers.clone(),
            });
        }
        file.append(line);
        let content = Bytes::from(file.render());
        match state
            .gh
            .put_file(
                &token,
                &tenant.storage_repo,
                Some(&tenant.branch),
                FileWrite {
                    path: &path,
                    content: &content,
                    sha: sha.as_deref(),
                    message,
                },
            )
            .await
        {
            Ok(new_sha) => {
                tenant
                    .record_write(&state.blobs, &path, new_sha, content)
                    .await;
                return Ok(());
            }
            Err(GitHubError::Conflict) => {
                tenant.refresh(&state.gh, &state.blobs).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::IndexContention)
}

/// Sets or clears `yanked` on a version (SPEC §4.5).
pub async fn set_yanked(
    state: &AppState,
    tenant: &Tenant,
    name: &str,
    version: &str,
    yanked: bool,
    login: &str,
) -> Result<(), ApiError> {
    let name = CrateName::parse(name).map_err(|_| ApiError::NotFound)?;
    let path = index_path(name.as_str());
    let token = tenant.storage_token(&state.gh).await?;
    let verb = if yanked { "Yank" } else { "Unyank" };
    let message = format!("{verb} {name} {version}\n\nBy: {login}\n");
    let _write = tenant.write_lock.lock().await;
    for _ in 0..INDEX_RETRIES {
        let (sha, mut file) = current_index(state, tenant, &name)
            .await?
            .ok_or(ApiError::NotFound)?;
        let before = file.render();
        if !file.set_yanked(version, yanked) {
            return Err(ApiError::NotFound);
        }
        let content = Bytes::from(file.render());
        if content == before.as_bytes() {
            return Ok(());
        }
        match state
            .gh
            .put_file(
                &token,
                &tenant.storage_repo,
                Some(&tenant.branch),
                FileWrite {
                    path: &path,
                    content: &content,
                    sha: Some(&sha),
                    message: &message,
                },
            )
            .await
        {
            Ok(new_sha) => {
                tenant
                    .record_write(&state.blobs, &path, new_sha, content)
                    .await;
                return Ok(());
            }
            Err(GitHubError::Conflict) => {
                tenant.refresh(&state.gh, &state.blobs).await?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(ApiError::IndexContention)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(meta: &[u8], krate: &[u8]) -> Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(meta);
        out.extend_from_slice(&(krate.len() as u32).to_le_bytes());
        out.extend_from_slice(krate);
        Bytes::from(out)
    }

    const META: &[u8] = br#"{"name":"a","vers":"1.0.0","deps":[],"features":{}}"#;

    #[test]
    fn parses_a_publish_body() {
        let upload = parse_body(&body(META, b"crate"), 100).unwrap();
        assert_eq!(upload.meta.name, "a");
        assert_eq!(&upload.crate_bytes[..], b"crate");
    }

    #[test]
    fn rejects_truncated_and_oversized_bodies() {
        let full = body(META, b"crate");
        for len in [0, 3, 10, full.len() - 1] {
            assert!(parse_body(&full.slice(..len), 100).is_err(), "{len}");
        }
        let mut extra = full.to_vec();
        extra.push(0);
        assert!(parse_body(&Bytes::from(extra), 100).is_err());
        let err = parse_body(&body(META, &[0; 101]), 100).err().unwrap();
        assert!(matches!(err, ApiError::CrateTooLarge { .. }));
        let mut huge = vec![0xff, 0xff, 0xff, 0xff];
        huge.extend_from_slice(META);
        assert!(parse_body(&Bytes::from(huge), 100).is_err());
    }

    #[test]
    fn repository_urls() {
        assert_eq!(
            github_repository("https://github.com/Acme/Story-Engine").as_deref(),
            Some("acme/story-engine")
        );
        assert_eq!(
            github_repository("https://github.com/acme/story-engine.git/").as_deref(),
            Some("acme/story-engine")
        );
        assert_eq!(github_repository("https://github.com/acme"), None);
        // Monorepo crates link to their directory.
        assert_eq!(
            github_repository("https://github.com/Acme/Mono/tree/main/crates/foo").as_deref(),
            Some("acme/mono")
        );
        assert_eq!(
            github_repository("https://github.com/acme/mono/blob/main/README.md").as_deref(),
            Some("acme/mono")
        );
        assert_eq!(github_repository("https://github.com/acme/mono/tree"), None);
        assert_eq!(
            github_repository("https://github.com/acme/mono/issues"),
            None
        );
        assert_eq!(github_repository("https://gitlab.com/acme/a"), None);
    }
}
