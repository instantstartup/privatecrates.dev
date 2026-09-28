//! The Postgres records (terms acceptances) against a real database. Ignored by default: they run with
//! `DATABASE_URL` set, as CI's `postgres` job does (`cargo test -p privatecrates-server --test records -- --ignored`).

use std::time::{SystemTime, UNIX_EPOCH};

use privatecrates_server::records::{Acceptance, InvitationRequest, Postgres, Records, Via};

fn database() -> Postgres {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL is set for these tests");
    Postgres::connect_lazy(&url).unwrap()
}

/// An organisation ID no earlier run used, so that the tests need no clean database.
fn fresh_org_id() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    u64::try_from(nanos % (1 << 52)).unwrap()
}

fn acceptance<'a>(org_id: u64, user_login: &'a str, via: Via) -> Acceptance<'a> {
    Acceptance {
        org_id,
        org_login: "acme",
        user_id: 12345,
        user_login,
        version: "preview-2026-09-27",
        via,
        statement: "I have read and accept the PrivateCrates preview terms (preview-2026-09-27) on behalf of acme",
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn migrations_run_again_harmlessly() {
    let db = database();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    assert_eq!(
        db.acceptance(fresh_org_id(), "preview-2026-09-27")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_first_acceptance_is_kept() {
    let db = database();
    db.migrate().await.unwrap();
    let org_id = fresh_org_id();
    assert_eq!(
        db.acceptance(org_id, "preview-2026-09-27").await.unwrap(),
        None
    );

    assert!(
        db.accept(&acceptance(org_id, "BrynCooke", Via::Website))
            .await
            .unwrap()
    );
    let first = db
        .acceptance(org_id, "preview-2026-09-27")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.org_id, org_id);
    assert_eq!(first.org_login, "acme");
    assert_eq!(first.user_id, 12345);
    assert_eq!(first.user_login, "BrynCooke");
    assert_eq!(first.version, "preview-2026-09-27");
    assert_eq!(first.via, "website");
    assert!(first.statement.ends_with("on behalf of acme"));

    // A repeat, by anyone, records nothing.
    assert!(
        !db.accept(&acceptance(org_id, "someone-else", Via::Cli))
            .await
            .unwrap()
    );
    assert_eq!(
        db.acceptance(org_id, "preview-2026-09-27")
            .await
            .unwrap()
            .unwrap(),
        first
    );

    // Another version is another record.
    let mut next = acceptance(org_id, "BrynCooke", Via::Cli);
    next.version = "2027-01-01";
    assert!(db.accept(&next).await.unwrap());
    assert_eq!(
        db.acceptance(org_id, "2027-01-01")
            .await
            .unwrap()
            .unwrap()
            .via,
        "cli"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn asking_again_replaces_an_invitation_request() {
    let db = database();
    db.migrate().await.unwrap();
    let user_id = fresh_org_id();
    let request = |org: &str, email: &str| InvitationRequest {
        org_login: org.into(),
        user_id,
        user_login: "alice".into(),
        email: email.into(),
        note: String::new(),
    };
    assert!(db.invitations_requested(user_id).await.unwrap().is_empty());
    db.request_invitation(&request("globex", "a@globex.example"))
        .await
        .unwrap();
    db.request_invitation(&request("globex", "alice@globex.example"))
        .await
        .unwrap();
    db.request_invitation(&request("hooli", "a@hooli.example"))
        .await
        .unwrap();
    assert_eq!(
        db.invitations_requested(user_id).await.unwrap(),
        ["globex", "hooli"]
    );
}
