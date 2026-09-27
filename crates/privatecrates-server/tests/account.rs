//! The website's sign-in, session and account API (docs/website-api.md).

mod common;

use common::{Harness, cookie, error_code};
use privatecrates_testkit::{READER_APP_ID, STORAGE_APP_ID};
use serde_json::{Value, json};

/// A signed-in admin of the harness's `acme` organisation.
async fn admin(h: &Harness) -> (String, String) {
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &h.org, "admin");
    let session = h.sign_in(&token).await;
    (token, session)
}

fn steps(onboarding: &Value) -> Vec<(String, String)> {
    onboarding["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_owned(),
                s["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn statuses(onboarding: &Value) -> Vec<String> {
    steps(onboarding).into_iter().map(|(_, s)| s).collect()
}

#[tokio::test]
async fn sign_in_round_trip() {
    let h = Harness::start().await;
    let response = h.api_get("/api/session", "").await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: Value = response.json().await.unwrap();
    let install_url = format!("{}/apps/privatecrates-reader/installations/new", h.fake.url);
    assert_eq!(
        body,
        json!({ "user": null, "orgs": [], "install_url": install_url })
    );

    let (_, session) = admin(&h).await;
    let body: Value = h
        .api_get("/api/session", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["user"]["login"], "alice");
    assert_eq!(body["install_url"], install_url);
    let orgs = body["orgs"].as_array().unwrap();
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["login"], "acme");
    assert_eq!(orgs[0]["id"], h.org.id);
    assert_eq!(orgs[0]["role"], "admin");
    assert_eq!(orgs[0]["tenant"]["slug"], "acme");
    assert_eq!(orgs[0]["tenant"]["registry_url"], h.base());
    // Billing is not configured: there is no Stripe status, and the plan follows the member count.
    assert_eq!(orgs[0]["tenant"]["status"], Value::Null);
    assert_eq!(orgs[0]["members"], 1);
    assert_eq!(orgs[0]["plan"], "free");

    let logout = h.api_post("/auth/logout", &session).send().await.unwrap();
    assert_eq!(logout.status(), 204);
    let cleared = cookie(&logout, "pc_session").unwrap();
    assert_eq!(cleared, "pc_session=");
    let header = logout.headers()["set-cookie"].to_str().unwrap();
    assert!(header.contains("Max-Age=0"), "{header}");
}

#[tokio::test]
async fn the_session_cookie_is_sealed() {
    let h = Harness::start().await;
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &h.org, "admin");
    let login = h
        .client
        .get(h.apex("/auth/github/login"))
        .send()
        .await
        .unwrap();
    let location = reqwest::Url::parse(login.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(location.path(), "/login/oauth/authorize");
    let query: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], privatecrates_testkit::READER_CLIENT_ID);
    assert_eq!(query["redirect_uri"], h.apex("/auth/github/callback"));
    let callback = h
        .client
        .get(h.apex("/auth/github/callback"))
        .query(&[
            ("code", h.fake.web_flow_code(&token).as_str()),
            ("state", &query["state"]),
        ])
        .header("Cookie", cookie(&login, "pc_sign_in").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(callback.status(), 302);
    assert_eq!(callback.headers()["location"], "/account");
    let set_cookie = callback
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .find(|v| v.starts_with("pc_session="))
        .unwrap();
    for attribute in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=28800",
    ] {
        assert!(set_cookie.contains(attribute), "{set_cookie}");
    }
    // The token is not readable in the cookie.
    assert!(!set_cookie.contains(&token));

    let session = cookie(&callback, "pc_session").unwrap();
    let mut tampered = session.clone().into_bytes();
    let last = tampered.len() - 2;
    tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).unwrap();
    let body: Value = h
        .api_get("/api/session", &tampered)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["user"], Value::Null);
    let response = h.api_get("/api/orgs/acme/onboarding", &tampered).await;
    assert_eq!(response.status(), 401);
    assert_eq!(error_code(response).await, "account::sign_in_required");

    // A session from another deployment (another secret) is not accepted either.
    let forged = "pc_session=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let body: Value = h
        .api_get("/api/session", forged)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["user"], Value::Null);
}

