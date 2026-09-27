//! `cargo privatecrates login` and `setup` against a real server and a fake GitHub.

#[path = "../../privatecrates-server/tests/common/mod.rs"]
mod common;

use std::{path::Path, process::Output};

use common::{Harness, Options};
use privatecrates_testkit::{READER_APP_ID, STORAGE_APP_ID};
use serde_json::Value;

const CLI: &str = env!("CARGO_BIN_EXE_cargo-privatecrates");

/// Runs `cargo privatecrates …` with its tokens in `config`.
async fn cli(config: &Path, args: &[&str]) -> Output {
    tokio::process::Command::new(CLI)
        .arg("privatecrates")
        .args(args)
        .env("PRIVATECRATES_CREDENTIAL_STORE", "file")
        .env("PRIVATECRATES_CONFIG_DIR", config)
        .output()
        .await
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            stdout(output),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn statuses(setup: &Value) -> Vec<(String, String)> {
    setup["steps"]
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

#[tokio::test]
async fn an_admin_sets_up_a_registry() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &globex, "admin");
    h.fake.set_device_flow_user(&token);
    let config = tempfile::tempdir().unwrap();
    let domain = h.apex("");
    let with_domain = |args: &[&'static str]| {
        let mut all: Vec<&str> = args.to_vec();
        all.extend(["--domain", domain.as_str()]);
        all
    };

    // Not signed in yet: setup says how to sign in, and starts nothing interactive.
    let output = cli(config.path(), &with_domain(&["setup", "globex"])).await;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "run `cargo privatecrates login --domain {domain}`"
        )),
        "{stderr}"
    );

    let output = cli(config.path(), &with_domain(&["login", "--json"])).await;
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(json(&output)["user"], "alice");
    assert!(String::from_utf8_lossy(&output.stderr).contains("enter the code WDJB-MJHT"));
    // Shared with the credential provider: its store, keyed by the apex.
    let stored = std::fs::read_to_string(config.path().join("credentials.json")).unwrap();
    assert!(stored.contains(&format!("\"{domain}\"")), "{stored}");

    // Nothing installed: the Apps need a person; the storage repository is the admin's own gh commands.
    let output = cli(config.path(), &with_domain(&["setup", "globex"])).await;
    assert!(output.status.success(), "{:?}", output);
    let text = stdout(&output);
    for expected in [
        "[to do] Install the reader App",
        "A person must do this on GitHub",
        &format!("{}/apps/privatecrates-reader/installations/new", h.fake.url),
        "gh repo create globex/crates-store --private",
        "gh api -X PUT repos/globex/crates-store/immutable-releases",
        "[blocked] Choose the registry name",
    ] {
        assert!(text.contains(expected), "{expected} in:\n{text}");
    }

    // Bearer requests need no CSRF headers, and the JSON has the onboarding document's shape.
    h.fake.install_app(&globex, READER_APP_ID);
    h.fake.install_app(&globex, STORAGE_APP_ID);
    let output = cli(config.path(), &with_domain(&["setup", "globex", "--json"])).await;
    let setup = json(&output);
    assert_eq!(setup["org"]["login"], "globex");
    assert_eq!(setup["suggested_slug"], "globex");
    assert_eq!(setup["registry_url"], Value::Null);
    let settings = &setup["steps"][3];
    assert_eq!(settings["id"], "settings");
    assert_eq!(settings["status"], "todo");
    assert_eq!(settings["detail"], "Choose your registry name.");
    assert_eq!(
        settings["commands"][0],
        format!("cargo privatecrates setup globex --slug globex --domain {domain}")
    );
    assert!(setup["steps"][0]["action_url"].as_str().is_some());

    let output = cli(
        config.path(),
        &with_domain(&["setup", "globex", "--slug", "globex-crates", "--json"]),
    )
    .await;
    assert!(output.status.success(), "{:?}", output);
    let setup = json(&output);
    assert_eq!(setup["performed"], serde_json::json!(["settings"]));
    assert!(
        statuses(&setup).iter().all(|(_, s)| s == "done"),
        "{setup:#}"
    );
    assert_eq!(
        setup["registry_url"],
        format!("http://globex-crates.localhost:{}", h.port)
    );
    assert!(
        h.fake
            .file(globex.storage_repo, "privatecrates.toml")
            .unwrap()
            .contains("slug = \"globex-crates\"")
    );

    // Again: nothing to do. Another name: refused, since only a pull request renames a registry.
    let output = cli(
        config.path(),
        &with_domain(&["setup", "globex", "--slug", "globex-crates", "--json"]),
    )
    .await;
    assert!(output.status.success());
    assert_eq!(json(&output)["performed"], serde_json::json!([]));
    let output = cli(
        config.path(),
        &with_domain(&["setup", "globex", "--slug", "globex2", "--json"]),
    )
    .await;
    assert!(!output.status.success());
    let error = json(&output);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already named globex-crates"),
        "{error}"
    );
    let output = cli(config.path(), &with_domain(&["setup", "globex"])).await;
    assert!(
        stdout(&output).contains(&format!(
            "The registry is live: http://globex-crates.localhost:{}",
            h.port
        )),
        "{}",
        stdout(&output)
    );

    // --url names a registry of the same deployment.
    let url = h.base();
    let output = cli(config.path(), &["setup", "acme", "--url", &url, "--json"]).await;
    assert!(!output.status.success());
    assert_eq!(json(&output)["error"]["code"], "account::org_not_found");

    let output = cli(config.path(), &with_domain(&["logout"])).await;
    assert!(output.status.success());
    let output = cli(config.path(), &with_domain(&["setup", "globex"])).await;
    assert!(!output.status.success());
}

