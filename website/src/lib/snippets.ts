// Code examples shared by the landing page, the docs and the account page, so they never drift apart.
// Derived from SPEC.md §3, §5 and the privatecrates-verify documentation.

import { PREVIEW, SITE_URL, TERMS_PATH, TERMS_VERSION } from './site';

export function cargoConfig(name: string, registryUrl: string): string {
	return `[registries.${name}]
index = "sparse+${registryUrl.replace(/\/$/, '')}/index/"
credential-provider = ["cargo-credential-privatecrates"]`;
}

export function dependency(name: string): string {
	return `[dependencies]
story_engine = { version = "0.2", registry = "${name}" }`;
}

export function publishTarget(name: string, org = 'acme'): string {
	return `[package]
name = "story_engine"
version = "0.2.0"
# The owning repository: it must be in your organisation.
# In a monorepo, a link to the crate's directory works too.
repository = "https://github.com/${org}/story-engine"
# Publish only to your registry, never to crates.io by accident.
publish = ["${name}"]`;
}

export const installProvider = `cargo install cargo-credential-privatecrates --locked`;

/** Publishing is tag-triggered (see ciPublish). */
export const pushTag = `git tag v0.2.0
git push origin v0.2.0`;

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
      - run: cargo install cargo-credential-privatecrates --locked
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
      - run: cargo install cargo-credential-privatecrates --locked
      - run: cargo publish --registry ${name}`;
}

/** The workflow `cargo privatecrates init` writes for a workspace: one crate per `<crate>-v<version>` tag. */
export function ciPublishWorkspace(name: string): string {
	return `name: publish
on:
  push:
    tags: ["v*", "*-v*"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - run: cargo install cargo-credential-privatecrates --locked
      - name: cargo publish
        env:
          TAG: \${{ github.ref_name }}
        run: |
          case "$TAG" in
            v[0-9]*) cargo publish --workspace --registry ${name} ;;
            *-v[0-9]*) cargo publish --package "\${TAG%-v*}" --registry ${name} ;;
            *) echo "::error::$TAG is neither v<version> nor <crate>-v<version>"; exit 1 ;;
          esac`;
}

/** The verifier's version, pinned in its workflow: the client tools' release (privatecrates-common's verifier.rs). */
export const VERIFIER_VERSION = '0.2.5';

/** The same workflow as privatecrates-common's `verifier::workflow`, which the account page offers. */
export function verifyWorkflow(registryUrl: string): string {
	return `# Checks this PrivateCrates registry independently of PrivateCrates: every crate file is an immutable release
# that matches the index, and every version's provenance is signed by GitHub. Findings fail the run.
name: verify registry
on:
  push:
  schedule: [{ cron: "17 4 * * *" }]   # daily, for changes to releases
  workflow_dispatch:
permissions:
  contents: read
concurrency: { group: verify, cancel-in-progress: true }
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: actions/cache@v4
        with: { path: .privatecrates-verify.json, key: "verify-\${{ github.run_id }}", restore-keys: verify- }
      - uses: actions/cache@v4
        id: verifier
        with: { path: ~/.privatecrates-verify, key: "privatecrates-verify-${VERIFIER_VERSION}-\${{ runner.os }}" }
      - if: steps.verifier.outputs.cache-hit != 'true'
        run: cargo install privatecrates-verify --version ${VERIFIER_VERSION} --locked --root ~/.privatecrates-verify
      - run: ~/.privatecrates-verify/bin/privatecrates-verify --registry ${registryUrl}
        env: { GITHUB_TOKEN: "\${{ github.token }}" }
`;
}

export function settingsToml(slug: string): string {
	return `slug = "${slug}"                # your hostname
name_clash = "refuse"        # or "warn"
ci_read = "organisation"     # the only option today
allow_manual_publish = false # the default for every repository

# A repository's own setting, by its name
[repositories.tools]
allow_manual_publish = true`;
}

export const ownersToml = `repository_id = 123456789               # survives renames
repository = "acme/story-engine"        # for humans
publish_workflows = ["publish.yml"]     # may publish
publish_environment = "crates"          # optional
allow_manual_publish = false            # the default`;