#[tokio::test]
async fn the_sign_in_state_is_checked() {
    let h = Harness::start().await;
    let token = h.fake.add_user("alice", "ghu_", &[]);
    // An off-site return_to is replaced with the account page.
    let login = h
        .client
        .get(h.apex("/auth/github/login?return_to=//evil.example/"))
        .send()
        .await
        .unwrap();
    let location = reqwest::Url::parse(login.headers()["location"].to_str().unwrap()).unwrap();
    let state = location
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let sign_in = cookie(&login, "pc_sign_in").unwrap();
    let callback = |state: String, cookie: Option<String>, code: String| {
        let mut request = h
            .client
            .get(h.apex("/auth/github/callback"))
            .query(&[("code", code), ("state", state)]);
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
        request.send()
    };

    // Started in another browser: no matching sign-in cookie.
    let response = callback(state.clone(), None, h.fake.web_flow_code(&token))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(error_code(response).await, "account::sign_in_state_invalid");
    let response = callback(
        state.clone(),
        Some("pc_sign_in=00000000000000000000000000000000".into()),
        h.fake.web_flow_code(&token),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), 400);
    // A forged state.
    let response = callback(
        "forged".into(),
        Some(sign_in.clone()),
        h.fake.web_flow_code(&token),
    )
    .await
    .unwrap();
    assert_eq!(error_code(response).await, "account::sign_in_state_invalid");
    // A code GitHub does not know.
    let response = callback(state.clone(), Some(sign_in.clone()), "stale".into())
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(error_code(response).await, "account::sign_in_failed");

    let response = callback(state, Some(sign_in), h.fake.web_flow_code(&token))
        .await
        .unwrap();
    assert_eq!(response.status(), 302);
    assert_eq!(response.headers()["location"], "/account");
}

#[tokio::test]
async fn state_changing_requests_must_come_from_the_website() {
    let h = Harness::start().await;
    let (_, session) = admin(&h).await;
    let post = |origin: Option<&str>, content_type: &str| {
        let mut request = h
            .client
            .post(h.apex("/api/orgs/acme/checkout"))
            .header("Cookie", &session)
            .header("Content-Type", content_type)
            .body("{}");
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        request.send()
    };
    for (origin, content_type) in [
        (None, "application/json"),
        (Some("http://evil.example"), "application/json"),
        (Some("http://acme.localhost"), "application/json"),
        (
            Some(h.apex("").as_str()),
            "application/x-www-form-urlencoded",
        ),
        (Some(h.apex("").as_str()), "text/plain"),
    ] {
        let response = post(origin, content_type).await.unwrap();
        assert_eq!(response.status(), 403, "{origin:?} {content_type}");
        assert_eq!(error_code(response).await, "account::cross_site_request");
    }
    let logout = h
        .client
        .post(h.apex("/auth/logout"))
        .header("Cookie", &session)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 403);
    // From the website, the request goes through (and fails only because billing is not configured).
    let response = post(Some(&h.apex("")), "application/json; charset=utf-8")
        .await
        .unwrap();
    assert_eq!(error_code(response).await, "billing::not_configured");
}

