# PrivateCrates: a hosted private Cargo registry that is a thin wrapper over GitHub

Status: specification, revised 27 September 2026 for a hosted, paid product. Nothing is built yet.

## 1. Purpose

GitHub Packages does not support Cargo, so private Rust crates are usually consumed as git dependencies. That works,
but it has no semver ranges, no `cargo publish`, no search and slow clone-based fetches.

PrivateCrates (privatecrates.dev) is a hosted service that implements Cargo's registry protocol, while **GitHub remains the store, the
identity provider and the source of truth for permissions**:
- crate files are stored as GitHub Release assets in a repository the customer owns;
- the registry index lives in that same repository;
- a caller may do what GitHub allows them to do on the repository that owns the crate.

Anyone who can read a crate's repository on GitHub can use the crate. Anyone who can push to it can publish it. No
separate accounts, keys or access lists.

### Positioning

- **Security is GitHub's.** Authentication, SAML SSO, SCIM provisioning and offboarding all apply with nothing to
  configure, because every permission decision is GitHub's. Competitors charge heavily for SSO; here it is included.
- **We hold nothing that matters.** Customer data lives in the customer's own organisation. The tokens we see are
  narrow (§6), and everything we write is an auditable, verifiable commit in the customer's repository (§10). The
  only thing we store ourselves is the record of who accepted our terms, for which organisation, and when
  (`docs/preview.md`); no registry data, code or tokens.
- **Don't trust us, verify us.** Every version is an immutable GitHub release carrying GitHub-signed proof of the
  workflow that built it. Even a fully compromised PrivateCrates cannot publish or alter a crate without the
  customer's own verifier noticing (§10.3).
- **Leave any time.** The index and crates are already in the customer's repository.
- **Rust only.** The target is Rust shops on GitHub, not buyers comparing breadth across package formats.

First customers (dogfooding): worldbuilding.dev and the movie production site, both depending on `story_engine` and
its packs from `worldbuilding-dev/story-engine`.

### Goals
- Cargo's sparse registry protocol with `auth-required`: index, download, publish, yank, unyank and search.
- Many GitHub organisations (tenants) on one deployment.
- Developers sign in with a GitHub device flow; tokens carry only metadata read access.
- CI on GitHub Actions needs no stored secrets: it authenticates with Actions OIDC.
- No long-lived credentials exist anywhere in the system.
- Every published version has verifiable provenance: which repository and workflow built exactly which bytes.
- Users' GitHub rate limits are barely touched: one permission lookup per user every few minutes, whatever they do.
- Stateless: all durable state lives on GitHub; the service can be redeployed or restarted at any time.
- One Rust binary, deployable on Railway.

### Non-goals, for now
- A crates.io mirror or proxy. Dependencies from crates.io stay on crates.io.
- Download statistics.
- GitHub Enterprise Server (a self-hosted licence may come later, §2.4).
- CI outside GitHub Actions, at launch (§3.3).
- Package formats other than Cargo.

## 2. Product

### 2.1 Tenancy

A tenant is one GitHub organisation that has installed both of our GitHub Apps (§6.1). Each tenant is served on its
own hostname, `{slug}.privatecrates.dev`. The slug is chosen at sign-up and recorded in the
tenant's storage repository (§5), not keyed to the organisation login, so an organisation rename breaks nothing.

The set of tenants is derived at start-up and kept current by webhooks (§7). There is no tenant database:
- `GET /app/installations` for each App gives the organisations that have installed it; a tenant needs both;
- the storage App is installed on exactly one repository in the organisation, which is therefore the storage
  repository (an installation on more than one repository is a configuration error, reported and not served);
- `privatecrates.toml` in that repository gives the slug.

### 2.2 Pricing and billing

- **Always free for personal GitHub accounts** (§6.8), whatever the member limit or Stripe says, now and after
  general availability.
- **Free for organisations with 5 or fewer members**, with every feature and no card. Growth-led: developers adopt it
  on small teams and bring it to the companies that pay.
- **Larger organisations: $100 per month per GitHub organisation, no user limit**, after a **3-month free trial that
  needs no card**. An organisation that grows past 5 members has its trial started automatically, so growth never
  breaks anything. SSO and every other feature are included at every level: no SSO tax.
- Billed through **Stripe**. Stripe is the source of truth: each subscription's metadata names the GitHub
  organisation, and the server loads subscriptions at start-up and keeps them current from Stripe webhooks, so
  billing needs no database of ours. Member counts come from GitHub.
- A tenant is active while free, or while its subscription is trialing, active or past due. Once it ends, publishing
  is refused at once and reads continue for 14 days, then stop, both with an error pointing to the account page.
- The full model, and the website's account API, are in `docs/website-api.md`.
- A higher tier later, priced on things that cost us more: hosted documentation and API search for coding agents
  (§12), several organisations under one enterprise, a support SLA.

### 2.3 Environments and the website

| Environment | Website and account API | Registries | GitHub Apps | Stripe |
|---|---|---|---|---|
| dev | `dev.privatecrates.dev` | `{slug}.dev.privatecrates.dev` | separate dev Apps | test mode |
| prod | `privatecrates.dev` | `{slug}.privatecrates.dev` | production Apps | live mode |

One server per environment on Railway serves both: a request for the apex host gets the website (a static SvelteKit
build) and the account API; a request for `{slug}.{apex}` gets that tenant's registry. Slugs that could be confused
with our own hosts (`www`, `dev`, `api`, `docs` and similar) are reserved. Signing in to the website uses the reader
App's web flow; the session is an encrypted cookie holding the user's token, so nothing is stored server-side.

Onboarding (`/account`) walks an organisation administrator through installing the Apps, creating the storage
repository with immutable releases, choosing a slug (the storage App then *creates* `privatecrates.toml`; it may
never change it afterwards) and starting the subscription.

### 2.4 What is open source

- **Open source (MIT or Apache-2.0):** the credential provider (§3.1) and the storage verifier (§10.3). Customers
  can inspect everything that touches their machines and CI, and can verify everything we write without trusting
  us.
- **Source-available:** the hosted service, under the Business Source License 1.1. Anyone may read, audit and
  self-host it for their own organisation (the Additional Use Grant); offering it as a hosted registry service to
  third parties is not permitted. Each version converts to Apache-2.0 four years after its release, the longest
  delay BSL 1.1 allows. Security reviewers can read the code that handles their tokens; competitors cannot host it.

