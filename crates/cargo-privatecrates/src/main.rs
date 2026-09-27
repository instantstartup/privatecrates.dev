//! `cargo privatecrates`: sets up PrivateCrates private registries from the command line, for people and for coding
//! agents (docs/agent-onboarding.md).
//!
//! - `login` / `logout`: GitHub's device flow, sharing the credential provider's token store.
//! - `setup <org>`: the organisation's onboarding checklist, and the steps the account API can do.
//! - `terms <org>`: whether the organisation has accepted the PrivateCrates terms, and accepting them.
//! - `init`: configures a crate repository or workspace to use and publish to a registry.
//! - `doctor`: checks that configuration, and says how to fix what is missing.
//!
//! Every command takes `--json` for machine-readable output, and exits non-zero on failure.

mod account;
mod doctor;
mod error;
mod init;
mod project;
mod target;

use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;

use crate::{error::Error, target::Domain};

/// Cargo runs `cargo-privatecrates privatecrates …` for `cargo privatecrates …`.
#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    Privatecrates(Cli),
}

/// Set up PrivateCrates private registries: sign in, onboard an organisation, configure crate repositories to
/// publish from GitHub Actions, and check the result.
#[derive(clap::Args)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Print JSON on standard output, for scripts and coding agents.
    #[arg(long, global = true)]
    json: bool,

    /// The PrivateCrates deployment: privatecrates.dev (the default), dev.privatecrates.dev, or a URL such as
    /// http://localhost:8080 for a local server.
    #[arg(long, global = true, value_name = "DOMAIN")]
    domain: Option<Domain>,

    /// A registry's URL, such as https://acme.privatecrates.dev, in place of --domain (and of the name in --registry
    /// for `init`).
    #[arg(long, global = true, value_name = "URL", conflicts_with = "domain")]
    url: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in with GitHub. Prints a code and a link to approve it; the token is kept in the system keyring, shared
    /// with cargo-credential-privatecrates.
    Login,
    /// Forget the stored token.
    Logout,
    /// Show an organisation's set-up checklist, and do the steps the account API can: choose the registry name and
    /// start the trial. Needs `login` first.
    Setup {
        /// The GitHub organisation.
        org: String,
        /// Save the registry name: its hostname and its name in Cargo. Needs --accept-terms.
        #[arg(long, value_name = "NAME")]
        slug: Option<String>,
        /// Accept the PrivateCrates terms on behalf of the organisation: the version shown with them. Only an admin
        /// who has read the terms passes this; an agent must never pass it on their behalf.
        #[arg(long, value_name = "VERSION")]
        accept_terms: Option<String>,
        /// With --slug: let developers also publish from their own machines, first versions included (no provenance).
        /// The default for every repository, saved in privatecrates.toml. Without it, crates are published from
        /// GitHub Actions only. The admin's choice: an agent asks them.
        #[arg(long, requires = "slug")]
        allow_manual_publish: bool,
        /// Start the no-card free trial (organisations over the free member limit). Needs --billing-email.
        #[arg(long)]
        start_trial: bool,
        /// Where Stripe sends the reminder before the trial ends, and the invoices after it.
        #[arg(long, value_name = "EMAIL", requires = "start_trial")]
        billing_email: Option<String>,
    },
    /// Show whether an organisation has accepted the current PrivateCrates terms, or accept them for a registry
    /// set up before them. Exits non-zero while they are not accepted. Needs `login` first.
    Terms {
        /// The GitHub organisation.
        org: String,
        /// Accept the terms on behalf of the organisation: the version shown with them. Only an admin who has read
        /// the terms passes this; an agent must never pass it on their behalf.
        #[arg(long, value_name = "VERSION")]
        accept: Option<String>,
    },
    /// Configure the crate repository or workspace in the current directory: the registry in
    /// .cargo/config.toml, package.repository from the `origin` remote, and a publish workflow.
    Init {
        /// The registry's name.
        #[arg(long, value_name = "NAME")]
        registry: String,
        /// The workflow file to write in .github/workflows.
        #[arg(long, value_name = "FILE", default_value = "publish.yml")]
        workflow_name: String,
        /// Replace a registry configuration or workflow that differs.
        #[arg(long)]
        force: bool,
        /// Apply the changes without asking (for scripts and coding agents, once a person has seen the plan).
        #[arg(long, short = 'y')]
        yes: bool,
        /// Show the changes and write nothing.
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
        /// Leave out the GitHub Actions workflow that publishes.
        #[arg(long)]
        no_workflow: bool,
    },
    /// Add the verifier workflow to an organisation's storage repository, with your own `gh` login: it checks the
    /// registry independently of PrivateCrates on each publish and daily. Shows what it will commit, and asks.
    AddVerifier {
        /// The GitHub organisation.
        org: String,
        /// Commit it without asking (for scripts and coding agents, once a person has agreed).
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Check the current crate repository: the credential provider, the registry, package.repository, the publish
    /// workflow and the published versions.
    Doctor {
        /// A registry to check (repeatable); all those using cargo-credential-privatecrates by default.
        #[arg(long = "registry", value_name = "NAME")]
        registries: Vec<String>,
        /// A crate to look up as a developer using it (repeatable): whether you can see it, and why not. Skips the
        /// publishing checks.
        #[arg(long = "crate", value_name = "NAME")]
        crates: Vec<String>,
    },
}