export const publishRefused = `error: story_engine is published from GitHub Actions only, so every version can be traced to a
       commit. To publish it: in its repository run \`cargo privatecrates init --registry acme\`, merge the
       workflow it adds, then push the tag story_engine-v0.2.0 (in a workspace) or v0.2.0. An organisation
       admin can allow publishing from developers' machines instead, with allow_manual_publish = true in
       privatecrates.toml in the storage repository. See https://acme.privatecrates.dev/login#publish`;

// ---------------------------------------------------------------------------------------------------------------
// cargo privatecrates, the set-up CLI, and the prompts for AI coding agents (docs/agent-onboarding.md).

/** The production apex. Other environments pass `--domain` to every `cargo privatecrates` command. */
export const PROD_APEX = 'privatecrates.dev';

/** The storage repository name the docs and prompts suggest. */
export const STORAGE_REPO = 'crates-store';

const INSTALL_CLI = 'cargo install cargo-privatecrates --locked';

export const installCli = `${INSTALL_CLI}
# or, prebuilt and checksummed:
cargo binstall cargo-privatecrates`;

/** A `cargo privatecrates` command, with `--domain` added outside production. */
export function cli(args: string, apex = PROD_APEX): string {
	return `cargo privatecrates ${args}${apex === PROD_APEX ? '' : ` --domain ${apex}`}`;
}

/** The current terms, for pages that are built before any session exists. */
export const CURRENT_TERMS = { version: TERMS_VERSION, url: `${SITE_URL}${TERMS_PATH}` };

// The trial flags stay in the CLI for later; during the private preview the server refuses them (billing::preview).
export const cliUsage = `cargo privatecrates login
cargo privatecrates logout
cargo privatecrates setup <org> [--slug <name> --accept-terms <version> [--allow-manual-publish]]${PREVIEW ? '' : ' [--start-trial --billing-email <address>]'} [--json]
cargo privatecrates terms <org> --accept <version>
cargo privatecrates init --registry <name> [--dry-run | --yes] [--no-workflow] [--domain <domain> | --url <url>]
cargo privatecrates doctor [--json]`;

/** Creates the storage repository and turns on immutable releases, with the admin's own gh login. */
export function storageRepoCommands(org: string): string {
	return `gh repo create ${org}/${STORAGE_REPO} --private
gh api -X PUT repos/${org}/${STORAGE_REPO}/immutable-releases`;
}

export function loginCli(apex = PROD_APEX): string {
	return `# Prints a code and a GitHub link; approve it in the browser
${cli('login', apex)}
# Forget the stored token
${cli('logout', apex)}`;
}

export function setupCommands(
	org: string,
	slug: string,
	apex = PROD_APEX,
	terms = CURRENT_TERMS,
	preview = PREVIEW
): string {
	const trial = preview
		? ''
		: `
# Organisations over the free limit: start the no-card trial. Stripe sends the
# trial-ending reminder and invoices to the billing email.
${cli(`setup ${org} --start-trial --billing-email billing@example.com`, apex)}`;
	return `# The checklist: each step's status, with a link where a person must act
${cli(`setup ${org}`, apex)}
# Choose the registry name (once both Apps are installed), after an admin has read
# and accepted the terms at ${terms.url} on behalf of ${org}
${cli(`setup ${org} --slug ${slug} --accept-terms ${terms.version}`, apex)}
# An existing registry whose admin has not accepted the current terms yet
${cli(`terms ${org} --accept ${terms.version}`, apex)}${trial}
# The same checklist as JSON, for agents and scripts
${cli(`setup ${org} --json`, apex)}`;
}

export function initCommands(slug: string, apex = PROD_APEX): string {
	return `# In a crate repository or workspace
git switch -c privatecrates
${cli(`init --registry ${slug}`, apex)}
git add -A && git commit -m "Publish to the ${slug} registry"
gh pr create --fill`;
}

export function doctorCommands(apex = PROD_APEX): string {
	return `${cli('doctor', apex)}
# For agents and scripts
${cli('doctor --json', apex)}`;
}

/** One onboarding step, as the account API returns it. */
export interface PromptStep {
	id: string;
	status: 'done' | 'todo' | 'blocked';
	action_url?: string;
}