## 3. How developers and CI use it

Registry name used below: `acme`. Hostname: `acme.privatecrates.dev`.

### 3.1 The credential provider: one tool everywhere

Developers and CI use the same `.cargo/config.toml` and plain Cargo commands: `cargo build`, `cargo publish`,
`cargo yank`. No custom actions or wrapper commands.

```toml
[registries.acme]
index = "sparse+https://acme.privatecrates.dev/index/"
credential-provider = ["cargo-credential-privatecrates"]
```

`Cargo.toml`:

```toml
story_engine = { version = "0.2", registry = "acme" }
```

`cargo-credential-privatecrates` implements Cargo's credential provider protocol (`get`, `login` and `logout`). Cargo
tells it which operation it needs a token for, and for a publish it passes the crate's name, version and checksum.
The provider picks the credential from its environment:

| Where | Operation | Credential |
|---|---|---|
| GitHub Actions (with `id-token: write`) | read | Actions OIDC token exchanged for a one-hour read-only registry token (§6.4); cached until it expires |
| GitHub Actions (with `id-token: write`) | publish | Actions OIDC token bound to that crate, version and checksum (§3.4); never cached |
| A developer's machine | any | reader App user token from the device flow (below) |

On a developer's machine, the first time a token is needed the provider runs GitHub's device flow for the **reader
App** (§6.1): it prints a code, the developer approves it in the browser, and the provider stores the refresh token
in the operating system's keyring. The resulting user access token (`ghu_…`) can do only what the reader App is
allowed to do, which is read repository metadata. It expires after 8 hours and is refreshed silently.
`cargo login --registry acme` runs the device flow on demand; `cargo logout --registry acme` deletes the stored
tokens.

Installing it: `cargo binstall cargo-credential-privatecrates` on a developer's machine, or `cargo install
cargo-credential-privatecrates --locked` without cargo-binstall. Prebuilt, checksummed binaries are published with
every release (`SHA256SUMS`), built by the release workflow with GitHub artifact attestations. In CI, the workflows
we write download the binary for the runner, pinned to a version, and check it against `SHA256SUMS` and with
`gh attestation verify` before installing it (`privatecrates_common::install::ci_step`): seconds rather than a
compile, and no third-party action.

Other GitHub tokens (`gh auth token`, classic and fine-grained personal access tokens) are still accepted through
`cargo:token-from-stdout` for compatibility, but documentation steers towards the credential provider, because those
tokens are usually far broader than we need.

### 3.2 CI on GitHub Actions

No App to create, no secret to store, and no action of ours: the install step is plain shell (§3.1).
`cargo install cargo-credential-privatecrates --locked` works too, but compiles for minutes unless cached:

```yaml
permissions:
  id-token: write
  contents: read
steps:
  - uses: actions/checkout@v5
  - name: Install cargo-credential-privatecrates (checksummed and attested)
    env: { GH_TOKEN: "${{ github.token }}", VERSION: "0.2.5" }
    run: |   # the release's archive for this runner, checked against SHA256SUMS and its build attestation
      …
  - run: cargo build --locked
```

The provider detects Actions from `ACTIONS_ID_TOKEN_REQUEST_URL`, requests an OIDC token with audience
`https://acme.privatecrates.dev`, and exchanges it at `POST /api/v1/oidc/exchange` for a read-only registry token.

If `id-token: write` is missing, the provider fails with a message saying exactly which line to add, rather than
falling back to the device flow, which would hang in CI.

Workflows triggered from forks do not receive OIDC tokens, so they cannot read private crates. That is the intended
behaviour.

### 3.3 Docker builds and CI outside GitHub

At launch, only GitHub Actions is supported for CI, so that no long-lived credential exists anywhere. Other
environments get their dependencies from a GitHub Actions job:
- **Build the image in GitHub Actions** and push it to a container registry the platform deploys from. The registry
  token is available to a BuildKit secret mount (`RUN --mount=type=secret`), never to an image layer.
- **Or vendor:** run `cargo vendor` in GitHub Actions and hand the vendored tree to the external build, which then
  needs no registry access.

Railway's Dockerfile builds expose variables only as `ARG`s, and build arguments can be recorded in image history,
so a registry token must never be passed to a Railway build. Railway services use one of the two routes above.

Support for other CI (GitLab, Buildkite and CircleCI all issue OIDC tokens) can be added later by mapping their
claims to a GitHub repository; it is not a launch requirement.

### 3.4 A publisher

Publishing is plain `cargo publish --registry acme`. What differs is where it runs.

**From GitHub Actions (the default, with provenance):**

```yaml
on:
  push:
    tags: ["v*"]
permissions:
  id-token: write
  contents: read
jobs:
  publish:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v5
      - …   # install the credential provider (§3.2)
      - run: cargo publish --registry acme
```

Cargo packages the crate and asks the provider for a publish token, passing the name, version and checksum of the
exact `.crate` it is about to upload. The provider requests an Actions OIDC token whose audience binds it to those:
`https://acme.privatecrates.dev/publish/{name}/{version}/{sha256}`. It answers Cargo with that token, marked as not
cacheable and valid for this operation only.

The service accepts the publish only if the audience matches the uploaded bytes, the token's `repository_id` is the
crate's owning repository, its workflow (and environment, if configured) is allowed in `owners/{name}.toml` (§5),
the run was triggered by `push`, `release` or `workflow_dispatch`, and the person who started it can create releases
in the owning repository (§6.4). The token is stored with the release as its provenance (§10.3). Because it names one immutable version and
checksum, it cannot be replayed for anything else. Publishing a whole workspace works the same way: Cargo asks for
one token per crate.

**From a developer's machine:** the same command, but by default it is refused with an error that explains why and
includes the workflow above, ready to paste:

```
error: story_engine is published from CI only, so every version has verifiable provenance.
       Add .github/workflows/publish.yml (see https://acme.privatecrates.dev/login#publish) and push a tag.
```

Publishing from developers' machines is the organisation's choice, made when the registry is created:
`allow_manual_publish` in `privatecrates.toml` is the default for every repository, a `[repositories.<name>]` table
overrides it for one repository (both ways), and a crate's owners file can allow it for that crate alone (§5). Where
it is allowed, anyone with Write access to the owning repository can publish from their machine, first versions
included, if the package was built from a clean git checkout: Cargo's `.cargo_vcs_info.json` must name a commit and
not be dirty, and the commit is recorded with the publisher. The commit is the publisher's claim, not provenance: such
versions have none, and the verifier, search and the compliance dashboard label them as manual publishes.

