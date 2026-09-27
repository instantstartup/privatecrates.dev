# Setting up with an AI agent

Plan, 27 September 2026. An organisation admin should be able to tell a coding agent (Claude Code or similar) "set up
PrivateCrates for our org and publish our crates", and have it done, with the human needed only where GitHub insists
on one.

## What an agent can and cannot do

| Step | Who | How |
|---|---|---|
| Sign in to PrivateCrates | agent, human approves once | `cargo privatecrates login`: the reader App's device flow, the same sign-in and token store as the credential provider |
| Install the reader App | **human** | GitHub has no API to install an App: the agent shows the link (pre-filled) and waits |
| Create the storage repository | agent | `gh repo create {org}/crates-store --private`, with the admin's own GitHub login |
| Enable immutable releases | agent | `gh api -X PUT repos/{org}/crates-store/immutable-releases`, with the admin's rights, never our App's |
| Install the storage App on it | **human** | the pre-selected install link from the onboarding document |
| Accept the terms | **human** | the agent shows the admin the terms link and asks them to accept on behalf of the organisation; it never accepts for them |
| Choose the registry name | agent, once the admin accepted | `cargo privatecrates setup {org} --slug {slug} --accept-terms {version}` (records the acceptance, then our API creates `privatecrates.toml`) |
| Start the trial (orgs over 5 members; not during the preview) | agent, with the admin's billing email | `cargo privatecrates setup {org} --start-trial --billing-email {email}` |
| Configure each crate repository | agent | `cargo privatecrates init --registry {slug} --dry-run`, shown to the person, then `--yes` and a pull request |
| Publish a first version | agent | push a tag; `cargo privatecrates doctor` confirms the result |

Our Apps keep their least-privilege shape throughout: everything that needs administration rights is done with the
admin's own `gh` login.

## 1. `/llms.txt` and `/docs/agents`

- `https://privatecrates.dev/llms.txt` in the [llms.txt](https://llmstxt.org) format: what PrivateCrates is, the flow
  above as numbered steps with exact commands, where the human is needed, how to check progress, and links to the docs
  pages as Markdown.
- `/docs/agents`: the same for people, including the prompt below and what each step asks the admin to approve.

## 2. "Set up with your AI agent" on `/account`

A copy button producing a prompt filled in with the organisation, registry name and URL, and the pre-filled install
links from the onboarding document. Two variants:
- during onboarding: finish setting up the registry (the table above, from wherever the checklist is);
- once live: configure the organisation's crate repositories and publish them.

The prompt tells the agent to read `/llms.txt`, to use `gh` for repository administration, and to stop and hand the
admin a link where a human is needed.

## 3. `cargo privatecrates` (new crate `cargo-privatecrates`, open source)

A Cargo subcommand; agents do better with one deterministic command than a prose recipe, and people benefit too.
Every command takes `--json`, `--domain` (another deployment, e.g. `dev.privatecrates.dev`) or `--url` (a registry's
URL), and exits non-zero on failure.
- `login` / `logout`: the device flow, sharing the credential provider's token store (both use the small
  `privatecrates-auth` crate). The token is stored under the apex URL; the apex serves `GET /api/v1/auth` for it.
- `setup <org> [--slug S --accept-terms V] [--start-trial --billing-email E] [--json]`: prints the onboarding
  checklist with each step's status and link, and the terms (version, URL, whether accepted); `--slug` and
  `--start-trial` perform those steps through the account API (the trial needs a billing email, which the agent asks
  the admin for). `--slug` needs `--accept-terms` with the current version: without it, the command prints the
  terms' URL and the exact flag to add, and exits 1 (`account::terms_not_accepted`, with `terms` in the `--json`
  error). The CLI never fills the version in: the admin passes it, or tells their agent to after reading the terms.
  `--json` for agents, with `terms: { version, url, accepted }`.
- `terms <org> [--accept V] [--json]`: whether the organisation has accepted the current terms (exits 1 while not),
  and, with `--accept`, accepts them for a registry set up before them, or before their current version. The same
  rule: only once the admin has read and accepted them.
- `init --registry <slug> [--url URL]`, run in a crate repository or workspace: merges the registry into
  `.cargo/config.toml` (preserving formatting, with `toml_edit`); sets `package.repository` from the git remote where
  missing (in `[workspace.package]` with `repository.workspace = true` in members, for workspaces); writes
  `.github/workflows/publish.yml` (unless `--no-workflow`); idempotent. It prints the plan and asks before writing (the
  workflow separately); without a terminal it prints the plan and exits 1. `--dry-run` shows the plan, `--yes`
  applies it without asking: an agent shows the person the dry run first.
- `doctor [--registry S …] [--json]`: checks the provider is installed and configured, the registry answers (401
  without a token, 200 with the stored one), `package.repository` matches the git remote, `publish` is restricted to
  the registry, the workflow exists with `id-token: write`, and that the current version is in the index. Whether
  the release is immutable and has provenance is `privatecrates-verify`'s job: it needs the storage repository,
  which a developer's token cannot read.

## 4. Machine-readable status for agents

The account API (`/api/session`, `/api/orgs/{org}/…`) also accepts `Authorization: Bearer ghu_…`: a reader App user
token from the device flow, as `cargo privatecrates login` stores. Bearer requests are not ambient credentials, so the
cookie session's CSRF check does not apply to them; other GitHub token types stay refused here, to keep broad tokens
out. Responses are the same JSON as for the website.

## Later

An MCP server exposing the registry to agents (search crates, read their documentation, check publish status), in
line with SPEC §12's "API search for coding agents".
