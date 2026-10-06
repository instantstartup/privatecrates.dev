//! Our own records (docs/preview.md §2): which organisation admin accepted which version of the terms, and when;
//! and, until the table is dropped, the requests made while the preview was by invitation (§5).
//!
//! Everything else PrivateCrates knows comes from GitHub or Stripe. Acceptances are the exception: they are our
//! evidence, so they are kept in our own Postgres rather than in the customer's storage repository, which the
//! customer can delete or stop us reading.
//!
//! The acceptances are append-only: nothing here updates or deletes one. The first acceptance of a version by an
//! organisation is kept, and a later one is a no-op.

use std::time::Duration;

use apollo_errors::Error;
use async_trait::async_trait;
use miette::Diagnostic;
use privatecrates_common::TERMS_VERSION;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use time::OffsetDateTime;
use tokio::sync::Mutex;

/// How long the answer to "has this organisation accepted the current terms?" is kept.
const ACCEPTED_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Error, Diagnostic)]
pub enum RecordsError {
    #[error("the database failed: {source}")]
    #[diagnostic(code(records::database))]
    Database {
        #[from]
        source: sqlx::Error,
    },
    #[error("migrating the database failed: {source}")]
    #[diagnostic(code(records::migrate))]
    Migrate {
        #[from]
        source: sqlx::migrate::MigrateError,
    },
}

/// How the admin accepted: the website's sign-in cookie, or the CLI's bearer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Website,
    Cli,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Website => "website",
            Self::Cli => "cli",
        }
    }
}

/// An acceptance to record.
pub struct Acceptance<'a> {
    pub org_id: u64,
    pub org_login: &'a str,
    pub user_id: u64,
    pub user_login: &'a str,
    pub version: &'a str,
    pub via: Via,
    /// The exact words the admin accepted.
    pub statement: &'a str,
}

/// A recorded acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub org_id: u64,
    pub org_login: String,
    pub user_id: u64,
    pub user_login: String,
    pub version: String,
    pub accepted_at: OffsetDateTime,
    /// `website` or `cli`.
    pub via: String,
    pub statement: String,
}

/// The words an admin accepts, recorded with the acceptance.
pub fn statement(version: &str, org_login: &str) -> String {
    format!(
        "I have read and accept the PrivateCrates preview terms ({version}) on behalf of {org_login}"
    )
}

#[async_trait]
pub trait Records: Send + Sync {
    /// Brings the schema up to date; run at start-up.
    async fn migrate(&self) -> Result<(), RecordsError>;

