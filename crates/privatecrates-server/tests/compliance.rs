//! The compliance dashboard (docs/trust-and-status.md §4): integrity, publishers, risks and the audit trail.

mod common;

use common::{Crate, Harness, STORAGE_WEBHOOK_SECRET, error_code};
use serde_json::{Value, json};

const REPO: &str = "acme/story-engine";
const OWNERS: &str = "owners/story_engine.toml";
const INDEX: &str = "index/st/or/story_engine";
const WORKFLOW: &str = "releaser via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v1 (run 42, attempt 1), triggered by push on refs/tags/v1";

/// A signed-in member of `acme` with the given role.
async fn member(h: &Harness, login: &str, role: &str) -> String {
    let token = h.fake.add_user(login, "ghu_", &[]);
    h.fake.add_member(&token, &h.org, role);
    h.sign_in(&token).await
}

async fn report(h: &Harness, session: &str) -> Value {
    report_at(h, session, "/api/orgs/acme/compliance").await
}

async fn report_at(h: &Harness, session: &str, path: &str) -> Value {
    let response = h.api_get(path, session).await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

async fn publish(h: &Harness, repo: u64, version: &str) {
    h.publish_from_ci(REPO, repo, &Crate::new("story_engine", version, REPO))
        .await;
}

/// Tells the server the storage repository changed, as GitHub's push webhook does.
async fn pushed(h: &Harness, delivery: &str) {
    let payload = json!({
        "ref": "refs/heads/main",
        "repository": { "id": h.org.storage_repo, "full_name": "acme/crates-store" },
        "installation": { "id": h.org.storage_installation },
    });
    let response = h
        .github_webhook(STORAGE_WEBHOOK_SECRET, "push", delivery, &payload)
        .await;
    assert_eq!(response.status(), 204);
}

fn codes(report: &Value) -> Vec<(String, Option<String>, Option<String>)> {
    report["risks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["code"].as_str().unwrap().to_owned(),
                r["crate"].as_str().map(str::to_owned),
                r["version"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

fn risk(
    code: &str,
    krate: Option<&str>,
    version: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    (
        code.into(),
        krate.map(str::to_owned),
        version.map(str::to_owned),
    )
}

fn problems(report: &Value) -> Vec<String> {
    report["integrity"]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{} {} {}: {}",
                p["code"].as_str().unwrap(),
                p["crate"].as_str().unwrap(),
                p["version"].as_str().unwrap_or("-"),
                p["detail"].as_str().unwrap()
            )
        })
        .collect()
}

/// What the audit says, without dates and shas: (action, crate, version, by, provenance).
fn audit(report: &Value) -> Vec<Value> {
    report["audit"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            json!([
                e["action"],
                e["crate"],
                e["version"],
                e["by"],
                e["provenance"]
            ])
        })
        .collect()
}