#[tokio::test]
async fn onboarding_checklist() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    let admin_token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&admin_token, &globex, "admin");
    let member_token = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&member_token, &globex, "member");
    let admin = h.sign_in(&admin_token).await;
    let member = h.sign_in(&member_token).await;
    let onboarding = || async {
        let response = h.api_get("/api/orgs/globex/onboarding", &admin).await;
        assert_eq!(response.status(), 200);
        response.json::<Value>().await.unwrap()
    };

    let doc = onboarding().await;
    assert_eq!(doc["org"], json!({ "id": globex.id, "login": "globex" }));
    assert_eq!(doc["suggested_slug"], "globex");
    assert_eq!(
        steps(&doc),
        [
            ("reader_app".to_owned(), "todo".to_owned()),
            ("storage_repo".into(), "todo".into()),
            ("storage_app".into(), "todo".into()),
            ("settings".into(), "blocked".into()),
            ("plan".into(), "done".into()),
        ]
    );
    // Members are counted with the reader App, which is not installed yet.
    assert_eq!(
        doc["steps"][4]["detail"],
        "Free for organisations with up to 5 members"
    );
    assert_eq!(
        doc["steps"][0]["action_url"],
        format!("{}/apps/privatecrates-reader/installations/new", h.fake.url)
    );
    // Our Apps cannot create repositories, so the step links to GitHub's create page, filled in.
    assert_eq!(
        doc["steps"][1]["action_url"],
        format!(
            "{}/new?owner=globex&name=crates-store&visibility=private&description=PrivateCrates+registry+storage%3A+the+index+and+crate+releases",
            h.fake.url
        )
    );
    assert!(
        doc["steps"][1]["detail"]
            .as_str()
            .unwrap()
            .contains("globex/crates-store")
    );
    assert_eq!(
        doc["steps"][2]["action_url"],
        format!(
            "{}/apps/privatecrates-storage/installations/new",
            h.fake.url
        )
    );

    // Once the reader App is installed it can see the new repository: the step is done, and the storage App's install
    // link pre-selects it.
    h.fake.install_app(&globex, READER_APP_ID);
    let doc = onboarding().await;
    assert_eq!(statuses(&doc), ["done", "done", "todo", "blocked", "done"]);
    assert_eq!(doc["steps"][4]["detail"], "Free: 2 of 5 members");
    assert_eq!(
        doc["steps"][1]["action_url"],
        format!("{}/globex/crates-store/settings", h.fake.url)
    );
    assert_eq!(
        doc["steps"][2]["action_url"],
        format!(
            "{}/apps/privatecrates-storage/installations/new/permissions?suggested_target_id={}&repository_ids[]={}",
            h.fake.url, globex.id, globex.storage_repo
        )
    );
    h.fake.install_app(&globex, STORAGE_APP_ID);
    assert_eq!(
        statuses(&onboarding().await),
        ["done", "done", "done", "todo", "done"]
    );

    // A member sees the same checklist, with what is left blocked.
    let response = h.api_get("/api/orgs/globex/onboarding", &member).await;
    let doc: Value = response.json().await.unwrap();
    assert_eq!(statuses(&doc), ["done", "done", "done", "blocked", "done"]);
    assert!(doc["steps"][3]["detail"].as_str().unwrap().contains("ask"));

    // Not a member.
    let response = h.api_get("/api/orgs/acme/onboarding", &admin).await;
    assert_eq!(response.status(), 404);
    assert_eq!(error_code(response).await, "account::org_not_found");
    let response = h.api_get("/api/orgs/..%2Fuser/onboarding", &admin).await;
    assert_eq!(response.status(), 404);
    let response = h.api_get("/api/orgs/globex/onboarding", "").await;
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn settings_create_the_registry() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    h.fake.install_app(&globex, READER_APP_ID);
    h.fake.install_app(&globex, STORAGE_APP_ID);
    let admin_token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&admin_token, &globex, "admin");
    let member_token = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&member_token, &globex, "member");
    let admin = h.sign_in(&admin_token).await;
    let member = h.sign_in(&member_token).await;
    let settings = |session: &str, slug: &str| {
        h.api_post("/api/orgs/globex/settings", session)
            .body(json!({ "slug": slug }).to_string())
            .send()
    };

    let response = settings(&member, "globex").await.unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(error_code(response).await, "account::admin_required");
    for (slug, status, code) in [
        ("Globex", 400, "account::slug_invalid"),
        ("-globex", 400, "account::slug_invalid"),
        ("glo.bex", 400, "account::slug_invalid"),
        ("", 400, "account::slug_invalid"),
        ("www", 400, "account::slug_reserved"),
        ("billing", 400, "account::slug_reserved"),
        ("acme", 409, "account::slug_taken"),
    ] {
        let response = settings(&admin, slug).await.unwrap();
        assert_eq!(response.status(), status, "{slug}");
        assert_eq!(error_code(response).await, code, "{slug}");
    }
    let response = h
        .api_post("/api/orgs/globex/settings", &admin)
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(error_code(response).await, "account::slug_invalid");
    assert!(
        h.fake
            .file(globex.storage_repo, "privatecrates.toml")
            .is_none()
    );

    let response = settings(&admin, "globex").await.unwrap();
    assert_eq!(response.status(), 200);
    let doc: Value = response.json().await.unwrap();
    assert_eq!(statuses(&doc), ["done", "done", "done", "done", "done"]);
    let file = h
        .fake
        .file(globex.storage_repo, "privatecrates.toml")
        .unwrap();
    assert!(file.ends_with("slug = \"globex\"\n"), "{file}");
    let commit = h.fake.commits(globex.storage_repo).pop().unwrap();
    assert_eq!(commit.path, "privatecrates.toml");
    assert_eq!(commit.by_installation, Some(globex.storage_installation));
    assert!(commit.message.contains("alice"), "{}", commit.message);

    // The registry is served at once.
    let registry = h
        .client
        .get(format!(
            "http://globex.localhost:{}/index/config.json",
            h.port
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(registry.status(), 401);
    let session: Value = h
        .api_get("/api/session", &admin)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(session["orgs"][0]["tenant"]["slug"], "globex");

    // The settings are created once; changes are the organisation's own pull requests.
    let response = settings(&admin, "globex2").await.unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "account::settings_exist");
}