**A brand-new crate** is first published from CI too (§6.2), so its first version always has provenance. The error
above tells a developer who tries it from their machine what to do.

**Who may publish.** One rule, from CI and from a machine alike: only people who can create releases in a crate's
owning repository, meaning GitHub's Write, Maintain or Admin role (a custom role counts as the role it is based on),
may publish the crate. Triage and Read are not enough. A workflow run must also have been triggered by an event only
such people can cause: `push`, `release` or `workflow_dispatch`. Bots, which cannot hold a role, may publish only
when listed in `publish_bots` in the crate's owners file (§5, §6.4).

Yanking and unyanking (`cargo yank`) are allowed to anyone with push access, from a machine or CI, without
provenance: yanking a bad version must be quick, it does not change any bytes, and it can be undone.

## 4. Cargo protocol surface

All paths are relative to the tenant's base URL (e.g. `https://acme.privatecrates.dev`). Every endpoint except
the unauthenticated `config.json` probe, `/login` and `/healthz` requires a token.

| Method and path | Purpose | Permission |
|---|---|---|
| `GET /index/config.json` | Registry configuration | Unauthenticated: 401. Authenticated: caller can read at least one repository in the tenant. |
| `GET /index/{prefix}/{name}` | Sparse index file for one crate | read on the crate's owning repository |
| `GET /api/v1/crates/{name}/{version}/download` | Download a `.crate` | read on the owning repository |
| `PUT /api/v1/crates/new` | Publish | a bound OIDC token from the owning repository's allowed workflow; or, if the crate allows manual publishing, push on the owning repository |
| `DELETE /api/v1/crates/{name}/{version}/yank` | Yank | push on the owning repository |
| `PUT /api/v1/crates/{name}/{version}/unyank` | Unyank | push on the owning repository |
| `GET /api/v1/crates?q=&per_page=` | Search (§9) | returns private crates the caller can read, then crates.io results |
| `POST /api/v1/oidc/exchange` | Actions OIDC token to read-only registry token (§6.4) | a valid OIDC token from a repository in the tenant |
| `POST /webhooks/github` | GitHub webhooks (§7) | valid `X-Hub-Signature-256` |

### 4.1 `config.json`

```json
{
  "dl": "https://acme.privatecrates.dev/api/v1/crates/{crate}/{version}/download",
  "api": "https://acme.privatecrates.dev",
  "auth-required": true
}
```

Cargo fetches `config.json` first. If that returns HTTP 401, Cargo retries with the token (RFC 3139). So an
unauthenticated request returns `401` with:

```
WWW-Authenticate: Cargo login_url="https://acme.privatecrates.dev/login"
```

`/login` is a static page explaining the credential provider, the `.cargo/config.toml` block and the CI action.

Because the check is "can read at least one repository in the tenant", outside collaborators with access to a single
repository can use that repository's crates and nothing else.

### 4.2 Sparse index files

The path layout follows the Cargo Book (names lowercased):

| Name length | Path |
|---|---|
| 1 character | `1/{name}` |
| 2 characters | `2/{name}` |
| 3 characters | `3/{first char}/{name}` |
| 4 or more | `{chars 1–2}/{chars 3–4}/{name}` |

Each file has one JSON line per published version, in publish order, in the index format from the Cargo Book:
- `name`, `vers`, `cksum` (sha256 of the `.crate`)
- `deps`, each with `name`, `req`, `features`, `optional`, `default_features`, `target`, `kind`, and optionally
  `registry` and `package`
- `features`, `features2` and `v` (2 when `features2` is present)
- `yanked`, `links`, `rust_version`

Dependencies from crates.io keep `"registry": "https://github.com/rust-lang/crates.io-index"`. Dependencies on the
tenant's own crates have no `registry` field.

Responses carry an `ETag` (the git blob sha of the file) and honour `If-None-Match` with `304`.

An unknown crate returns 404. So does a crate the caller cannot read, so that its existence is not revealed. The one
exception is SSO (§6.5).

### 4.3 Download

The service looks up the release asset for `{name}` and `{version}` (§5) and answers `302 Found` → the asset's
short-lived signed download URL, obtained with the tenant's **storage App** installation token:
- `GET /repos/{storage}/releases/assets/{id}` with `Accept: application/octet-stream`, without following the
  redirect;
- the `Location` header is returned to Cargo.

The signed URL is cached per asset for 60 seconds, well within its validity, so a burst of CI jobs fetching the same
crate costs one GitHub call, not one per job (§8). The asset ID behind a version is cached forever: releases are
immutable.

The bytes then come straight from GitHub's CDN. Cargo verifies them against `cksum` from the index. If the signed-URL
approach ever fails (for example, a proxy strips redirects), fall back to streaming the body.

### 4.4 Publish

Request body, from Cargo's web API: a 32-bit little-endian length, the JSON metadata, a 32-bit little-endian length,
then the `.crate` bytes.

Validation, rejected with Cargo's error format `{"errors":[{"detail":"…"}]}` and a plain-words message:
1. **Token:** either
   - a bound OIDC token (§6.4) whose audience names this crate, version and the sha256 of the uploaded bytes, from the
     owning repository and an allowed workflow; or
   - where manual publishing is allowed for the crate (§3.4), a token whose user has push permission on the owning
     repository, and a `.crate` packaged from a clean git checkout (`publish::not_clean` otherwise).
   For a crate's first publish, see §6.2.
2. **Name:** a valid crate name, not reserved. If the name also exists on crates.io, the publish is refused, unless
   `name_clash = "warn"` in `privatecrates.toml`; see §9.3.
3. **Version:** valid semver, and not already in the index. Versions are immutable; a yanked version cannot be
   re-published.
4. **Size:** the `.crate` is at most `MAX_CRATE_BYTES` (default 20 MiB).
5. **Metadata:** the `.crate` is a gzip tar whose `Cargo.toml` name and version match the metadata.
6. **Dependencies:** each one is either in this registry (and readable by the publisher) or from crates.io. Any
   other registry is refused.