/// A command's result, for people and as JSON.
struct Outcome {
    json: serde_json::Value,
    text: String,
    ok: bool,
}

impl Outcome {
    fn new(report: &(impl Serialize + std::fmt::Display), ok: bool) -> Self {
        Self {
            json: serde_json::to_value(report).expect("reports serialise"),
            text: report.to_string(),
            ok,
        }
    }
}

impl Cli {
    /// The deployment: from --domain, or the one --url's registry belongs to.
    fn domain(&self) -> Result<Domain, Error> {
        match (&self.url, &self.domain) {
            (Some(url), _) => Domain::of_registry(url).ok_or_else(|| {
                Error::Invalid(format!(
                    "`{url}` is not a registry URL such as https://acme.privatecrates.dev"
                ))
            }),
            (None, Some(domain)) => Ok(domain.clone()),
            (None, None) => Ok(Domain::default()),
        }
    }
}

fn run(cli: &Cli) -> Result<Outcome, Error> {
    let domain = cli.domain()?;
    let cwd = std::env::current_dir().map_err(Error::io("read", PathBuf::from(".")))?;
    Ok(match &cli.command {
        Command::Login => {
            let login = account::login(&domain)?;
            let text = match &login.user {
                Some(user) => format!("Signed in to {} as {user}.", login.apex),
                None => format!("Signed in to {}.", login.apex),
            };
            Outcome {
                json: serde_json::to_value(&login).expect("serialisable"),
                text,
                ok: true,
            }
        }
        Command::Logout => {
            let logout = account::logout(&domain)?;
            Outcome {
                text: format!("Signed out of {}.", logout.apex),
                json: serde_json::to_value(&logout).expect("serialisable"),
                ok: true,
            }
        }
        Command::Setup {
            org,
            slug,
            accept_terms,
            allow_manual_publish,
            start_trial,
            billing_email,
        } => {
            let setup = account::setup(
                &domain,
                &account::SetupOptions {
                    org,
                    slug: slug.as_deref(),
                    accept_terms: accept_terms.as_deref(),
                    allow_manual_publish: *allow_manual_publish,
                    start_trial: *start_trial,
                    billing_email: billing_email.as_deref(),
                },
            )?;
            Outcome::new(&setup, true)
        }
        Command::Terms { org, accept } => {
            let report = account::terms(&domain, org, accept.as_deref())?;
            let accepted = report.terms.accepted;
            Outcome::new(&report, accepted)
        }
        Command::Init {
            registry,
            workflow_name,
            force,
            yes,
            dry_run,
            no_workflow,
        } => {
            let url = match &cli.url {
                Some(url) => url.trim_end_matches('/').to_owned(),
                None => domain.registry(registry),
            };
            let (mut report, mut staged) = init::run(
                &cwd,
                &init::Options {
                    slug: registry,
                    url: &url,
                    workflow_name,
                    workflow: !no_workflow,
                    force: *force,
                },
            )?;
            if !report.has_changes() || *dry_run {
                return Ok(Outcome::new(&report, true));
            }
            if !yes {
                // Without a person to ask, show the plan and write nothing.
                if cli.json || !std::io::stdin().is_terminal() {
                    let mut outcome = Outcome::new(&report, false);
                    outcome
                        .text
                        .push_str(" Run it again with --yes to apply these changes.");
                    return Ok(outcome);
                }
                eprintln!("{report}\n");
                if let Some(change) = report.workflow_change() {
                    let path = change
                        .path
                        .strip_prefix(&report.root)
                        .unwrap_or(&change.path);
                    eprintln!(
                        "{} makes GitHub Actions publish your crates when you push a version tag. The job's OIDC \
                         token is the credential, so there are no secrets to store, and every version gets \
                         provenance signed by GitHub. A crate's first version can only be published this way.",
                        path.display()
                    );
                    if !confirm(&format!("Write {}?", path.display()))? {
                        report.drop_workflow(&mut staged);
                    }
                }
                if !report.has_changes() {
                    return Ok(Outcome::new(&report, true));
                }
                if !confirm("Apply these changes?")? {
                    return Ok(Outcome {
                        json: serde_json::to_value(&report).expect("reports serialise"),
                        text: "Nothing was written.".into(),
                        ok: false,
                    });
                }
            }
            staged.apply()?;
            report.applied = true;
            Outcome::new(&report, true)
        }
        Command::AddVerifier { org, yes } => {
            let verifier = account::verifier(&domain, org)?;
            let json = |added: bool| {
                json!({
                    "installed": verifier.installed || added,
                    "added": added,
                    "repository": verifier.repository,
                    "path": verifier.path,
                    "add_url": verifier.add_url,
                })
            };
            if verifier.installed {
                return Ok(Outcome {
                    json: json(false),
                    text: format!("{} already runs the verifier.", verifier.repository),
                    ok: true,
                });
            }
            let plan = format!(
                "This commits {} to {} with your own gh login. It runs privatecrates-verify on each push and daily, \
                 about a minute each time:\n\n{}",
                verifier.path, verifier.repository, verifier.workflow
            );
            if !yes {
                if cli.json || !std::io::stdin().is_terminal() {
                    return Ok(Outcome {
                        json: json(false),
                        text: format!(
                            "{plan}\nNothing was committed. Run it again with --yes to commit it, or add it in the \
                             browser: {}",
                            verifier.add_url
                        ),
                        ok: false,
                    });
                }
                eprintln!("{plan}");
                if !confirm(&format!(
                    "Commit {} to {}?",
                    verifier.path, verifier.repository
                ))? {
                    return Ok(Outcome {
                        json: json(false),
                        text: "Nothing was committed.".into(),
                        ok: false,
                    });
                }
            }
            account::add_verifier(&verifier)?;
            Outcome {
                json: json(true),
                text: format!(
                    "Added {} to {}. It runs on the next publish; to run it now, open the repository's Actions tab.",
                    verifier.path, verifier.repository
                ),
                ok: true,
            }
        }
        Command::Doctor { registries, crates } => {
            let report = doctor::run(
                &cwd,
                &doctor::Options {
                    registries,
                    crates,
                    domain: &domain,
                    path: std::env::var_os("PATH"),
                },
            );
            let ok = report.ok;
            Outcome::new(&report, ok)
        }
    })
}

/// Asks a yes-or-no question on the terminal; Enter means yes.
fn confirm(question: &str) -> Result<bool, Error> {
    loop {
        eprint!("{question} [Y/n] ");
        std::io::stderr().flush().ok();
        let mut answer = String::new();
        let read = std::io::stdin()
            .read_line(&mut answer)
            .map_err(Error::io("read", PathBuf::from("standard input")))?;
        if read == 0 {
            return Ok(false);
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {}
        }
    }
}

fn main() -> ExitCode {
    let Cargo::Privatecrates(cli) = Cargo::parse();
    match run(&cli) {
        Ok(outcome) => {
            if cli.json {
                println!("{:#}", outcome.json);
            } else {
                println!("{}", outcome.text);
            }
            if outcome.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            if cli.json {
                let mut error = json!({ "message": e.to_string(), "code": e.code() });
                if let Some(terms) = e.terms() {
                    error["terms"] = terms;
                }
                println!("{:#}", json!({ "error": error }));
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}