#[tokio::test]
async fn a_clean_registry() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    publish(&h, repo, "0.2.0").await;
    let session = member(&h, "carol", "member").await;

    let report = report(&h, &session).await;
    assert_eq!(report["org"], json!({ "id": h.org.id, "login": "acme" }));
    assert!(report["generated_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(
        report["integrity"],
        json!({ "versions": 2, "immutable": 2, "digest_matches": 2, "provenance": 2, "manual": 0, "problems": [] })
    );
    assert_eq!(
        report["publishers"],
        json!([{ "crate": "story_engine", "repository": REPO, "workflows": ["release.yml"],
                 "environment": null, "manual_publish": false }])
    );
    // Nothing runs the verifier in the storage repository yet.
    assert_eq!(codes(&report), [risk("no_verify_workflow", None, None)]);
    assert_eq!(
        audit(&report),
        [
            json!(["publish", "story_engine", "0.2.0", WORKFLOW, true]),
            json!(["publish", "story_engine", "0.1.0", WORKFLOW, true]),
            json!(["owners_change", "story_engine", null, WORKFLOW, null]),
            json!(["settings_change", null, null, "A Developer", null]),
        ]
    );
    // Each entry names its commit, newest first.
    let commits = h.fake.commits(h.org.storage_repo);
    let newest = commits.last().unwrap();
    let entry = &report["audit"][0];
    assert_eq!(entry["commit"], newest.sha);
    assert!(entry["at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(report["audit_next_before"], Value::Null);
}

#[tokio::test]
async fn manual_publishing() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    publish(&h, repo, "0.1.0").await;
    let session = member(&h, "carol", "member").await;

    // An owner allows manual publishing, with a commit of their own.
    h.fake.commit_file(
        h.org.storage_repo,
        OWNERS,
        &format!(
            "repository_id = {repo}\nrepository = \"{REPO}\"\npublish_workflows = [\"release.yml\"]\nallow_manual_publish = true\n"
        ),
        "Allow manual publishing of story_engine",
        Some("alice"),
    );
    pushed(&h, "d-1").await;
    let report = self::report(&h, &session).await;
    assert_eq!(report["publishers"][0]["manual_publish"], true);
    assert_eq!(
        codes(&report),
        [
            risk("manual_publish_allowed", Some("story_engine"), None),
            risk("no_verify_workflow", None, None),
        ]
    );
    assert_eq!(
        audit(&report)[0],
        json!(["owners_change", null, null, "alice", null])
    );

    // A version published from a laptop has no provenance: allowed, and shown.
    let manual = Crate::new("story_engine", "0.2.0", REPO);
    assert_eq!(h.publish(&manual, &alice).await.status(), 200);
    pushed(&h, "d-2").await;
    let report = self::report(&h, &session).await;
    assert_eq!(report["integrity"]["versions"], 2);
    assert_eq!(report["integrity"]["provenance"], 1);
    assert_eq!(report["integrity"]["manual"], 1);
    // A manual publish is listed, though it is not a failed check.
    assert_eq!(
        problems(&report),
        [
            "manual_publish story_engine 0.2.0: published manually, without provenance; check that its publisher meant to"
        ]
    );
    assert_eq!(
        codes(&report),
        [
            risk("missing_provenance", Some("story_engine"), Some("0.2.0")),
            risk("manual_publish_allowed", Some("story_engine"), None),
            risk("no_verify_workflow", None, None),
        ]
    );
    assert_eq!(
        audit(&report)[0],
        json!([
            "publish",
            "story_engine",
            "0.2.0",
            "alice (manual publish from commit 0123456789abcdef0123456789abcdef01234567, no provenance)",
            false
        ])
    );
}

#[tokio::test]
async fn versions_without_provenance_where_it_is_required() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    publish(&h, repo, "0.2.0").await;
    for version in ["0.1.0", "0.2.0"] {
        h.fake.remove_asset(
            h.org.storage_repo,
            &format!("story_engine-{version}"),
            &format!("story_engine-{version}.provenance.jwt"),
        );
    }
    let session = member(&h, "carol", "member").await;
    let report = report(&h, &session).await;
    assert_eq!(report["integrity"]["provenance"], 0);
    assert_eq!(report["integrity"]["manual"], 0);
    assert_eq!(
        problems(&report),
        [
            "provenance_missing story_engine 0.1.0: the first version of a crate must have provenance, and this one \
             has none",
            "provenance_missing story_engine 0.2.0: it has no provenance, and its crate did not allow manual \
             publishing",
        ]
    );
    assert_eq!(
        codes(&report)[..2],
        [
            risk("missing_provenance", Some("story_engine"), Some("0.1.0")),
            risk("missing_provenance", Some("story_engine"), Some("0.2.0")),
        ]
    );
    assert_eq!(report["audit"][0]["provenance"], false);
}

#[tokio::test]
async fn a_mutable_release() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    h.fake
        .make_release_mutable(h.org.storage_repo, "story_engine-0.1.0");
    let session = member(&h, "carol", "member").await;
    let report = report(&h, &session).await;
    assert_eq!(report["integrity"]["immutable"], 0);
    assert_eq!(report["integrity"]["digest_matches"], 1);
    assert_eq!(
        problems(&report),
        [
            "release_mutable story_engine 0.1.0: its release is not immutable, so its files could have been changed"
        ]
    );
}

#[tokio::test]
async fn a_digest_mismatch() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    h.fake.replace_asset(
        h.org.storage_repo,
        "story_engine-0.1.0",
        "story_engine-0.1.0.crate",
        b"not the published crate",
    );
    let session = member(&h, "carol", "member").await;
    let report = report(&h, &session).await;
    assert_eq!(report["integrity"]["digest_matches"], 0);
    assert_eq!(report["integrity"]["provenance"], 1);
    assert_eq!(
        problems(&report),
        ["digest_mismatch story_engine 0.1.0: the .crate file does not match the index checksum"]
    );
}