Steps, in an order chosen so that a crash never leaves the index pointing at a missing or mutable file:
1. Compute the sha256. For a bound OIDC token, it must equal the checksum in the token's audience.
2. Create a **draft** release tagged `{name}-{version}` and upload two assets to it: `{name}-{version}.crate` and,
   for a trusted publish, `{name}-{version}.provenance.jwt` (the OIDC token, §10.3). Any stale draft left by an
   earlier crash is deleted first.
3. Check that the `digest` GitHub reports for the `.crate` asset equals the computed sha256.
4. Publish the release. The storage repository has immutable releases enabled (§5), so from this point the tag and
   both assets can never be changed. Check that the published release reports `immutable: true`. If it does not, delete
   the release and refuse the publish with an error telling the organisation to enable immutable releases.
5. Append the index line with the GitHub Contents API, using the file's current blob sha for optimistic concurrency.
   On a sha conflict (a concurrent publish), re-read and retry up to 5 times.
6. Update the caches and the search index, then respond
   `200 {"warnings":{"invalid_categories":[],"invalid_badges":[],"other":[]}}`, with any warning in `other`.

If a crash happens between steps 4 and 5, a retried publish finds a published release with no index line. If its
`.crate` digest matches, it is reused and step 5 runs. If not, the version is taken for good (the release is
immutable), and the publish is refused with a message to publish a new version.

### 4.5 Yank and unyank

Rewrite the crate's index file with `yanked` flipped for that version (optimistic concurrency as in §4.4), then
respond `{"ok": true}`. Assets are never deleted.

## 5. GitHub storage layout

One private **storage repository** per tenant, created by the customer (conventionally `{org}/crates-store`), with
**immutable releases enabled**. Only the storage App is installed on it. Neither App has repository administration
permission, so the service cannot turn immutability off.

```
crates-store/
  privatecrates.toml              # tenant settings
  index/                      # sparse index files, same layout as §4.2
    st/or/story_engine
  owners/                     # which repository governs each crate (§6)
    story_engine.toml
  README.md
```

`privatecrates.toml`:

```toml
slug = "acme"                 # hostname and suggested registry name
name_clash = "refuse"         # "refuse" (default) or "warn" when a crate name also exists on crates.io
ci_read = "organisation"      # what an Actions OIDC token may read (§6.4): "organisation" or "same-access"
allow_manual_publish = false  # the default: may crates also be published from developers' machines (§3.4)

[repositories.tools]          # one repository's own settings, by name (current, or as at its first publish)
allow_manual_publish = true
```

`owners/story_engine.toml`:

```toml
repository_id = 123456789                        # authoritative; survives renames and transfers
repository = "worldbuilding-dev/story-engine"    # for humans, updated on rename (§7)
publish_workflows = ["release.yml"]              # trusted publishing: allowed workflow files in that repository
publish_environment = "crates"                   # optional: require this GitHub environment (e.g. with reviewers)
allow_manual_publish = false                     # default; true allows `cargo publish` from machines, without provenance
publish_bots = ["release-please[bot]"]           # optional: bots whose workflow runs may publish (§6.4)
```

`publish_environment` lets a customer require GitHub's environment protection rules, such as a required reviewer,
before a version can be published: a two-person rule enforced by GitHub, not by us.

- **Crate files:** one immutable GitHub Release per crate version, tagged `{name}-{version}` (e.g.
  `story_engine-0.2.0`), with the asset `{name}-{version}.crate` and, for trusted publishes,
  `{name}-{version}.provenance.jwt`.
- **Retention:** every version is kept forever. Cargo needs yanked versions to stay downloadable for existing
  lockfiles, and immutable releases make the history tamper-proof. Nothing is ever pruned.
- **Why a separate storage repository:** writes happen in one place, the storage App needs write access to only one
  repository, and the crates' own repositories keep clean release pages. Permissions still come from each crate's
  own repository (§6).
- **Settings are in the repository**, not in a database: customers can read and change them with a pull request, and
  the history is their audit log.

## 6. Authorisation model: GitHub permissions are the registry permissions

### 6.1 Two GitHub Apps

GitHub App permissions apply to every repository in an installation. One App with contents write, installed on the
crates' repositories, could write to all of them. So there are two:

| App | Installed on | Permissions | Used for |
|---|---|---|---|
| **reader** | all repositories (or those owning crates) | metadata read; organisation members read | user sign-in (device flow), permission lookups, most webhooks |
| **storage** | the storage repository only | contents read and write | index and release reads and writes, signed download URLs, push webhooks |

A developer's `ghu_` token comes from the reader App, so if it is stolen it can list repository metadata and nothing
more.

### 6.2 Owning repository

Each crate has one, recorded in `owners/{name}.toml` at its first publish. It is stored by repository ID, so renames
and transfers inside the organisation do not break it.

**First publish.** There is no owners file yet, so:
- `package.repository` in the crate's metadata must point at a repository in the tenant's organisation;
- the bound OIDC token's `repository_id` must be that repository;
- the service creates `owners/{name}.toml` with that repository and the token's workflow file as the only entry in
  `publish_workflows`, in the same publish.

Where manual publishing is allowed for the repository `package.repository` names (§3.4), a first publish can instead
come from a developer's machine: the user must be able to read and push to that repository, and the owners file is
created with an empty `publish_workflows`, meaning any workflow in the owning repository may publish later versions
(narrow it by listing workflows). Otherwise a crate's first version always has provenance.

Changing a crate's owning repository or its trusted-publishing workflows is an administrator's edit to
`owners/{name}.toml`. It is deliberately not an API.

If the owning repository is deleted, the crate stays readable only by organisation owners until an administrator
assigns a new owner.

### 6.3 Developers: one repository-set lookup per user

For a reader App user token, the service makes one paginated call,
`GET /user/installations/{installation_id}/repositories` (100 per page), which lists every repository in the
installation the user can access, with their `permissions`. The resulting set of repository IDs, and the subset with
push permission, is cached by sha256(token) (§8).

Every read check (index, download, search, `config.json`) is answered from that set. A developer costs one to three
GitHub calls per cache period, however many crates they build or search.

For other token types (personal access tokens, `gh` OAuth tokens), which cannot list installation repositories, the
service falls back to `GET /repos/{owner}/{repo}` per owning repository, cached per (token hash, repository).

### 6.4 CI: Actions OIDC