#[tokio::test]
async fn members_see_the_checklist_but_cannot_act() {
    let h = Harness::start().await;
    let globex = h.fake.add_org_without_apps("globex");
    h.fake.install_app(&globex, READER_APP_ID);
    h.fake.install_app(&globex, STORAGE_APP_ID);
    h.fake.add_org_members(&globex, 10);
    let token = h.fake.add_user("bob", "ghu_", &[]);
    h.fake.add_member(&token, &globex, "member");
    h.fake.set_device_flow_user(&token);
    let config = tempfile::tempdir().unwrap();
    let domain = h.apex("");
    let output = cli(config.path(), &["login", "--domain", &domain]).await;
    assert!(output.status.success(), "{:?}", output);
    assert!(
        stdout(&output).contains("Signed in to"),
        "{}",
        stdout(&output)
    );

    let output = cli(
        config.path(),
        &["setup", "globex", "--domain", &domain, "--json"],
    )
    .await;
    let setup = json(&output);
    assert_eq!(setup["steps"][3]["status"], "blocked");
    assert!(
        setup["steps"][3]["detail"]
            .as_str()
            .unwrap()
            .contains("ask one of them")
    );

    for args in [
        &["setup", "globex", "--slug", "globex", "--json"][..],
        &[
            "setup",
            "globex",
            "--start-trial",
            "--billing-email",
            "billing@globex.example",
            "--json",
        ][..],
    ] {
        let mut args = args.to_vec();
        args.extend(["--domain", domain.as_str()]);
        let output = cli(config.path(), &args).await;
        assert!(!output.status.success(), "{args:?}");
        assert_eq!(
            json(&output)["error"]["code"],
            "account::admin_required",
            "{args:?}"
        );
    }
}

#[tokio::test]
async fn an_admin_starts_the_trial_with_a_billing_email() {
    let h = Harness::start_with(Options {
        stripe: true,
        ..Options::default()
    })
    .await;
    let stripe = h.stripe.clone().unwrap();
    h.fake.add_org_members(&h.org, 11);
    let token = h.fake.add_user("alice", "ghu_", &[]);
    h.fake.add_member(&token, &h.org, "admin");
    h.fake.set_device_flow_user(&token);
    let config = tempfile::tempdir().unwrap();
    let domain = h.apex("");
    let with_domain = |args: &[&'static str]| {
        let mut all: Vec<&str> = args.to_vec();
        all.extend(["--domain", domain.as_str(), "--json"]);
        all
    };
    let output = cli(config.path(), &with_domain(&["login"])).await;
    assert!(output.status.success(), "{:?}", output);

    // The checklist says how, email included.
    let setup = json(&cli(config.path(), &with_domain(&["setup", "acme"])).await);
    let plan = &setup["steps"][4];
    assert_eq!(plan["status"], "todo");
    assert_eq!(
        plan["commands"][0],
        format!(
            "cargo privatecrates setup acme --start-trial --billing-email <EMAIL> --domain {domain}"
        )
    );

    // Without the email, nothing is sent.
    let output = cli(
        config.path(),
        &with_domain(&["setup", "acme", "--start-trial"]),
    )
    .await;
    assert!(!output.status.success());
    let error = json(&output);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--billing-email"),
        "{error}"
    );
    assert!(stripe.customers().is_empty());

    // The server checks the address.
    let output = cli(
        config.path(),
        &with_domain(&[
            "setup",
            "acme",
            "--start-trial",
            "--billing-email",
            "billing",
        ]),
    )
    .await;
    assert!(!output.status.success());
    assert_eq!(json(&output)["error"]["code"], "billing::email_invalid");

    let output = cli(
        config.path(),
        &with_domain(&[
            "setup",
            "acme",
            "--start-trial",
            "--billing-email",
            "billing@acme.example",
        ]),
    )
    .await;
    assert!(output.status.success(), "{:?}", output);
    let setup = json(&output);
    assert_eq!(setup["performed"], serde_json::json!(["trial"]));
    assert_eq!(setup["steps"][4]["status"], "done");
    let [customer]: [Value; 1] = stripe.customers().try_into().unwrap();
    assert_eq!(customer["email"], "billing@acme.example");

    // An email without the trial is a mistake in the command.
    let output = cli(
        config.path(),
        &with_domain(&["setup", "acme", "--billing-email", "billing@acme.example"]),
    )
    .await;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--start-trial"));
}