#[tokio::test]
async fn settings_are_never_overwritten() {
    let h = Harness::start().await;
    let initech = h.fake.add_org_without_apps("initech");
    h.fake.install_app(&initech, STORAGE_APP_ID);
    // Written by a person before the reader App was installed, so the organisation is not a tenant yet.
    h.fake.write_file(
        initech.storage_repo,
        "privatecrates.toml",
        "slug = \"initech\"\n",
    );
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &initech, "admin");
    let session = h.sign_in(&token).await;
    let response = h
        .api_post("/api/orgs/initech/settings", &session)
        .body(json!({ "slug": "other" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "account::settings_exist");
    assert_eq!(
        h.fake
            .file(initech.storage_repo, "privatecrates.toml")
            .unwrap(),
        "slug = \"initech\"\n"
    );

    // Once the reader App is installed, the checklist notices the existing settings.
    h.fake.install_app(&initech, READER_APP_ID);
    let doc: Value = h
        .api_get("/api/orgs/initech/onboarding", &session)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(statuses(&doc)[3], "done");
    assert_eq!(doc["suggested_slug"], "initech");
}

#[tokio::test]
async fn storage_must_be_ready_before_settings() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    h.fake.install_app(&globex, READER_APP_ID);
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &globex, "admin");
    let session = h.sign_in(&token).await;
    let response = h
        .api_post("/api/orgs/globex/settings", &session)
        .body(json!({ "slug": "globex" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(error_code(response).await, "account::storage_not_ready");
}

#[tokio::test]
async fn the_error_catalog_is_published() {
    let h = Harness::start().await;
    let catalog: Vec<Value> = h
        .client
        .get(h.apex("/api/errors"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let find = |code: &str| {
        catalog
            .iter()
            .find(|e| e["code"] == code)
            .unwrap_or_else(|| panic!("{code} missing"))
            .clone()
    };
    assert_eq!(find("billing::subscription_inactive")["http_status"], 402);
    assert_eq!(find("publish::ci_only")["http_status"], 403);
    assert!(
        find("publish::ci_only")["message"]
            .as_str()
            .unwrap()
            .contains("{name}")
    );
    // Internal error types are not part of the public catalog.
    assert!(!catalog.iter().any(|e| e["code"] == "github::unauthorized"));
}

#[tokio::test]
async fn tools_use_a_bearer_token_without_the_csrf_check() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    h.fake.install_app(&globex, READER_APP_ID);
    h.fake.install_app(&globex, STORAGE_APP_ID);
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &globex, "admin");
    let bearer = format!("Bearer {token}");

    let session: Value = h
        .client
        .get(h.apex("/api/session"))
        .header("Authorization", &bearer)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(session["user"]["login"], "alice");
    assert_eq!(session["orgs"][0]["login"], "globex");

    // No cookie, no Origin, no JSON content type: a bearer token is not an ambient credential.
    let response = h
        .client
        .post(h.apex("/api/orgs/globex/settings"))
        .header("Authorization", &bearer)
        .body(json!({ "slug": "globex" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().get("set-cookie").is_none());
    let doc: Value = response.json().await.unwrap();
    assert_eq!(statuses(&doc), ["done", "done", "done", "done", "done"]);

    // A cookie session is still checked.
    let (_, session) = admin(&h).await;
    let response = h
        .client
        .post(h.apex("/api/orgs/acme/trial"))
        .header("Cookie", &session)
        .header("Origin", "http://evil.example")
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(error_code(response).await, "account::cross_site_request");
}

#[tokio::test]
async fn the_account_api_refuses_other_tokens() {
    let h = Harness::start().await;
    let (_, session) = admin(&h).await;
    let oauth = h.fake.add_user("bob", "gho_", &[]);
    h.fake.add_member(&oauth, &h.org, "admin");
    for authorization in [
        format!("Bearer {oauth}"),
        "Bearer ghp_classic".into(),
        "token github_pat_fine_grained".into(),
        "Bearer ghs_installation".into(),
        "Bearer pcr_eyJ.a.b".into(),
        "Bearer eyJhbGc.eyJzdWI.sig".into(),
        "Bearer ".into(),
    ] {
        for request in [
            h.client.get(h.apex("/api/session")),
            h.client.get(h.apex("/api/orgs/acme/onboarding")),
            h.client.post(h.apex("/api/orgs/acme/trial")).body("{}"),
        ] {
            // A valid session cookie alongside does not help: the header alone authenticates.
            let response = request
                .header("Authorization", &authorization)
                .header("Cookie", &session)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 401, "{authorization}");
            assert_eq!(
                error_code(response).await,
                "account::token_not_accepted",
                "{authorization}"
            );
        }
    }
}

#[tokio::test]
async fn an_expired_bearer_token_must_sign_in_again() {
    let h = Harness::start().await;
    for path in ["/api/session", "/api/orgs/acme/onboarding"] {
        let response = h
            .client
            .get(h.apex(path))
            .header("Authorization", "Bearer ghu_revoked")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{path}");
        assert_eq!(error_code(response).await, "account::sign_in_required");
    }
}

#[tokio::test]
async fn the_apex_serves_the_device_flow_client() {
    let h = Harness::start().await;
    let body: Value = h
        .client
        .get(h.apex("/api/v1/auth"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body,
        json!({ "github_client_id": privatecrates_testkit::READER_CLIENT_ID, "github_url": h.fake.url })
    );
}