Every Actions OIDC token is checked for:
- its signature, against GitHub's published signing keys (cached);
- its expiry;
- `repository_owner_id`, which must be the tenant's organisation;
- its audience, which depends on the use (below).

**Reading.** `POST /api/v1/oidc/exchange` accepts a token whose audience is the tenant's base URL. It returns a
read-only registry token (`pcr_…`): a JWT signed by the service (key in a KMS, §10.2), valid for one hour, carrying
the tenant and `repository_id`. The service verifies it without storing it.
- With `ci_read = "organisation"` (the default), it may read every crate in the tenant.
- Planned, not yet built: `"same-access"`, reading only crates whose owning repository the calling repository could
  read, checked with the reader App's installation token.

A leaked `pcr_` token expires within the hour. To revoke sooner, rotate the signing key; that invalidates every
outstanding token, which costs CI jobs a fresh exchange at most. The distinctive prefix lets us apply to GitHub's
secret scanning partner programme, so leaks in public code are reported to us.

**Publishing.** There is no exchange: the bound OIDC token, requested by the credential provider (§3.4), is sent
as the publish credential itself. Its
audience, `{base_url}/publish/{name}/{version}/{sha256}`, must match the request exactly. Then:
- `repository_id` must equal the owners file's (or satisfy the first-publish rule, §6.2);
- the workflow file from `job_workflow_ref` must be listed in `publish_workflows`;
- if `publish_environment` is set, the token's `environment` claim must equal it;
- `event_name` must be `push`, `release` or `workflow_dispatch`, which only someone with Write access to the
  repository can cause. Anything else is refused with `publish::trigger_not_allowed`: `pull_request_target`,
  `issue_comment`, `pull_request`, `workflow_run`, `schedule`, `merge_group`, `repository_dispatch` and the rest can
  run a workflow for people without Write access, or with no person behind it;
- the run's `actor` must currently be able to create releases in the owning repository. The service asks
  `GET /repos/{owner}/{repo}/collaborators/{actor}/permission` with the reader App's installation token (metadata
  read is enough) and accepts `permission` `write` or `admin` only: GitHub reports Maintain as `write` and Triage as
  `read`. The response's `user.id` must equal the token's `actor_id`, so a login renamed since the run started
  cannot stand for someone else. Not a collaborator (404) is a refusal. The answer is cached per (repository ID,
  actor ID) for at most 60 seconds, and a refusal is `publish::actor_cannot_release`, naming the actor and the
  repository;
- a bot (a login ending in `[bot]`, such as `release-please[bot]`) cannot hold a repository role, so it may publish
  only when listed in the owners file's `publish_bots`; the trigger rule still applies. A crate's first publish
  has no owners file yet, so it must be started by a person.

