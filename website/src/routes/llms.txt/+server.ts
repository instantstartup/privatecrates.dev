// /llms.txt (https://llmstxt.org): how a coding agent sets up PrivateCrates for an organisation. Prerendered to
// build/llms.txt, so the server serves it as a static file. Every command comes from $lib/snippets.
import {
	FREE_MEMBER_LIMIT,
	GA_TARGET,
	PREVIEW,
	PRICE_USD,
	SITE_URL,
	TERMS_PATH,
	TERMS_VERSION,
	TRIAL_MONTHS
} from '$lib/site';
import {
	cli,
	cliUsage,
	doctorCommands,
	installCli,
	PROD_APEX,
	STORAGE_REPO,
	storageRepoCommands
} from '$lib/snippets';

export const prerender = true;

const fence = (code: string, lang = 'sh') => '```' + lang + '\n' + code + '\n```';
const indent = (text: string) => text.replace(/^/gm, '   ');

const ORG = '<org>';
const SLUG = '<name>';
const TERMS_URL = `${SITE_URL}${TERMS_PATH}`;

const pricing = PREVIEW
	? `- **Private preview, by invitation only:** only organisations PrivateCrates has invited can have a registry. Anyone else asks for an invitation on ${SITE_URL}/account (signed in with GitHub; a person does this). It is free and billing is off: there is no trial, no card and nothing to pay, whatever the organisation's size. It is provided as is, at the user's own risk, under the private preview terms (${TERMS_URL}, version \`${TERMS_VERSION}\`). Planned pricing from general availability (${GA_TARGET}): free up to ${FREE_MEMBER_LIMIT} members, then $${PRICE_USD} per organisation per month.`
	: `- Free for organisations with up to ${FREE_MEMBER_LIMIT} members. Larger ones start a ${TRIAL_MONTHS}-month free trial with no card, then pay $${PRICE_USD} per organisation per month.`;

const trialFlag = PREVIEW
	? ''
	: '; `--start-trial --billing-email <address>` starts the no-card trial (Stripe sends the trial-ending reminder and invoices to that address; ask the admin for it, never guess it)';

const planStep = PREVIEW
	? `Plan: nothing to do. Billing is off during the private preview, so the \`plan\` step is \`done\` ("Free during the private preview.").`
	: `Plan. Organisations with up to ${FREE_MEMBER_LIMIT} members are free: nothing to do. Larger ones: ask the admin for a billing email address, then \`${cli(`setup ${ORG} --start-trial --billing-email <address>`)}\` (no card). If the checklist says the organisation has had its trial, **a person is needed:** the admin subscribes with a card at https://${PROD_APEX}/account.`;

