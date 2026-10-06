//! GitHub webhooks (SPEC §7), from both Apps, served on the website's host.
//!
//! They keep tenants, permission caches and storage snapshots current without waiting for TTLs. They are an
//! optimisation: GitHub does not guarantee delivery, so the TTLs remain the upper bound on staleness.

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use bytes::Bytes;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::{
    AppState,
    error::ApiError,
    tenant::{SETTINGS_PATH, Tenant},
};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/webhooks/github", post(github))
}

/// Delivery IDs handled in the last hour: GitHub may deliver an event more than once.
pub fn deliveries() -> moka::future::Cache<String, ()> {
    moka::future::Cache::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(60 * 60))
        .build()
}

async fn github(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    verify(&state.config.webhook_secrets, &headers, &body)?;
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let delivery = header("x-github-delivery").map(str::to_owned);
    if let Some(id) = &delivery
        && state.webhook_deliveries.contains_key(id)
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    handle(&state, header("x-github-event").unwrap_or_default(), &body).await?;
    // Recorded only once handled, so that a redelivery of a failed event is not ignored.
    if let Some(id) = delivery {
        state.webhook_deliveries.insert(id, ()).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Checks `X-Hub-Signature-256`: an HMAC-SHA256 of the body with one of the webhook secrets (each App has its own),
/// compared in constant time.
fn verify(secrets: &[Vec<u8>], headers: &HeaderMap, body: &[u8]) -> Result<(), ApiError> {
    if secrets.is_empty() {
        return Err(ApiError::WebhooksNotConfigured);
    }
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("sha256="))
        .and_then(|v| hex::decode(v).ok())
        .ok_or(ApiError::WebhookSignatureInvalid)?;
    let signed_with = |secret: &Vec<u8>| {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
        mac.update(body);
        mac.verify_slice(&signature).is_ok()
    };
    if secrets.iter().any(signed_with) {
        Ok(())
    } else {
        Err(ApiError::WebhookSignatureInvalid)
    }
}

/// The parts of a payload we use. Which are present depends on the event.
#[derive(Deserialize)]
struct Payload {
    action: Option<String>,
    installation: Option<Id>,
    organization: Option<Id>,
    /// `organization` events: the member added or removed.
    membership: Option<OrgMembership>,
    /// `membership` (team) and `member` (collaborator) events: the user.
    member: Option<Id>,
    /// Who caused the event; for `github_app_authorization`, the user who revoked it.
    sender: Option<Id>,
    repository: Option<Id>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Deserialize)]
struct Id {
    id: u64,
}

#[derive(Deserialize)]
struct OrgMembership {
    user: Option<Id>,
}

impl Payload {
    /// The user whose access an event changed.
    fn user(&self, event: &str) -> Option<u64> {
        match event {
            "organization" => self.membership.as_ref()?.user.as_ref().map(|u| u.id),
            "membership" | "member" => self.member.as_ref().map(|u| u.id),
            "github_app_authorization" => self.sender.as_ref().map(|u| u.id),
            _ => None,
        }
    }

    /// The tenant of the App installation that sent the event.
    fn tenant(&self, state: &AppState) -> Option<Arc<Tenant>> {
        let installation = self.installation.as_ref()?.id;
        state.tenants.all().into_iter().find(|t| {
            t.reader_installation == installation || t.storage_installation == installation
        })
    }
}