The commit and release notes of a trusted publish name the actor, workflow, run, trigger and ref ("Published by
alice via workflow … (run 42, attempt 1), triggered by push on refs/tags/v1"), for the audit trail. The verifier
checks the trigger in the stored provenance (§10.3), but cannot re-check the actor: GitHub answers for current
permissions only, not for when a version was published.

Manual publishing (`allow_manual_publish`) applies the same rule to the developer's own token: GitHub's `push`
permission, which is true for Write, Maintain and Admin and false for Triage and Read.

This is trusted publishing: no publish secrets anywhere, and nothing that can publish any bytes other than the ones
the workflow built.

Permission checks for CI use our installation tokens, so CI never consumes a developer's rate limit.

### 6.5 SSO

When an organisation enforces SAML SSO, GitHub refuses calls with tokens that have not been authorised for it,
answering `403` with an `X-GitHub-SSO` header. The service turns that into a Cargo error saying the token needs SSO
authorisation, with the URL from the header, instead of the usual 404. This reveals nothing about any crate: the lack
of authorisation applies to the whole organisation.

How SSO applies differs by token type: classic personal access tokens and OAuth tokens must be explicitly authorised;
GitHub App user tokens depend on the user having an active SAML session. Both paths are tested against a
SAML-enabled test organisation before launch.

### 6.6 Tokens

Tokens are read from the raw `Authorization` header (Cargo sends the token verbatim; a `Bearer ` or `token ` prefix
is tolerated). Accepted:
- reader App user tokens (`ghu_…`), preferred for developers;
- read-only registry tokens from the OIDC exchange (`pcr_…`);
- bound Actions OIDC tokens, for publishing only;
- OAuth tokens (`gho_…`) and classic and fine-grained personal access tokens, for compatibility.

Caller tokens are never forwarded anywhere except GitHub's API, and only to make permission lookups on the caller's
behalf. Every storage read and write uses the storage App's installation token.

### 6.7 Registry names reveal nothing

Registry names are guessable (usually the organisation's login), so the registry must not confirm which
organisations use PrivateCrates, or which organisation a registry belongs to, to anyone who cannot use it:

- A well-formed name with no registry is served as an empty registry that nobody can use. Every request gets the
  answer a registry gives an outsider: 401 with Cargo's login challenge without a token, 403 `auth::no_access` for
  the registry's configuration, 404 for crates, and 403 `auth::workflow_outside_organisation` for an Actions token.
- The `/login` page is the same for every name and does not name the organisation; neither do the errors an outsider
  can receive.
- The subscription is checked only once the caller is known to be able to use the registry (§2.2); anyone else gets
  `auth::no_access`. A manual publish checks that the caller can use the registry before anything about its settings.
- Certificates are a single wildcard, so certificate transparency logs list no registry names.

Response times still differ slightly (a real registry makes more GitHub lookups for some callers); this reveals less
than the above did, and is accepted.

### 6.8 Personal accounts

A personal GitHub account can have a registry, as an organisation does. GitHub reports each installation's account
type; the tenant records it (`personal`).

- **Its only admin is its owner.** The account API treats the signed-in user's own login as an organisation they
  administer (GitHub has no membership for a personal account), and lists it first once the reader App is installed
  on it or it has a registry. Installations are looked up at `/users/{login}/installation` instead of
  `/orgs/{org}/installation`.
- **Reads** follow the reader App's installation as for an organisation: the owner and each repository's
  collaborators can read the crates of repositories they can read. The owner stands in for organisation owners where
  those may read a crate whose repository was deleted (§6.2).
- **Publishing** from Actions works unchanged: a personal repository's `repository_owner_id` is the account's ID.
- **Always free:** no member count is taken; the plan is `free`; the trial, Checkout, the billing portal and the
  billing email are refused with `409 billing::personal_account`.

## 7. Webhooks

Webhooks keep caches, tenants and search current without polling. They are an optimisation: GitHub does not
guarantee delivery, so the TTLs in §8 remain the upper bound on staleness. Every delivery is verified with
`X-Hub-Signature-256` and de-duplicated by `X-GitHub-Delivery`.

| Event | App | Effect |
|---|---|---|
| `installation` (created, deleted, suspend, unsuspend) | both | add, remove or suspend a tenant |
| `installation_repositories` (added, removed) | reader | drop the tenant's cached repository sets |
| `organization` (member_removed, member_added) | reader | drop that user's cached repository sets |
| `membership` (team membership added, removed) | reader | drop that user's cached repository sets |
| `team` (added_to_repository, removed_from_repository, edited) | reader | drop the tenant's cached repository sets |
| `member` (collaborator added, removed, edited) | reader | drop that user's cached repository sets |
| `repository` (renamed, transferred, deleted, visibility changed) | reader | handle deleted owners (§6.2); drop the tenant's caches. Owners files are not rewritten: `repository_id` is authoritative, and the storage App may never change an existing owners file |
| `github_app_authorization` (revoked) | reader | drop that user's cached repository sets |
| `push` to the storage repository | storage | reload `privatecrates.toml`, `owners/` and changed index files; update the search index |

To drop a user's cached sets, the permission cache keeps a secondary index from GitHub user ID to token hashes.

With these, removing someone from the organisation or a team cuts off their access within seconds, not at the end
of the cache TTL.

Stripe webhooks (`checkout.session.completed`, `customer.subscription.created|updated|deleted`) arrive at
`POST /webhooks/stripe`, verified with `Stripe-Signature`, and update the tenant's billing state.

Later: publish automatically when a version tag is pushed on an owning repository (trusted publishing covers most of
this need already).

## 8. Caching, rate limits and performance

- **Repository-set cache (§6.3):** keyed by sha256(token), holding repository IDs with read and push flags and the
  GitHub user ID. TTL `PERMISSION_TTL_SECS` (default 300), 30 seconds for failures. Cleared early by webhooks (§7).
  Tokens are never stored; only their hashes are used as keys.
- **Per-repository cache (fallback token types):** keyed by (sha256(token), repository ID), same TTLs.
- **Index cache:** index files by path with their GitHub blob sha as the ETag. Updated directly on publish, yank and
  unyank, and on storage-repository `push` webhooks, so steady-state reads cost no GitHub calls.
- **Owners and settings:** loaded at start and updated by `push` webhooks, with a conditional refresh every 10
  minutes as a backstop.
- **Signed download URLs:** cached per asset for 60 seconds (§4.3).
- **Budgets:**
  - a developer: one to three calls on their own rate limit per cache period;
  - CI with OIDC: nothing on any user's rate limit; a few calls on our installation tokens per crate per URL
    minute;
  - an installation starts at 5,000 calls an hour and rises with the organisation's size.
- **Later, if a large tenant needs it:** cache `.crate` bytes in object storage with free egress (for example
  Cloudflare R2). Crates are immutable, so this is safe; GitHub stays the source of truth.
- **Targets:** cached requests under 20 ms; cold ones under 500 ms; publishing a 5 MB crate under 5 s.

## 9. Search

### 9.1 Private crates

Each tenant has an in-memory search index of its crates: name, description, keywords, categories and latest
non-yanked version, from the index files and the published metadata. It is built at start-up from the storage
repository with the storage App's token and updated on publish, yank and `push` webhooks. No user token is ever used
to build it. A tenant has a few hundred crates at most, so this is small; use `tantivy` only if simple ranking proves
insufficient.

Queries are filtered by the caller's cached repository set (§6.3), so search costs the caller no GitHub calls when
the set is cached.

### 9.2 crates.io results

`GET /api/v1/crates?q=` returns the caller's matching private crates first, then crates.io results for the same query.
- Private results are labelled in the description, e.g. `[acme] …`, because `cargo search` shows only names and
  descriptions. A developer should not add a crates.io crate with `registry = "acme"`.
- crates.io responses are cached briefly per query, and requests follow crates.io's crawler policy (identifying
  `User-Agent`, rate limit). Check the current policy when implementing.
- If crates.io is slow or down, private results are returned alone.

### 9.3 Name clashes

A private crate whose name also exists on crates.io is a dependency-confusion risk: a developer who forgets
`registry = "acme"` gets the public crate. Publishing a clashing name is refused by default (§4.4).

Someone can also register a private crate's name on crates.io *after* it was published privately. So:
- the service checks the tenant's crate names against crates.io daily and flags clashes in search results;
- the verifier reports clashes too (§10.3), so the customer hears about them in their own CI;
- the documentation recommends reserving important names on crates.io with a placeholder crate.

## 10. Security

### 10.1 Trust boundary

What the service sees: caller tokens and crate bytes in transit during publish. What it stores durably: nothing;
caches only, keyed by token hashes. What it can do: write to each tenant's storage repository, through the storage
App.

Threats this design has to survive, and how:

| Threat | Outcome |
|---|---|
| A developer's `ghu_` token is stolen | The thief can list repository metadata. Nothing else. |
| A `pcr_` token is stolen | The thief can read the tenant's crates for under an hour. |
| The storage App key is stolen, or the service is compromised | Published bytes cannot be changed (immutable releases). A malicious new version has no valid provenance, and a changed owners file or rewritten index line is not an App-authored append; the verifier reports all three. The exception is a crate that allows manual publishing: there, a forged version looks like a manual publish, which is why manual publishing is off by default and the verifier lists every manual publish for review. Deleting releases breaks downloads (availability) and is reported. |
| GitHub itself is compromised | Out of scope: GitHub is the root of trust by design. |

### 10.2 Controls

- **Tokens:** never logged, never stored, never forwarded to anything other than GitHub's API.
- **Narrow tokens by default:** developers hold reader App tokens (metadata read); CI holds one-hour registry tokens
  with no GitHub access at all.
- **App keys in a KMS:** both Apps' private keys, and the key that signs registry tokens, live in a cloud KMS. The
  service asks the KMS to sign App JWTs and registry tokens; the keys never exist in the environment or on Railway.
  A leaked storage App key could write to every tenant's storage repository. Immutable releases and provenance
  (§10.3) make such writes detectable; the KMS makes stealing the key far harder in the first place.
- **Least privilege:** the split between the reader and storage Apps (§6.1).
- **Transport:** HTTPS only, with HSTS.
- **No oracle:** a crate the caller cannot read is a 404, never a 403 (SSO errors excepted, §6.5).
- **Limits:** request bodies capped at `MAX_CRATE_BYTES` + metadata, a publish rate limit per token hash, and
  timeouts on every GitHub call.

### 10.3 Integrity and provenance

- **Immutable releases:** once published, a release's tag and assets cannot be modified or deleted, and a deleted
  release's tag can never be reused. Enabling or disabling this needs repository administration, which neither App
  has. So the bytes behind `{name}-{version}` can never change, whoever holds our keys.
- **Checksums checked at publish:** the service computes the sha256 itself and checks it against the `digest` GitHub
  computes for the uploaded asset before publishing the release (§4.4).
- **Provenance:** a trusted publish stores the bound OIDC token as the release asset
  `{name}-{version}.provenance.jwt`. It is a statement signed by GitHub, not by us, that a specific workflow run in
  a specific repository asked to publish exactly this crate, version and sha256. We cannot forge it, and it cannot be
  reused for other bytes or another version.
- **Append-only index:** the service only ever appends lines or flips `yanked`. It refuses to rewrite a line any
  other way, even when asked by an administrator's edit arriving via `push`: such an edit is reported, not trusted.
- **Every write is a commit** in the customer's storage repository, made through the API by the storage App (so
  GitHub marks it verified), with the publisher's GitHub login in the commit message and the release notes.
