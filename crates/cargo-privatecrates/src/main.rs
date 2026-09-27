//! `cargo privatecrates`: sets up PrivateCrates private registries from the command line, for people and for coding
//! agents (docs/agent-onboarding.md).
//!
//! - `login` / `logout`: GitHub's device flow, sharing the credential provider's token store.
//! - `setup <org>`: the organisation's onboarding checklist, and the steps the account API can do.
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

use std::{path::PathBuf, process::ExitCode};

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
        /// Save the registry name: its hostname and its name in Cargo.
        #[arg(long, value_name = "NAME")]
        slug: Option<String>,
        /// Start the no-card free trial (organisations over the free member limit). Needs --billing-email.
        #[arg(long)]
        start_trial: bool,
        /// Where Stripe sends the reminder before the trial ends, and the invoices after it.
        #[arg(long, value_name = "EMAIL", requires = "start_trial")]
        billing_email: Option<String>,
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
    },
    /// Check the current crate repository: the credential provider, the registry, package.repository, the publish
    /// workflow and the published versions.
    Doctor {
        /// A registry to check (repeatable); all those using cargo-credential-privatecrates by default.
        #[arg(long = "registry", value_name = "NAME")]
        registries: Vec<String>,
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
            start_trial,
            billing_email,
        } => {
            let setup = account::setup(
                &domain,
                &account::SetupOptions {
                    org,
                    slug: slug.as_deref(),
                    start_trial: *start_trial,
                    billing_email: billing_email.as_deref(),
                },
            )?;
            Outcome::new(&setup, true)
        }
        Command::Init {
            registry,
            workflow_name,
            force,
        } => {
            let url = match &cli.url {
                Some(url) => url.trim_end_matches('/').to_owned(),
                None => domain.registry(registry),
            };
            let report = init::run(
                &cwd,
                &init::Options {
                    slug: registry,
                    url: &url,
                    workflow_name,
                    force: *force,
                },
            )?;
            Outcome::new(&report, true)
        }
        Command::Doctor { registries } => {
            let report = doctor::run(
                &cwd,
                &doctor::Options {
                    registries,
                    domain: &domain,
                    path: std::env::var_os("PATH"),
                },
            );
            let ok = report.ok;
            Outcome::new(&report, ok)
        }
    })
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
                println!(
                    "{:#}",
                    json!({ "error": { "message": e.to_string(), "code": e.code() } })
                );
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}