async fn handle(state: &AppState, event: &str, body: &[u8]) -> Result<(), ApiError> {
    let payload = || {
        serde_json::from_slice::<Payload>(body).map_err(|e| ApiError::WebhookPayloadInvalid {
            reason: e.to_string(),
        })
    };
    match event {
        // An App was installed, removed, suspended or given other repositories: the tenant may have appeared,
        // gone or changed, and so may what its users can see.
        "installation" | "installation_repositories" => {
            if let Some(tenant) = payload()?.tenant(state) {
                state
                    .permissions
                    .forget_installation(tenant.reader_installation)
                    .await;
            }
            state.discover().await?;
        }
        "organization" | "membership" | "member" | "github_app_authorization" => {
            let payload = payload()?;
            if let Some(user) = payload.user(event) {
                let installation = payload.tenant(state).map(|t| t.reader_installation);
                state.permissions.forget_user(user, installation).await;
            }
            if event == "organization" {
                member_count_changed(state, &payload).await;
            }
        }
        // Team access, repository renames, transfers, deletions and visibility changes can change what anyone in
        // the tenant can read. Owners files are not rewritten: their `repository_id` is authoritative, and a
        // deleted owning repository is handled when reading (SPEC §6.2).
        "team" | "repository" => {
            if let Some(tenant) = payload()?.tenant(state) {
                state
                    .permissions
                    .forget_installation(tenant.reader_installation)
                    .await;
            }
        }
        "push" => {
            let payload = payload()?;
            let pushed = payload.repository.as_ref().map(|r| r.id);
            let Some(tenant) = state
                .tenants
                .all()
                .into_iter()
                .find(|t| pushed == Some(t.storage_repo_id))
            else {
                return Ok(());
            };
            if payload.git_ref != Some(format!("refs/heads/{}", tenant.branch)) {
                return Ok(());
            }
            let settings = tenant.file_sha(SETTINGS_PATH);
            tenant.refresh(&state.gh, &state.blobs).await?;
            state.compliance.invalidate(tenant.storage_repo_id).await;
            // A changed slug or other setting takes effect only through discovery.
            if tenant.file_sha(SETTINGS_PATH) != settings {
                state.discover().await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Keeps an organisation's member count current as members join and leave (invitations do not count until
/// accepted), and starts the trial of a tenant that has just grown past the free limit.
async fn member_count_changed(state: &AppState, payload: &Payload) {
    let change = match payload.action.as_deref() {
        Some("member_added") => 1,
        Some("member_removed") => -1,
        _ => return,
    };
    let Some(org) = &payload.organization else {
        return;
    };
    state.members.adjust(org.id, change).await;
    if let Some(tenant) = state.tenants.by_org(org.id) {
        state.keep_billing_in_step(&tenant).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn signed(secret: &[u8], body: &[u8]) -> HeaderMap {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-hub-signature-256",
            HeaderValue::from_str(&format!(
                "sha256={}",
                hex::encode(mac.finalize().into_bytes())
            ))
            .unwrap(),
        );
        headers
    }

    #[test]
    fn signatures() {
        let body = br#"{"zen":"Keep it logically awesome."}"#;
        let headers = signed(b"secret", body);
        let secret = [b"secret".to_vec()];
        assert!(verify(&secret, &headers, body).is_ok());
        assert!(matches!(
            verify(&[b"other".to_vec()], &headers, body),
            Err(ApiError::WebhookSignatureInvalid)
        ));
        assert!(matches!(
            verify(&secret, &headers, b"{}"),
            Err(ApiError::WebhookSignatureInvalid)
        ));
        assert!(matches!(
            verify(&secret, &HeaderMap::new(), body),
            Err(ApiError::WebhookSignatureInvalid)
        ));
        assert!(matches!(
            verify(&[], &headers, body),
            Err(ApiError::WebhooksNotConfigured)
        ));
    }

    #[test]
    fn each_app_signs_with_its_own_secret() {
        let body = b"{}";
        let secrets = [b"reader".to_vec(), b"storage".to_vec()];
        assert!(verify(&secrets, &signed(b"reader", body), body).is_ok());
        assert!(verify(&secrets, &signed(b"storage", body), body).is_ok());
        assert!(matches!(
            verify(&secrets, &signed(b"other", body), body),
            Err(ApiError::WebhookSignatureInvalid)
        ));
    }

    #[test]
    fn the_affected_user_depends_on_the_event() {
        let payload: Payload = serde_json::from_str(
            r#"{"action":"member_removed","membership":{"user":{"id":7}},"sender":{"id":1}}"#,
        )
        .unwrap();
        assert_eq!(payload.user("organization"), Some(7));
        let payload: Payload =
            serde_json::from_str(r#"{"action":"removed","member":{"id":8},"sender":{"id":1}}"#)
                .unwrap();
        assert_eq!(payload.user("membership"), Some(8));
        assert_eq!(payload.user("member"), Some(8));
        let payload: Payload =
            serde_json::from_str(r#"{"action":"revoked","sender":{"id":9}}"#).unwrap();
        assert_eq!(payload.user("github_app_authorization"), Some(9));
        assert_eq!(payload.user("team"), None);
    }
}