- **Open-source verifier** (`privatecrates-verify`), which customers run in their own CI on every push to the storage
  repository and daily, with a read-only token. An admin adds the workflow with their own account (the account page's
  pre-filled GitHub link, or `cargo privatecrates add-verifier`); our Apps never write it. It reports:
  - any index history change other than App-authored appends and `yanked` flips;
  - any change to an existing `owners/` file or to an existing `privatecrates.toml` made by the storage App (the App
    only ever creates an owners file at a crate's first publish, and `privatecrates.toml` during onboarding; every
    other change there must come from a person);
  - any version whose release is missing, not immutable, or whose `.crate` digest differs from the index `cksum`;
  - any version whose provenance is missing (unless the crate allows manual publishing, in which case the version is
    listed as a manual publish), has an invalid GitHub signature, has an audience that does not name that crate,
    version and checksum, or names a repository, workflow or environment that the owners file did not allow at
    the time, or a trigger other than `push`, `release` or `workflow_dispatch` (the actor's permission at the
    time cannot be re-checked, §6.4);
  - crate names that clash with crates.io (§9.3).

  GitHub rotates its OIDC signing keys, and old keys eventually disappear from its key set, so provenance must be
  verified while the key is still published. The verifier records the versions it has verified in a small state file
  in the customer's own repository or CI cache, and reports a version it could not verify in time.

The pitch this supports: "don't trust us, verify us."

## 11. Implementation

- **Stack:** Rust, axum (HTTP), reqwest (GitHub and Stripe REST), serde, sha2, flate2 and tar (metadata check),
  jsonwebtoken (OIDC and registry tokens), moka (in-memory caches), apollo-errors (every server error has a stable
  code and HTTP status; the client tools use thiserror to stay small). The website is SvelteKit with Tailwind, built
  statically and served by the server. Check the latest version of every dependency when implementing.
- **Code layout (service):**
  - `tenant` (tenant discovery, `privatecrates.toml`, hostname routing)
  - `index` (paths and line format)
  - `publish` (body parsing and validation)
  - `github` (App JWTs via KMS, installation tokens, Contents API, Releases API, repository-set lookups)
  - `auth` (token extraction, repository-set cache, OIDC exchange, registry tokens, SSO errors)
  - `webhooks` (verification, de-duplication, cache and tenant updates)
  - `search` (per-tenant index, crates.io client)
  - `billing` (Stripe subscriptions, Checkout, portal, webhooks)
  - `account` and `session` (website sign-in, onboarding API)
  - `routes`
- **Separate crates, open source:** `cargo-credential-privatecrates` and `privatecrates-verify`, each released with prebuilt,
  attested binaries.
- **Configuration**, from the environment only; `docs/deploy.md` lists every variable per environment. Until
  milestone 4 moves them into a KMS, the App private keys and signing secrets are environment variables.
- **Deployment:** one container on Railway, with health check `GET /healthz` (no GitHub calls). Logs are structured,
  without tokens.
- **Conventions:** the same as worldbuilding.dev: clippy with warnings as errors, `cargo fmt`, tests as part of done,
  no dead code.

## 12. Later phases (the higher tier)

- **Hosted documentation:** `cargo doc` output for every published version, served under the same permissions. Builds
  run customer code (`build.rs`, proc-macros), so each runs in a throwaway sandbox with no network access. This is
  the first durable data the service stores; say so in the security section when it ships.
- **API search for coding agents:** a symbol index built from rustdoc JSON (nightly; the format changes), exposed as
  an MCP server, filtered by the caller's GitHub access.
- **Semver checks at publish:** run `cargo-semver-checks` and warn about breaking changes without a major bump.
- **Dependents view (opt-in):** which repositories depend on which versions. Needs contents read on consuming
  repositories, which works against least privilege, so it must be opt-in.
- **A web page** listing crates and versions the caller can read (GitHub sign-in).
- **Byte caching in object storage** (§8).

## 13. Testing

1. **Unit tests:**
   - index path layout;
   - index line encoding and decoding round trip against examples from the Cargo Book;
   - publish body parsing, including truncated and oversized bodies;
   - metadata and `.crate` consistency;
   - permission decisions from repository sets, including outside collaborators;
   - OIDC validation: signature, audience, expiry, wrong organisation, trusted-publishing workflow and environment
     matching, the first-publish rule;
   - bound publish tokens: refused when the uploaded bytes, crate or version differ from the audience;
   - manual publishing refused unless the crate allows it;
   - registry token signing and verification;
   - webhook signature verification, de-duplication and the cache entries each event drops;
   - `X-GitHub-SSO` responses turned into the SSO error;
   - optimistic-concurrency retry;
   - append-only enforcement.
