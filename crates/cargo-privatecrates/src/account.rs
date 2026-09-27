//! Signing in, and setting an organisation's registry up through the account API (docs/website-api.md), with the
//! reader App user token from the device flow as a bearer token.

use privatecrates_auth::{device, store::Store};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{error::Error, target::Domain};

#[derive(Serialize)]
pub struct Login {
    pub apex: String,
    /// The GitHub login signed in as.
    pub user: Option<String>,
}

/// Runs the device flow for the deployment's reader App and stores the token where the credential provider keeps
/// its own.
pub fn login(domain: &Domain) -> Result<Login, Error> {
    let apex = domain.apex();
    let http = crate::target::http(&apex)?;
    let stored = device::sign_in(&http, &apex, &Store::open()?)?;
    let api = Api {
        http,
        apex: apex.clone(),
        token: stored.access_token,
    };
    let session = api.get("/api/session")?;
    Ok(Login {
        user: session["user"]["login"].as_str().map(str::to_owned),
        apex,
    })
}

#[derive(Serialize)]
pub struct Logout {
    pub apex: String,
}

pub fn logout(domain: &Domain) -> Result<Logout, Error> {
    let apex = domain.apex();
    Store::open()?.delete(&apex)?;
    Ok(Logout { apex })
}

/// The account API, as the signed-in user.
struct Api {
    http: Client,
    apex: String,
    token: String,
}

impl Api {
    /// With the stored token, refreshed if it has expired. Never starts a sign-in: that is `login`'s job.
    fn signed_in(domain: &Domain) -> Result<Self, Error> {
        let apex = domain.apex();
        let http = crate::target::http(&apex)?;
        let stored =
            device::current(&http, &apex, &Store::open()?)?.ok_or_else(|| Error::NotSignedIn {
                apex: apex.clone(),
                domain_flag: domain.flag(),
            })?;
        Ok(Self {
            http,
            apex,
            token: stored.access_token,
        })
    }

    fn get(&self, path: &str) -> Result<Value, Error> {
        self.send(path, self.http.get(format!("{}{path}", self.apex)))
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, Error> {
        self.send(
            path,
            self.http.post(format!("{}{path}", self.apex)).json(body),
        )
    }

    fn send(&self, path: &str, request: RequestBuilder) -> Result<Value, Error> {
        let url = format!("{}{path}", self.apex);
        let response =
            request
                .bearer_auth(&self.token)
                .send()
                .map_err(|source| Error::Unreachable {
                    url: url.clone(),
                    source,
                })?;
        let status = response.status();
        let body: Value = response.json().unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(body);
        }
        let error = &body["errors"][0];
        Err(Error::Api {
            url,
            status: status.as_u16(),
            code: error["code"].as_str().map(str::to_owned),
            detail: error["detail"]
                .as_str()
                .unwrap_or_else(|| status.canonical_reason().unwrap_or("error"))
                .to_owned(),
        })
    }
}

pub struct SetupOptions<'a> {
    pub org: &'a str,
    pub slug: Option<&'a str>,
    pub start_trial: bool,
}

/// The onboarding document (`GET /api/orgs/{org}/onboarding`), with what was done and how to do the rest.
#[derive(Serialize)]
pub struct Setup {
    pub apex: String,
    pub org: Org,
    pub steps: Vec<Step>,
    pub suggested_slug: String,
    /// The registry, once its name is saved.
    pub registry_url: Option<String>,
    /// What this run did: `settings` and `trial`.
    pub performed: Vec<&'static str>,
}

#[derive(Serialize, Deserialize)]
pub struct Org {
    pub id: u64,
    pub login: String,
}

/// A step as the account API returns it, plus how to act on it.
#[derive(Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_url: Option<String>,
    /// Whether GitHub requires a person for it (there is no API to install an App).
    #[serde(default)]
    pub needs_person: bool,
    /// Commands that do it, with the admin's own `gh` login or this CLI.
    #[serde(default)]
    pub commands: Vec<String>,
}

impl Step {
    pub fn done(&self) -> bool {
        self.status == "done"
    }

    pub fn title(&self) -> &str {
        match self.id.as_str() {
            "reader_app" => "Install the reader App",
            "storage_repo" => "Create the storage repository",
            "storage_app" => "Install the storage App on the storage repository",
            "settings" => "Choose the registry name",
            "plan" => "Plan",
            other => other,
        }
    }
}

#[derive(Deserialize)]
struct Onboarding {
    org: Org,
    steps: Vec<Step>,
    suggested_slug: String,
}

/// The storage repository's conventional name, as the onboarding document suggests.
const STORAGE_REPO: &str = "crates-store";

