// Code examples shared by the landing page, the docs and the account page, so they never drift apart.
// Derived from SPEC.md §3, §5 and the privatecrates-verify documentation.

/** The pinned cargo-binstall action. Customers pin it to a commit of their choice. */
const BINSTALL = 'cargo-bins/cargo-binstall@<commit sha>   # pin to a commit';

export function cargoConfig(name: string, registryUrl: string): string {
	return `[registries.${name}]
index = "sparse+${registryUrl.replace(/\/$/, '')}/index/"
credential-provider = ["cargo-credential-privatecrates"]`;
}

export function dependency(name: string): string {
	return `[dependencies]
story_engine = { version = "0.2", registry = "${name}" }`;
}

export function publishTarget(name: string): string {
	return `[package]
name = "story_engine"
version = "0.2.0"
# The owning repository: it must be in your organisation.
repository = "https://github.com/acme/story-engine"
# Publish only to your registry, never to crates.io by accident.
publish = ["${name}"]`;
}

export const installProvider = `cargo install cargo-credential-privatecrates --locked`;

export function loginCommands(name: string): string {
	return `# Sign in now (optional: the first build does it too)
cargo login --registry ${name}
# Forget the stored token
cargo logout --registry ${name}`;
}

export const ciBuild = `name: build
on: [push, pull_request]
permissions:
  id-token: write   # for the OIDC token
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: ${BINSTALL}
      - run: cargo binstall --no-confirm cargo-credential-privatecrates
      - run: cargo build --locked`;

export function ciPublish(name: string): string {
	return `name: publish
on:
  push:
    tags: ["v*"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: ${BINSTALL}
      - run: cargo binstall --no-confirm cargo-credential-privatecrates
      - run: cargo publish --registry ${name}`;
}

export function verifyWorkflow(registryUrl: string): string {
	return `name: verify registry
on:
  schedule: [{ cron: "17 * * * *" }]   # hourly
permissions:
  contents: read
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: actions/cache@v4
        with: { path: .privatecrates-verify.json, key: verify-\${{ github.run_id }}, restore-keys: verify- }
      - uses: ${BINSTALL}
      - run: cargo binstall --no-confirm privatecrates-verify
      - run: privatecrates-verify --registry ${registryUrl}
        env: { GITHUB_TOKEN: "\${{ github.token }}" }`;
}

export function settingsToml(slug: string): string {
	return `slug = "${slug}"             # your hostname
name_clash = "refuse"     # or "warn"
ci_read = "organisation"  # or "same-access"`;
}

export const ownersToml = `repository_id = 123456789               # survives renames
repository = "acme/story-engine"        # for humans
publish_workflows = ["publish.yml"]     # may publish
publish_environment = "crates"          # optional
allow_manual_publish = false            # the default`;

export const publishRefused = `error: story_engine is published from CI only, so every version has verifiable provenance.
       Add .github/workflows/publish.yml (see https://acme.privatecrates.dev/login#publish) and push a tag.`;