    /// Records an acceptance, unless the organisation has accepted that version already. Returns whether it was
    /// recorded.
    async fn accept(&self, acceptance: &Acceptance<'_>) -> Result<bool, RecordsError>;

    /// The organisation's acceptance of a version, if any.
    async fn acceptance(
        &self,
        org_id: u64,
        version: &str,
    ) -> Result<Option<Accepted>, RecordsError>;

    /// Whether the compliance dashboard has verified the provenance this fingerprint stands for
    /// (`compliance::provenance_fingerprint`).
    async fn provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<bool, RecordsError>;

    /// Notes that the provenance this fingerprint stands for verified.
    async fn record_provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<(), RecordsError>;
}

/// Postgres (`DATABASE_URL`).
pub struct Postgres {
    pool: PgPool,
}

impl Postgres {
    /// Connects on first use, so that start-up does not depend on the order in which services come up.
    pub fn connect_lazy(url: &str) -> Result<Self, RecordsError> {
        Ok(Self {
            pool: PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect_lazy(url)?,
        })
    }
}

#[async_trait]
impl Records for Postgres {
    async fn migrate(&self) -> Result<(), RecordsError> {
        sqlx::migrate!().run(&self.pool).await?;
        // Requests to join the invitation-only preview, which has ended: kept for at most 12 months, as the privacy
        // notice says, until the table is dropped.
        sqlx::query(
            "delete from invitation_requests where requested_at < now() - interval '12 months'",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn accept(&self, a: &Acceptance<'_>) -> Result<bool, RecordsError> {
        let result = sqlx::query(
            "insert into terms_acceptances (org_id, org_login, user_id, user_login, version, via, statement) \
             values ($1, $2, $3, $4, $5, $6, $7) \
             on conflict (org_id, version) do nothing",
        )
        // GitHub IDs fit in a bigint; the cast round-trips every u64 all the same.
        .bind(a.org_id.cast_signed())
        .bind(a.org_login)
        .bind(a.user_id.cast_signed())
        .bind(a.user_login)
        .bind(a.version)
        .bind(a.via.as_str())
        .bind(a.statement)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn acceptance(
        &self,
        org_id: u64,
        version: &str,
    ) -> Result<Option<Accepted>, RecordsError> {
        let row = sqlx::query(
            "select org_id, org_login, user_id, user_login, version, accepted_at, via, statement \
             from terms_acceptances where org_id = $1 and version = $2",
        )
        .bind(org_id.cast_signed())
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(Accepted {
            org_id: row.try_get::<i64, _>("org_id")?.cast_unsigned(),
            org_login: row.try_get("org_login")?,
            user_id: row.try_get::<i64, _>("user_id")?.cast_unsigned(),
            user_login: row.try_get("user_login")?,
            version: row.try_get("version")?,
            accepted_at: row.try_get("accepted_at")?,
            via: row.try_get("via")?,
            statement: row.try_get("statement")?,
        }))
    }

    async fn provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<bool, RecordsError> {
        let row = sqlx::query("select 1 from provenance_verdicts where fingerprint = $1")
            .bind(fingerprint.as_slice())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    async fn record_provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<(), RecordsError> {
        sqlx::query(
            "insert into provenance_verdicts (fingerprint) values ($1) on conflict do nothing",
        )
        .bind(fingerprint.as_slice())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// In memory, for tests and local development without `DATABASE_URL`. Lost at every restart.
#[derive(Default)]
pub struct Memory {
    rows: Mutex<Vec<Accepted>>,
    verified: Mutex<std::collections::HashSet<[u8; 32]>>,
}

#[async_trait]
impl Records for Memory {
    async fn migrate(&self) -> Result<(), RecordsError> {
        Ok(())
    }

    async fn accept(&self, a: &Acceptance<'_>) -> Result<bool, RecordsError> {
        let mut rows = self.rows.lock().await;
        if rows
            .iter()
            .any(|r| r.org_id == a.org_id && r.version == a.version)
        {
            return Ok(false);
        }
        rows.push(Accepted {
            org_id: a.org_id,
            org_login: a.org_login.to_owned(),
            user_id: a.user_id,
            user_login: a.user_login.to_owned(),
            version: a.version.to_owned(),
            accepted_at: OffsetDateTime::now_utc(),
            via: a.via.as_str().to_owned(),
            statement: a.statement.to_owned(),
        });
        Ok(true)
    }

    async fn acceptance(
        &self,
        org_id: u64,
        version: &str,
    ) -> Result<Option<Accepted>, RecordsError> {
        Ok(self
            .rows
            .lock()
            .await
            .iter()
            .find(|r| r.org_id == org_id && r.version == version)
            .cloned())
    }

    async fn provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<bool, RecordsError> {
        Ok(self.verified.lock().await.contains(fingerprint))
    }

    async fn record_provenance_verified(&self, fingerprint: &[u8; 32]) -> Result<(), RecordsError> {
        self.verified.lock().await.insert(*fingerprint);
        Ok(())
    }
}

/// The terms acceptances, with a short cache of which organisations accepted the current version.
pub struct Terms {
    records: Box<dyn Records>,
    /// GitHub organisation ID → whether it accepted [`TERMS_VERSION`].
    accepted: moka::future::Cache<u64, bool>,
}

impl Terms {
    pub fn new(records: Box<dyn Records>) -> Self {
        Self {
            records,
            accepted: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(ACCEPTED_TTL)
                .build(),
        }
    }

    pub fn records(&self) -> &dyn Records {
        self.records.as_ref()
    }

    /// Whether the organisation accepted the current terms. A failed lookup is logged and answers `false`
    /// (uncached): an admin asked to accept again records nothing new.
    pub async fn accepted(&self, org_id: u64) -> bool {
        self.accepted
            .optionally_get_with(org_id, async {
                match self.records.acceptance(org_id, TERMS_VERSION).await {
                    Ok(accepted) => Some(accepted.is_some()),
                    Err(e) => {
                        tracing::warn!(org_id, error = %e, "looking up the terms acceptance failed");
                        None
                    }
                }
            })
            .await
            .unwrap_or(false)
    }

    /// Records an acceptance; the first one of each version is kept.
    pub async fn accept(&self, acceptance: &Acceptance<'_>) -> Result<(), RecordsError> {
        let recorded = self.records.accept(acceptance).await?;
        self.accepted.invalidate(&acceptance.org_id).await;
        tracing::info!(org = %acceptance.org_login, by = %acceptance.user_login, version = %acceptance.version, via = acceptance.via.as_str(), recorded, "terms accepted");
        Ok(())
    }
}