pub fn setup(domain: &Domain, options: &SetupOptions<'_>) -> Result<Setup, Error> {
    let api = Api::signed_in(domain)?;
    let path = format!("/api/orgs/{}", options.org);
    let mut onboarding = api.get(&format!("{path}/onboarding"))?;
    let mut performed = Vec::new();
    let step_done = |doc: &Value, id: &str| {
        doc["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|s| s["id"] == id && s["status"] == "done")
    };
    if let Some(slug) = options.slug {
        if step_done(&onboarding, "settings") {
            let current = onboarding["suggested_slug"].as_str().unwrap_or_default();
            if current != slug {
                return Err(Error::Invalid(format!(
                    "{}'s registry is already named {current}; its name changes only by a pull request to \
                     privatecrates.toml in the storage repository",
                    options.org
                )));
            }
        } else {
            onboarding = api.post(&format!("{path}/settings"), &json!({ "slug": slug }))?;
            performed.push("settings");
        }
    }
    if options.start_trial && !step_done(&onboarding, "plan") {
        onboarding = api.post(&format!("{path}/trial"), &json!({}))?;
        performed.push("trial");
    }
    let onboarding: Onboarding = serde_json::from_value(onboarding).map_err(|e| Error::Api {
        url: format!("{}{path}/onboarding", api.apex),
        status: 200,
        code: None,
        detail: format!("unexpected onboarding document: {e}"),
    })?;
    Ok(describe(domain, onboarding, performed))
}

/// Adds to each step what a person or an agent does about it.
fn describe(domain: &Domain, onboarding: Onboarding, performed: Vec<&'static str>) -> Setup {
    let org = &onboarding.org.login;
    let cli = |args: String| format!("cargo privatecrates {args}{}", domain.flag());
    let storage_app_done = onboarding
        .steps
        .iter()
        .any(|s| s.id == "storage_app" && s.done());
    let settings_done = onboarding
        .steps
        .iter()
        .any(|s| s.id == "settings" && s.done());
    let steps = onboarding
        .steps
        .into_iter()
        .map(|mut step| {
            let todo = step.status == "todo";
            match step.id.as_str() {
                "reader_app" | "storage_app" => step.needs_person = !step.done(),
                // Our Apps cannot create repositories or turn immutable releases on (that needs administration
                // rights), so the admin does it with their own login.
                "storage_repo" if todo => {
                    step.commands = vec![
                        format!("gh repo create {org}/{STORAGE_REPO} --private"),
                        format!("gh api -X PUT repos/{org}/{STORAGE_REPO}/immutable-releases"),
                    ];
                }
                "storage_repo" if step.done() && !storage_app_done => {
                    step.commands = vec![format!(
                        "gh api -X PUT repos/{org}/{STORAGE_REPO}/immutable-releases"
                    )];
                }
                "settings" if todo => {
                    step.commands = vec![cli(format!(
                        "setup {org} --slug {}",
                        onboarding.suggested_slug
                    ))];
                }
                "plan" if todo => {
                    step.commands = vec![cli(format!("setup {org} --start-trial"))];
                }
                _ => {}
            }
            step
        })
        .collect();
    Setup {
        apex: domain.apex(),
        registry_url: settings_done.then(|| domain.registry(&onboarding.suggested_slug)),
        org: onboarding.org,
        steps,
        suggested_slug: onboarding.suggested_slug,
        performed,
    }
}

impl std::fmt::Display for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for action in &self.performed {
            match *action {
                "settings" => writeln!(f, "Saved the registry name {}.", self.suggested_slug)?,
                _ => writeln!(f, "Started the free trial.")?,
            }
        }
        writeln!(
            f,
            "Setting up PrivateCrates for {} ({}):",
            self.org.login, self.apex
        )?;
        for step in &self.steps {
            let status = match step.status.as_str() {
                "todo" => "to do",
                other => other,
            };
            writeln!(f, "\n  [{status}] {}", step.title())?;
            if let Some(detail) = &step.detail {
                writeln!(f, "    {detail}")?;
            }
            if step.needs_person {
                writeln!(
                    f,
                    "    A person must do this on GitHub, which has no API for it. Open:"
                )?;
                if let Some(url) = &step.action_url {
                    writeln!(f, "      {url}")?;
                }
                continue;
            }
            if !step.commands.is_empty() {
                let whose = if step.id == "storage_repo" {
                    " (with your own GitHub login)"
                } else {
                    ""
                };
                writeln!(f, "    Run{whose}:")?;
                for command in &step.commands {
                    writeln!(f, "      {command}")?;
                }
            }
            if let Some(url) = step.action_url.as_ref().filter(|_| !step.done()) {
                writeln!(f, "    Or open: {url}")?;
            }
        }
        match &self.registry_url {
            Some(url) if self.steps.iter().all(Step::done) => {
                write!(f, "\nThe registry is live: {url}")
            }
            Some(url) => write!(f, "\nRegistry: {url}"),
            None => write!(f, "\nSuggested registry name: {}", self.suggested_slug),
        }
    }
}