2. **Integration tests against a fake GitHub:** an in-process axum mock of the GitHub endpoints used (installations,
   installation repositories with permissions, repository metadata, Contents API with blob shas, Releases and assets,
   App token minting, the device flow, OIDC signing keys, webhook deliveries). A fake KMS signs with a local key. The
   service runs against it with two tenants, and the real `cargo` binary drives:
   - `cargo publish` through the credential provider, in a simulated Actions environment with a fake OIDC endpoint,
     of two crates, one depending on the other plus a crates.io dependency;
   - `cargo publish` of a workspace, one bound token per crate;
   - a consumer project with `cargo generate-lockfile` and `cargo fetch`, through the credential provider;
   - a CI-style fetch through the credential provider with an OIDC exchange, and the clear error when
     `id-token: write` is missing;
   - `cargo publish` from a developer's machine, refused by default with the CI instructions, and accepted once the
     crate allows it;
   - `cargo search` returning private and (mocked) crates.io results;
   - yank and unyank;
   - a second user without access, who gets 404 on the index and refused downloads and publishes;
   - a user removed by webhook, who loses access before the TTL expires;
   - a tenant that cannot see the other tenant's crates;
   - a concurrent publish of two different crates;
   - a re-publish of the same version, which is refused;
   - a crash between publishing the release and appending the index, then a retry with the same bytes (reused) and
     with different bytes (refused);
   - a storage repository without immutable releases, where publishing is refused.
3. **Verifier tests:** storage repository histories that must pass (appends, yanks, manual publishes on a crate that
   allows them, an administrator's owners change) and ones that must be reported:
   - a rewritten index line, and a `cksum` that differs from the asset digest;
   - an App-authored change to an existing owners file or `privatecrates.toml`;
   - a version with no provenance, a forged or re-signed provenance token, and a genuine token reused for another
     version or checksum;
   - a provenance token from a workflow or environment the owners file did not allow;
   - a deleted or mutable release.
4. **Live smoke tests, manual and off CI:** against a throwaway test organisation, publish `hello_privatecrates` with
   `cargo publish` from a real Actions workflow and consume it using the credential provider, `gh auth token` and an Actions OIDC token; repeat
   against a second test organisation with SAML SSO enforced.

## 14. Milestones

1. **Read path** (built): both Apps, `config.json` with 401, sparse index, downloads with signed-URL caching,
   repository-set permissions, SSO errors, the credential provider. There is no import tool: existing crates are
   published afresh from CI, so every version has provenance.
2. **Write path** (built): trusted publishing with bound OIDC tokens from the credential provider, immutable releases,
   provenance assets, yank and unyank, append-only enforcement, and `privatecrates-verify`.
3. **CI and freshness:** OIDC exchange for reads in the credential provider, prebuilt provider binaries, webhooks,
   search. Deploy on Railway and move
   worldbuilding.dev and the movie site from git dependencies to the registry, with their Railway images built in
   GitHub Actions (§3.3).
4. **Launch:** the website and onboarding, Stripe billing, dev and production environments, KMS-held keys.
5. **Later phases:** §12.

## 15. Open questions

None at present.

### To confirm in a test organisation before building on it

These are believed true from GitHub's and Cargo's documentation, but the design depends on them:
1. `GET /user/installations/{id}/repositories` returns each repository's `permissions` (including `push`) for a
   user token from an App with only metadata read. The documentation says the user's access is included. If not,
   push checks fall back to `GET /repos/{owner}/{repo}`.
2. Enabling or disabling immutable releases needs repository administration, so an App with only contents write
   cannot turn it off; and an App can publish a draft release with assets, after which the release reports
   `immutable: true`.
3. An Actions OIDC audience of the form `{base_url}/publish/{name}/{version}/{sha256}` is accepted (length and
   characters).
4. How long GitHub keeps retired OIDC signing keys in its published key set, which sets how promptly the verifier
   must run (§10.3).

### Resolved

- **Name and domain:** PrivateCrates, at `privatecrates.dev`.
- **Credential provider name:** `cargo-credential-privatecrates`, following Cargo's naming convention for
  credential providers.
- **Licence:** BSL 1.1, converting to Apache-2.0 after four years (§2.4).
- **CI outside GitHub:** not at launch; no long-lived credentials (§3.3).
- **Railway builds:** Railway exposes build-time variables only as `ARG`s, so images are built in GitHub Actions or
  from vendored sources (§3.3).
- **Publishing:** plain `cargo publish`; from CI it carries provenance, and publishing from machines is opt-in per
  crate (§3.4). Cargo's credential provider protocol passes the crate's checksum with a publish request, so the
  provider can bind the token to the exact bytes without any wrapper.
- **Name clashes with crates.io:** refused by default, and checked daily afterwards (§9.3).
- **Retention:** every version kept forever, enforced by immutable releases (§5).
- **Outside collaborators:** can use the crates of the repositories they can read (§4.1).

## References

- Cargo Book: [Registry Index](https://doc.rust-lang.org/cargo/reference/registry-index.html),
  [Registry Authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html),
  [Credential Provider Protocol](https://doc.rust-lang.org/cargo/reference/credential-provider-protocol.html),
  [Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html).
- [RFC 3139: Cargo alternative registry authentication](https://rust-lang.github.io/rfcs/3139-cargo-alternative-registry-auth.html).
- GitHub: [OpenID Connect in Actions](https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect),
  [device flow for GitHub Apps](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app),
  [webhook events](https://docs.github.com/en/webhooks/webhook-events-and-payloads),
  [Stripe Billing](https://docs.stripe.com/billing),
  [immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases),
  [release asset digests](https://github.blog/changelog/2025-06-03-releases-now-expose-digests-for-release-assets/),
  [installation repositories for a user token](https://docs.github.com/en/rest/apps/installations#list-repositories-accessible-to-the-user-access-token).
- Railway: [build-time and runtime secrets](https://docs.railway.com/guides/build-time-vs-runtime-secrets).
- [Business Source License 1.1](https://mariadb.com/bsl11/).
- [GitHub Packages: Cargo support request (community discussion #12067)](https://github.com/orgs/community/discussions/12067).
- [crates.io trusted publishing](https://crates.io/docs/trusted-publishing), the model for §3.4 and §6.4.
- Existing private registries for comparison: Cloudsmith, JFrog Artifactory, AWS CodeArtifact, Kellnr, Meuse.
