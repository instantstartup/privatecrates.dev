//! The workflow that runs `privatecrates-verify` in a storage repository (SPEC §10.3). An admin adds it with their
//! own GitHub account, never our Apps: it checks us, so we do not write or maintain it.

/// Where the workflow is added, in the storage repository.
pub const WORKFLOW_PATH: &str = ".github/workflows/privatecrates-verify.yml";

/// What identifies a workflow that runs the verifier.
pub const VERIFIER: &str = "privatecrates-verify";

/// The workflow for a registry (`https://acme.privatecrates.dev`). It runs on every push (each publish, yank and
/// settings change is one) and daily (release changes are not pushes), with the verifier pinned to this version and
/// installed from its release, checked: well under a minute a run.
pub fn workflow(registry_url: &str) -> String {
    let install = crate::install::ci_step(VERIFIER);
    format!(
        r#"# Checks this PrivateCrates registry independently of PrivateCrates: every crate file is an immutable release
# that matches the index, and every version's provenance is signed by GitHub. Findings fail the run.
name: verify registry
on:
  push:
  schedule: [{{ cron: "17 4 * * *" }}]   # daily, for changes to releases
  workflow_dispatch:
permissions:
  contents: read
concurrency: {{ group: verify, cancel-in-progress: true }}
jobs:
  verify:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v5
        with: {{ fetch-depth: 0 }}
      - uses: actions/cache@v4
        with: {{ path: .privatecrates-verify.json, key: "verify-${{{{ github.run_id }}}}", restore-keys: verify- }}
{install}      - run: privatecrates-verify --registry {registry_url}
        env: {{ GITHUB_TOKEN: "${{{{ github.token }}}}" }}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workflow_pins_this_version() {
        let workflow = workflow("https://acme.privatecrates.dev");
        assert!(workflow.contains(&crate::install::ci_step(VERIFIER)));
        assert!(workflow.contains(
            "      - run: privatecrates-verify --registry https://acme.privatecrates.dev\n"
        ));
        assert!(workflow.contains(VERIFIER));
    }
}