#[tokio::test]
async fn forged_provenance() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    let krate = Crate::new("story_engine", "0.1.0", REPO);
    let claims = h.fake.actions_claims(&h.org, REPO, repo, "release.yml");
    let forged = privatecrates_testkit::forge_oidc(
        &h.fake.oidc_issuer(),
        &privatecrates_common::audience::publish(
            &h.base(),
            "story_engine",
            "0.1.0",
            &krate.cksum(),
        ),
        &claims,
    );
    h.fake.replace_asset(
        h.org.storage_repo,
        "story_engine-0.1.0",
        "story_engine-0.1.0.provenance.jwt",
        forged.as_bytes(),
    );
    let session = member(&h, "carol", "member").await;
    let report = report(&h, &session).await;
    assert_eq!(report["integrity"]["provenance"], 0);
    let problem = &report["integrity"]["problems"][0];
    assert_eq!(problem["code"], "provenance_invalid");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .starts_with("its provenance is invalid: "),
        "{problem}"
    );
    // The release has a provenance asset, even if it is not valid.
    assert_eq!(report["audit"][0]["provenance"], true);
}

#[tokio::test]
async fn a_name_clash_with_crates_io() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    // The organisation chose to be warned of clashes rather than refused.
    h.fake.write_file(
        h.org.storage_repo,
        "privatecrates.toml",
        "slug = \"acme\"\nname_clash = \"warn\"\n",
    );
    h.state.discover().await.unwrap();
    h.fake.add_crates_io_crate("story_engine");
    publish(&h, repo, "0.1.0").await;
    let session = member(&h, "carol", "member").await;
    let report = report(&h, &session).await;
    assert_eq!(
        codes(&report),
        [
            risk("name_clash", Some("story_engine"), None),
            risk("no_verify_workflow", None, None),
        ]
    );
    assert_eq!(report["integrity"]["problems"], json!([]));
}

#[tokio::test]
async fn a_verify_workflow_in_the_storage_repository() {
    let h = Harness::start().await;
    let session = member(&h, "carol", "member").await;
    h.fake.write_file(
        h.org.storage_repo,
        ".github/workflows/ci.yml",
        "on: push\njobs: {}\n",
    );
    pushed(&h, "d-1").await;
    let report = report(&h, &session).await;
    let detail = report["risks"][0]["detail"].as_str().unwrap();
    assert_eq!(report["risks"][0]["code"], "no_verify_workflow");
    assert!(detail.contains("acme/crates-store"), "{detail}");
    assert!(detail.contains("/docs/verify"), "{detail}");

    h.fake.write_file(
        h.org.storage_repo,
        ".github/workflows/verify.yml",
        "on:\n  schedule: [{ cron: \"17 * * * *\" }]\njobs:\n  verify:\n    steps:\n      - run: cargo binstall --no-confirm privatecrates-verify\n",
    );
    pushed(&h, "d-2").await;
    let report = self::report(&h, &session).await;
    assert_eq!(report["risks"], json!([]));
    // Workflow changes are not registry changes: the audit leaves them out.
    assert_eq!(audit(&report).len(), 1);
}