export interface SetupPromptInput {
	org: string;
	/** The registry name: chosen, or suggested by the server. */
	slug: string;
	/** The apex host, e.g. privatecrates.dev. */
	apex: string;
	/** The onboarding checklist, with absolute action URLs. */
	steps: PromptStep[];
	/** What the plan step needs, when it is not done: the no-card trial, or a subscription with a card. */
	plan: 'trial' | 'subscribe' | null;
	/** The terms the admin must accept before the registry is created (session.terms). */
	terms?: { version: string; url: string };
	/** The preview (session.preview): billing is off, so the prompt has no trial or billing step. */
	preview?: boolean;
	/** The user's own GitHub account rather than an organisation. */
	personal?: boolean;
}

const doneNames: Record<string, string> = {
	reader_app: 'the reader App is installed',
	storage_repo: 'the storage repository exists',
	storage_app: 'the storage App is installed',
	settings: 'the registry name is saved',
	plan: 'the plan is settled',
	subscription: 'the plan is settled'
};

function numbered(items: string[]): string {
	return items.map((item, i) => `${i + 1}. ${item}`).join('\n');
}

function environmentNote(apex: string): string {
	return apex === PROD_APEX
		? ''
		: `\nThis is the ${apex} environment: every cargo privatecrates command takes --domain ${apex}, as below.`;
}

function agentRules(org: string, check: string): string {
	return `Rules:
- Use my own gh login for repository administration (check it with gh auth status). Never ask me for a personal access token or any other GitHub token, and never create one.
- Where GitHub needs a person, stop: give me the link, say what to choose there, and wait until I say it is done. Then check with ${check}.
- Change nothing in ${org} beyond what these steps need.`;
}

/** The prompt that finishes setting up a registry, from wherever the checklist is. */
export function setupPrompt({
	org,
	slug,
	apex,
	steps,
	plan,
	terms = CURRENT_TERMS,
	preview = false,
	personal = false
}: SetupPromptInput): string {
	const step = (id: string) => steps.find((s) => s.id === id || (id === 'plan' && s.id === 'subscription'));
	const todo = (id: string) => {
		const s = step(id);
		return !!s && s.status !== 'done';
	};
	const account = `https://${apex}/account?org=${encodeURIComponent(org)}`;
	const link = (id: string) => step(id)?.action_url ?? account;
	const done = steps.filter((s) => s.status === 'done').map((s) => doneNames[s.id] ?? s.id);
	const makeRepo = todo('storage_repo');

	const items = [
		`Install the CLI if it is missing: ${INSTALL_CLI}`,
		`Sign in: ${cli('login', apex)}. It prints a code and a GitHub link; give me both and wait while I approve.`
	];
	if (todo('reader_app'))
		items.push(
			`Stop and ask me to install the PrivateCrates reader App on ${org}, on all repositories or on those that own crates: ${link('reader_app')}`
		);
	if (makeRepo)
		items.push(
			`Create the storage repository with my gh login, and turn on immutable releases:\n   ${storageRepoCommands(org).replace(/\n/g, '\n   ')}`
		);
	if (todo('storage_app'))
		items.push(
			`Stop and ask me to install the PrivateCrates storage App on ${makeRepo ? `${org}/${STORAGE_REPO}` : 'the storage repository'} only ("Only select repositories"): ${link('storage_app')}`
		);
	if (todo('settings'))
		items.push(
			`Show me the terms, ${terms.url}, and ask me to read them and accept them on behalf of ${org}. Wait until I say I accept. Never accept them for me, and do not go on if I decline. Then ask me who may publish crates: GitHub Actions only (recommended: every version is built from a commit, with provenance signed by GitHub), or also developers' own machines with cargo publish (no provenance). Only then choose the registry name: ${cli(`setup ${org} --slug ${slug} --accept-terms ${terms.version}`, apex)}, adding --allow-manual-publish only if I chose developers' machines`
		);
	if (!preview && todo('plan') && plan === 'trial')
		items.push(
			`Start the free trial, no card needed. First ask me for the billing email address: Stripe sends the trial-ending reminder and invoices there. Do not guess it or take it from git config. Then run ${cli(`setup ${org} --start-trial --billing-email <address>`, apex)}`
		);
	if (!preview && todo('plan') && plan === 'subscribe')
		items.push(`Stop and ask me to subscribe at ${account}: it needs a card, so only I can do it.`);
	items.push(
		`Confirm: ${cli(`setup ${org} --json`, apex)} must report every step as done. Then tell me the registry URL.`
	);

	return `Set up PrivateCrates, ${personal ? 'a private Cargo registry' : 'our private Cargo registry'}, for ${personal ? 'my personal GitHub account' : 'the GitHub organisation'} ${org}.

First read https://${apex}/llms.txt: it describes the set-up flow and the cargo privatecrates CLI.${environmentNote(apex)}

Registry name: ${slug}, served at https://${slug}.${apex}${done.length ? `\nAlready done: ${done.join('; ')}.` : ''}

Steps:
${numbered(items)}

${agentRules(org, cli(`setup ${org} --json`, apex))}${todo('settings') ? '\n- Never accept the PrivateCrates terms on my behalf, and never pass --accept-terms until I have said I accept.' : ''}`;
}