const body = `# PrivateCrates

> PrivateCrates is a private Cargo registry for a GitHub organisation, at https://<name>.${PROD_APEX}. GitHub is the storage, the identity provider and the source of truth for permissions: whoever can read a crate's repository can use the crate, and whoever can push to it can publish it, from GitHub Actions. This file tells coding agents how to set a registry up with the \`cargo privatecrates\` CLI and the \`gh\` CLI, where a person has to step in, and how to configure crate repositories to publish.

- The index and every crate file live in a private storage repository the organisation owns (conventionally \`${STORAGE_REPO}\`); each version is an immutable GitHub release.
- Two GitHub Apps with narrow permissions: the **reader App** (repository metadata and organisation membership, read-only) and the **storage App** (contents write on the storage repository only).
- Developers use \`cargo-credential-privatecrates\`, a Cargo credential provider that signs in with GitHub. CI uses GitHub Actions' OIDC token: no secrets anywhere.
${pricing}
- Other environments: add \`--domain <domain>\` to every \`cargo privatecrates\` command. Production (${PROD_APEX}) needs no flag.

## The CLI

${fence(installCli)}

${fence(cliUsage)}

- \`login\` / \`logout\`: GitHub's device flow for the reader App. The token is stored in the operating system's keyring, shared with the credential provider.
- \`setup <org>\`: prints the organisation's set-up checklist, each step's status (\`done\`, \`todo\` or \`blocked\`) and, where a person must act, the link. \`--slug <name> --accept-terms <version>\` saves the registry name, once an admin has accepted the terms (without \`--accept-terms\` it prints the terms URL and the flag to add, and exits non-zero)${trialFlag}; \`--json\` prints the checklist as JSON.
- \`terms <org> --accept <version>\`: records an admin's acceptance of the current terms for a registry created before them.
- \`init --registry <name>\`: run in a crate repository or workspace. Adds the registry to \`.cargo/config.toml\`, sets \`package.repository\` from the git remote where missing, and writes \`.github/workflows/publish.yml\` (unless \`--no-workflow\`). It shows the plan and asks before writing; without a terminal it only prints the plan and exits non-zero. \`--dry-run\` prints the plan; \`--yes\` applies it without asking. Idempotent.
- \`doctor\`: checks the credential provider, the registry, \`package.repository\`, the publish workflow's \`id-token: write\` permission, that \`publish\` is restricted to the registry and, once a version is published, that it is in the registry's index. Immutable releases and provenance are checked by \`privatecrates-verify\`, run on the storage repository.

## Set up a registry

Run the steps in order, skipping those \`${cli(`setup ${ORG} --json`)}\` reports as \`done\`. Only an admin of the organisation can complete them, so work with an admin at the keyboard.

1. Install the CLI (above).
2. Sign in: \`${cli('login')}\`. It prints a code and a GitHub link. **A person is needed:** give the admin both and wait while they approve.
3. Read the checklist: \`${cli(`setup ${ORG} --json`)}\`. If the organisation is not listed, the reader App is not installed on it yet. If the steps are \`blocked\` because the organisation is not invited to the private preview, **stop: a person is needed.** Tell the admin to ask for an invitation at ${SITE_URL}/account, and go no further until it is granted.
4. Reader App. **A person is needed:** GitHub has no API to install an App. Give the admin the \`reader_app\` step's link (or https://${PROD_APEX}/account), ask them to install it on the organisation, on all repositories or on those that own crates, and wait.
5. Storage repository, with the admin's own \`gh\` login (check \`gh auth status\`; it needs rights to create repositories in the organisation):
${indent(fence(storageRepoCommands(ORG)))}
   Or use an existing, empty private repository. It must hold nothing but the registry.
6. Storage App. **A person is needed:** give the admin the \`storage_app\` step's link. It is pre-selected for the organisation; they choose "Only select repositories" and pick the storage repository alone. Wait.
7. Terms. **A person is needed:** an admin must accept the terms on behalf of the organisation before the registry is created. Show the admin the link, ${TERMS_URL}, ask them to read the terms and to say whether they accept them for the organisation, and wait. **Never accept on the admin's behalf,** never pass \`--accept-terms\` before they have said they accept, and stop if they decline.
8. Registry name, once both Apps are installed and the admin has accepted the terms: \`${cli(`setup ${ORG} --slug ${SLUG} --accept-terms ${TERMS_VERSION}`)}\`. The name becomes the hostname \`<name>.${PROD_APEX}\` and the registry name in Cargo; lowercase letters, digits and hyphens. The service saves it as \`privatecrates.toml\` in the storage repository. Before running it, ask the admin who may publish crates: GitHub Actions only (the default and the recommendation: every version is built from a commit, with provenance signed by GitHub), or also developers' own machines with \`cargo publish\` (versions without provenance). Add \`--allow-manual-publish\` only if they chose the latter; it becomes the default for every repository, which \`[repositories.<name>]\` in \`privatecrates.toml\` can override per repository.
9. ${planStep}
10. Confirm: \`${cli(`setup ${ORG} --json`)}\` reports every step as \`done\`. The registry is live at https://<name>.${PROD_APEX}.
11. Recommend the verifier, which checks the registry independently of PrivateCrates on every publish and daily (about a minute of Actions time a run). Ask the admin; if they agree, \`${cli(`add-verifier ${ORG} --yes`)}\` commits the workflow to the storage repository with their own \`gh\` login (it may need \`gh auth refresh -s workflow\`). Without \`--yes\` it only shows what it would commit.

## Configure crate repositories and publish

1. List the organisation's repositories with Rust crates (\`gh repo list ${ORG}\`, then look for \`Cargo.toml\`), and ask the admin which crates to publish.
2. In each chosen repository, on a new branch, run \`${cli(`init --registry ${SLUG} --dry-run`)}\` and show the admin the plan, then \`${cli(`init --registry ${SLUG} --yes`)}\`. Check that each crate to publish has \`publish = ["<name>"]\` in its \`Cargo.toml\`, so it can never go to crates.io by accident. Commit and open a pull request with \`gh pr create\`. The admin reviews and merges.
3. Publish a first version by pushing a tag that matches the crate's version, e.g. \`git tag v0.1.0 && git push origin v0.1.0\`. In a workspace, \`<crate>-v<version>\` (e.g. \`story_engine-v0.1.0\`) publishes that one crate and \`v<version>\` publishes every crate. The workflow publishes with GitHub Actions' OIDC token and records provenance. Publishing from a laptop is refused by default.
4. Check each repository:
${indent(fence(doctorCommands()))}

Developers who depend on the crates install the credential provider once (\`cargo install cargo-credential-privatecrates --locked\`); the first build signs them in with GitHub.

## Where a person is needed

- Asking for an invitation to the private preview, if the organisation is not invited.
- Approving the sign-in that \`cargo privatecrates login\` starts.
- Installing the reader App, and installing the storage App on the storage repository.
- Accepting the terms on behalf of the organisation: the admin reads them and accepts; the agent never does.
${PREVIEW ? '' : '- Subscribing with a card, only for an organisation that has already had its trial.\n'}- Merging the pull requests.

At each of these, stop. Give the admin the exact link, say what to choose there, and wait until they say it is done; do not poll GitHub in a loop. Then re-run \`${cli(`setup ${ORG} --json`)}\` to confirm the step is \`done\` before going on.

## Checking progress

\`${cli(`setup ${ORG} --json`)}\` prints the set-up checklist: for each step (\`reader_app\`, \`storage_repo\`, \`storage_app\`, \`settings\`, \`plan\`), its \`status\` (\`done\`, \`todo\` or \`blocked\`), a \`detail\` where there is one, and an \`action_url\` where a person must act. A \`blocked\` step waits for an earlier one, or for an admin. For crate repositories, \`${cli('doctor --json')}\` reports each check's result.

## Security

- Never ask for, create or store a personal access token (classic or fine-grained) or any other broad GitHub token. Nothing in this flow needs one.
- Repository administration (creating the storage repository, turning on immutable releases) is done with the admin's own \`gh\` login, so the PrivateCrates Apps never need administration rights.
- The token from \`cargo privatecrates login\` is a reader App user token: it can read repository metadata and nothing else. It expires after 8 hours and is refreshed from the keyring.
- CI needs no secrets: the publish and build workflows use \`permissions: id-token: write\`. Do not add registry tokens to repository secrets.
- Publish from GitHub Actions by pushing a tag, never from the agent's machine, so every version has provenance.
- Never accept the terms for the admin: accepting binds the organisation. Pass \`--accept-terms\` (or \`terms --accept\`) only with the version the admin has read and said they accept.

## Docs

- [Set up with an AI agent](${SITE_URL}/docs/agents): this flow for people, the prompts, and the CLI reference
- [Set up a registry](${SITE_URL}/docs/setup): the Apps, the storage repository, the settings file and developer set-up
- [Joining a team](${SITE_URL}/docs/joining): for a developer on a project that already uses a registry: install, sign in, editors, "not found", and the prompt for their coding agent
- [CI without secrets](${SITE_URL}/docs/ci): reading crates from GitHub Actions with OIDC
- [Publishing](${SITE_URL}/docs/publishing): trusted publishing, the owners file, first publishes and yanking
- [Security model](${SITE_URL}/docs/security): what PrivateCrates can see and store, and what a compromise could do

## Optional

- [Verify the registry](${SITE_URL}/docs/verify): the open-source verifier for the storage repository
- [Error reference](${SITE_URL}/docs/errors): every error code the registry returns
- [Private preview terms](${TERMS_URL}): the terms an admin accepts before a registry is created
- [Pricing](${SITE_URL}/pricing): ${PREVIEW ? 'free during the private preview; planned pricing from general availability' : 'free plan, trial and price'}
`;

export function GET(): Response {
	return new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
