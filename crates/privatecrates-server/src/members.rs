//! Organisation member counts, which decide whether an organisation is free (docs/website-api.md).
//!
//! Counted with the reader App's installation token, which may read members, and cached for a day. The
//! `organization` webhooks adjust a cached count as members join and leave, without extending its life, so every
//! count is taken again from GitHub at least daily.

use std::time::{Duration, Instant};

use crate::github::{AppKind, GitHub, GitHubError};

/// How long a count is kept.
pub const COUNT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Expires each count a day after it was taken from GitHub; adjustments keep its expiry.
struct CountExpiry;

impl moka::Expiry<u64, u64> for CountExpiry {
    fn expire_after_create(&self, _org_id: &u64, _count: &u64, _at: Instant) -> Option<Duration> {
        Some(COUNT_TTL)
    }
}

pub struct Members {
    /// GitHub organisation ID → its number of members.
    counts: moka::future::Cache<u64, u64>,
}

impl Default for Members {
    fn default() -> Self {
        Self {
            counts: moka::future::Cache::builder()
                .max_capacity(100_000)
                .expire_after(CountExpiry)
                .build(),
        }
    }
}

impl Members {
    /// The organisation's member count, or `None` when it is unknown: the reader App is not installed there, or
    /// the lookup failed (logged). `installation` is the reader App's installation, when already known.
    pub async fn count(
        &self,
        gh: &GitHub,
        org_id: u64,
        org_login: &str,
        installation: Option<u64>,
    ) -> Option<u64> {
        self.counts
            .optionally_get_with(org_id, async {
                match lookup(gh, org_login, installation).await {
                    Ok(count) => count,
                    Err(e) => {
                        tracing::warn!(org = %org_login, error = %e, "counting members failed; treating the organisation as free");
                        None
                    }
                }
            })
            .await
    }

    /// Adjusts a cached count for a member who joined (`+1`) or left (`-1`). An organisation not counted yet is
    /// counted when next needed, which includes the change.
    pub async fn adjust(&self, org_id: u64, change: i64) {
        if let Some(count) = self.counts.get(&org_id).await {
            self.counts
                .insert(org_id, count.saturating_add_signed(change))
                .await;
        }
    }
}

async fn lookup(
    gh: &GitHub,
    org_login: &str,
    installation: Option<u64>,
) -> Result<Option<u64>, GitHubError> {
    let installation = match installation {
        Some(id) => id,
        None => match gh.org_installation(AppKind::Reader, org_login).await? {
            Some(installation) => installation.id,
            None => return Ok(None),
        },
    };
    let token = gh.installation_token(AppKind::Reader, installation).await?;
    Ok(Some(gh.org_member_count(&token, org_login).await?))
}