/** The prompt that configures an organisation's crate repositories once the registry is live. */
export function publishPrompt({ org, slug, apex }: { org: string; slug: string; apex: string }): string {
	return `Configure the Rust crates in the GitHub organisation ${org} to use and publish to our PrivateCrates registry, ${slug} (https://${slug}.${apex}).

First read https://${apex}/llms.txt: it describes the cargo privatecrates CLI.${environmentNote(apex)}

Steps:
${numbered([
	`Install the CLI if it is missing: ${INSTALL_CLI}`,
	`List ${org}'s repositories that contain Rust crates (gh repo list ${org} --limit 500, then look for Cargo.toml). Show me the list and ask which crates to publish before changing anything.`,
	`In each chosen repository, on a new branch: run ${cli(`init --registry ${slug} --dry-run`, apex)} and show me the plan, then ${cli(`init --registry ${slug} --yes`, apex)}, which also sets publish = ["${slug}"] on each crate; set publish = false on any crate that should not be published, commit, and open a pull request with gh pr create. Do not merge it: I review and merge.`,
	`Once a pull request is merged, publish a first version from CI by pushing a tag that matches the crate's version, e.g. git tag v0.1.0 && git push origin v0.1.0; in a workspace, tag one crate with <crate>-v<version> (e.g. story_engine-v0.1.0), since v<version> publishes every crate. The publish workflow runs in GitHub Actions; follow it with gh run watch.`,
	`Run ${cli('doctor', apex)} in each repository, and fix or report anything it flags.`
])}

${agentRules(org, cli('doctor', apex))}
- Never publish from this machine and never add secrets to CI: publishing uses GitHub Actions' OIDC token.`;
}

/** The prompt a developer gives their coding agent to start working on a project that uses private crates. */
export function joinPrompt({ slug, apex }: { slug: string; apex: string }): string {
	return `Set up this machine to build this Rust project, which uses private crates from our PrivateCrates registry, ${slug} (https://${slug}.${apex}).

First read https://${apex}/llms.txt: it describes the registry and the cargo privatecrates CLI.${environmentNote(apex)}

Steps:
${numbered([
	`Install the credential provider if it is missing: ${installProvider}`,
	`Check that .cargo/config.toml in this repository (or ~/.cargo/config.toml) has [registries.${slug}] with the credential provider; if it does not, tell me rather than adding it.`,
	`Signing in needs me: ask me to run cargo login --registry ${slug} in my own terminal and approve the code on GitHub. Wait until I say it is done. Cargo run by you has no terminal, so it stops with "not signed in" instead of prompting.`,
	`Run cargo build. If a private crate is "not found", run ${cli(`doctor --crate <name>`, apex)} and tell me what it says: usually I need read access to the crate's GitHub repository, which someone in the organisation grants.`
])}

Rules:
- Never approve a GitHub sign-in or enter a device code yourself, and never paste tokens anywhere.
- Never publish crates from this machine: publishing happens in GitHub Actions.`;
}