#[tokio::test]
async fn the_audit_trail_pages_and_exports() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    let alice = h.fake.add_user("alice", "ghu_", &[(repo, true)]);
    let versions: Vec<String> = (1..=48).map(|minor| format!("0.{minor}.0")).collect();
    for version in &versions {
        publish(&h, repo, version).await;
    }
    for action in ["yank", "unyank"] {
        let request = match action {
            "yank" => h
                .client
                .delete(h.url("/api/v1/crates/story_engine/0.1.0/yank")),
            _ => h
                .client
                .put(h.url("/api/v1/crates/story_engine/0.1.0/unyank")),
        };
        let response = request
            .header("Authorization", &alice)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{action}");
    }
    // A person edited the index directly, which only the App should do.
    let index = h.fake.file(h.org.storage_repo, INDEX).unwrap();
    h.fake.commit_file(
        h.org.storage_repo,
        INDEX,
        &index,
        "Publish story_engine 9.9.9",
        Some("mallory"),
    );
    let session = member(&h, "carol", "member").await;

    // 48 publishes, the owners file, a yank, an unyank, the edit and the settings: 53 entries.
    let first = report(&h, &session).await;
    let page = audit(&first);
    assert_eq!(page.len(), 50);
    assert_eq!(
        page[0],
        json!(["index_change", null, null, "mallory", null])
    );
    assert_eq!(
        page[1],
        json!(["unyank", "story_engine", "0.1.0", "alice", null])
    );
    assert_eq!(
        page[2],
        json!(["yank", "story_engine", "0.1.0", "alice", null])
    );
    assert_eq!(
        page[3],
        json!(["publish", "story_engine", "0.48.0", WORKFLOW, true])
    );
    let next = first["audit_next_before"].as_str().unwrap();
    assert_eq!(next, first["audit"][49]["commit"]);

    let second = report_at(
        &h,
        &session,
        &format!("/api/orgs/acme/compliance?before={next}"),
    )
    .await;
    assert_eq!(
        audit(&second),
        [
            json!(["publish", "story_engine", "0.1.0", WORKFLOW, true]),
            json!(["owners_change", "story_engine", null, WORKFLOW, null]),
            json!(["settings_change", null, null, "A Developer", null]),
        ]
    );
    assert_eq!(second["audit_next_before"], Value::Null);
    let unknown = report_at(&h, &session, "/api/orgs/acme/compliance?before=0000000").await;
    assert_eq!(unknown["audit"], json!([]));

    let response = h
        .api_get("/api/orgs/acme/compliance/audit.csv", &session)
        .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "text/csv; charset=utf-8"
    );
    assert_eq!(
        response.headers()["content-disposition"],
        "attachment; filename=\"acme-audit.csv\""
    );
    let csv = response.text().await.unwrap();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 54);
    assert_eq!(rows[0], "at,action,crate,version,by,provenance,commit");
    let edit = &first["audit"][0];
    assert_eq!(
        rows[1],
        format!(
            "{},index_change,,,mallory,,{}",
            edit["at"].as_str().unwrap(),
            edit["commit"].as_str().unwrap()
        )
    );
    assert!(
        rows[4].contains(&format!(
            ",publish,story_engine,0.48.0,\"{WORKFLOW}\",true,"
        )),
        "{}",
        rows[4]
    );
    let older = h
        .api_get(
            &format!("/api/orgs/acme/compliance/audit.csv?before={next}"),
            &session,
        )
        .await
        .text()
        .await
        .unwrap();
    assert_eq!(older.lines().count(), 4);
}

#[tokio::test]
async fn any_member_may_see_it_and_nobody_else() {
    let h = Harness::start().await;
    for path in [
        "/api/orgs/acme/compliance",
        "/api/orgs/acme/compliance/audit.csv",
    ] {
        let response = h.api_get(path, "").await;
        assert_eq!(response.status(), 401, "{path}");
        assert_eq!(error_code(response).await, "account::sign_in_required");

        let outsider = h.fake.add_user("mallory", "ghu_", &[]);
        let outsider = h.sign_in(&outsider).await;
        let response = h.api_get(path, &outsider).await;
        assert_eq!(response.status(), 404, "{path}");
        assert_eq!(error_code(response).await, "account::org_not_found");

        let member = member(&h, "carol", "member").await;
        assert_eq!(h.api_get(path, &member).await.status(), 200, "{path}");
    }

    // Tools and agents use a bearer token instead of the cookie.
    let token = h.fake.add_user("dave", "ghu_", &[]);
    h.fake.add_member(&token, &h.org, "admin");
    let response = h
        .client
        .get(h.apex("/api/orgs/acme/compliance"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // An organisation with no registry has nothing to report.
    let globex = h.fake.add_org_without_apps("globex");
    let token = h.fake.add_user("erin", "ghu_", &[]);
    h.fake.add_member(&token, &globex, "admin");
    let response = h
        .client
        .get(h.apex("/api/orgs/globex/compliance"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(error_code(response).await, "compliance::not_set_up");
}

#[tokio::test]
async fn reports_are_cached_until_the_storage_repository_changes() {
    let h = Harness::start().await;
    let repo = h.repo("story-engine");
    publish(&h, repo, "0.1.0").await;
    let session = member(&h, "carol", "member").await;
    let first = report(&h, &session).await;
    assert_eq!(first["integrity"]["versions"], 1);

    publish(&h, repo, "0.2.0").await;
    h.fake.clear_calls();
    let cached = report(&h, &session).await;
    assert_eq!(cached, first);
    let github = h.fake.calls();
    assert!(!github.iter().any(|c| c.contains("/commits")), "{github:?}");

    // The push GitHub sends for the new version discards the report; versions that passed are not checked again.
    pushed(&h, "d-1").await;
    h.fake.clear_calls();
    let fresh = report(&h, &session).await;
    assert_eq!(fresh["integrity"]["versions"], 2);
    assert_eq!(fresh["integrity"]["provenance"], 2);
    let github = h.fake.calls();
    let releases: Vec<&String> = github
        .iter()
        .filter(|c| c.contains("/releases/tags/"))
        .collect();
    assert_eq!(
        releases,
        ["GET /repos/acme/crates-store/releases/tags/story_engine-0.2.0"]
    );
}
