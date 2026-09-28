//! `privatecrates-verify`: check a PrivateCrates storage repository for anything the service should not have done.
//!
//! Run it in the storage repository itself, on every push and daily: `cargo privatecrates add-verifier <org>` commits
//! the workflow, which is at <https://privatecrates.dev/docs/verify>.

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use privatecrates_verify::{Options, Severity, State, git::Git, remote::GitHub, verify};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// The registry's base URL, e.g. https://acme.privatecrates.dev
    #[arg(long)]
    registry: String,
    /// The storage repository, `owner/name`. Defaults to the repository the workflow runs in.
    #[arg(long, env = "GITHUB_REPOSITORY")]
    repo: String,
    /// A token that can read the storage repository.
    #[arg(long, env = "GITHUB_TOKEN", hide_env_values = true)]
    token: String,
    /// The local clone, with full history.
    #[arg(long, default_value = ".")]
    clone: PathBuf,
    /// Where to keep what has been verified, so later runs check only what is new.
    #[arg(long, default_value = ".privatecrates-verify.json")]
    state: PathBuf,
    /// The login of the registry's storage App bot.
    #[arg(long, default_value = "privatecrates-storage[bot]")]
    storage_app: String,
    #[arg(long, env = "GITHUB_API_URL", default_value = "https://api.github.com")]
    github_api: String,
    #[arg(long, default_value = "https://token.actions.githubusercontent.com")]
    oidc_issuer: String,
    #[arg(
        long,
        default_value = "https://token.actions.githubusercontent.com/.well-known/jwks"
    )]
    oidc_jwks: String,
    #[arg(long, default_value = "https://crates.io")]
    crates_io: String,
    /// Print the findings as JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let state: State = std::fs::read_to_string(&args.state)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let remote = GitHub {
        http: GitHub::client(),
        api: args.github_api.trim_end_matches('/').to_owned(),
        repo: args.repo,
        token: args.token,
        jwks_url: args.oidc_jwks,
        crates_io: args.crates_io.trim_end_matches('/').to_owned(),
    };
    let options = Options {
        base_url: args.registry.trim_end_matches('/').to_owned(),
        storage_app_login: args.storage_app,
        oidc_issuer: args.oidc_issuer,
    };
    let report = match verify(&Git::new(&args.clone), &remote, &options, &state) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.findings).expect("findings serialise")
        );
    } else if report.findings.is_empty() {
        println!("Verified: nothing to report.");
    } else {
        for f in &report.findings {
            let label = match f.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "warning",
                Severity::Info => "info",
            };
            println!("{label}: {}: {}", f.subject, f.message);
        }
    }
    if let Err(e) = std::fs::write(
        &args.state,
        serde_json::to_string_pretty(&report.state).expect("state serialises"),
    ) {
        eprintln!("warning: could not save {}: {e}", args.state.display());
    }
    if report.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
