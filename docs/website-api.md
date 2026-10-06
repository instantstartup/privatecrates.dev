# Website ↔ server contract

The website (SvelteKit, static build) is served by the Rust server on the **apex host** of each environment:

| Environment | Website and account API | Tenant registries | GitHub Apps | Stripe |
|---|---|---|---|---|
| dev | `https://dev.privatecrates.dev` | `https://{slug}.dev.privatecrates.dev` | "PrivateCrates Dev Reader", "PrivateCrates Dev Storage" | test mode |
| prod | `https://privatecrates.dev` | `https://{slug}.privatecrates.dev` | "PrivateCrates Reader", "PrivateCrates Storage" | live mode |

`BASE_DOMAIN` is `dev.privatecrates.dev` or `privatecrates.dev`. A request whose `Host` is exactly `BASE_DOMAIN` (or
`www.` + it, redirected) is a website/account request; `{slug}.BASE_DOMAIN` is a registry request.

Reserved slugs (refused at onboarding, never routed as tenants): `www`, `dev`, `api`, `app`, `docs`, `status`,
`mail`, `admin`, `billing`, `login`, `static`, `assets`.

## The preview

`PREVIEW` (default `true` until general availability; docs/preview.md §1) turns billing off whatever Stripe
configuration is present: every organisation's `plan` is `free` (with `members` still reported), `trial_available` is
`false`, nothing is enforced, no trial reminders are added to publishes, Stripe is never called (no subscription
loading, trials, Checkout or portal, and `/webhooks/stripe` answers 404), and `/trial`, `/checkout`, `/portal` and
`/billing-email` answer `409 billing::preview` for admins. The onboarding `plan` step is `done`, with the detail
"Free during the private preview." The billing model below applies with `PREVIEW=false`.

## Billing model

- **Free** for organisations with **5 or fewer members** (`FREE_MEMBER_LIMIT`, default 5): every feature, no Stripe
  customer, no card. Members are the organisation's active members (`GET /orgs/{org}/members` with the reader App's
  installation token, which has Members read); outside collaborators and pending invitations do not count. The count
  is cached for 24 hours and kept current by the `organization` member_added/member_removed webhooks.
- **Larger organisations: $10 a month for each member past 5, never more than $100 a month**, unlimited users, after
  a **3-month free trial with no card**. The Stripe price is $10 per unit; a subscription's quantity is the members past
  the free limit, at most 10 (`billing::billed_members`), set when it starts and kept in step with the member count by
  the membership webhooks and the periodic refresh, with `proration_behavior=none` (the next invoice uses it). A
  subscription of an organisation back at 5 or fewer members drops to quantity 0, and so costs nothing, rather than
  being cancelled. `GET /api/session` reports each organisation's `monthly_price_usd`. The trial (`TRIAL_DAYS`,
  default 90) is one click: the server creates the Stripe customer and subscription
  directly (`trial_period_days`, `payment_settings[save_default_payment_method]=on_subscription`,
  `trial_settings[end_behavior][missing_payment_method]=cancel`); no Checkout page. A card is added any time through
  the customer portal. Each organisation gets one trial (a returning organisation subscribes through Checkout, with
  a card and no trial).
- **Growing past the limit never breaks anything**: when a free organisation's member count first exceeds the limit
  and it has never had a subscription, the server starts its 3-month trial automatically (same call as above). It
  does so from the `organization` webhook and from the 10-minute refresh, for any registered organisation over the
  limit that has never had a subscription (so one registered while already over the limit gets its trial within 10
  minutes if no admin starts it first).
- Without Stripe configured (local development, and dev until billing is set up) plans are still computed and shown,
  but nothing is enforced: every tenant is active, `trial_available` is `false`, and `tenant.status` is `null`.
- Stripe is the source of truth for subscriptions. Each subscription's metadata holds `github_org_id` and
  `github_org_login`. The server loads subscriptions at start-up, re-lists them every 10 minutes, and keeps them
  current from Stripe webhooks: billing state has no database of ours. The only thing the server stores itself is the
  terms acceptance records, in Postgres (see *Terms* below).
- A tenant is **active** when it is free (at or under the limit), or its subscription is `trialing`, `active` or
  `past_due` (Stripe retries payment). Otherwise (subscription `canceled`/`unpaid`, e.g. a trial that ended with no
  card): publishing is refused at once with `billing::subscription_inactive` (HTTP 402); reads keep working for 14 days
  after the period ended, then are refused with the same error. Messages point to `https://{BASE_DOMAIN}/account`.
- **Reminders**: in the last 14 days of a trial with no card, successful publishes return a Cargo warning ("the free
  trial ends on …; add a card at …/account") and the account page shows a banner. Stripe's own trial-ending email is
  enabled in the Dashboard (see `docs/deploy.md`).
- An organisation that shrinks to the limit becomes free again at once; the account page tells admins with a
  subscription that they can cancel it in the billing portal (we do not cancel automatically: counts can fluctuate).

## Terms

An organisation admin accepts the current PrivateCrates terms (version `private-preview-2026-09-28-2`, published at
`https://{BASE_DOMAIN}/legal/terms`) before its registry is created, and an admin of a registry set up earlier is
asked to. Each acceptance is recorded in our own Postgres (`DATABASE_URL`), not in the customer's repository: the
organisation (id, login), the admin (GitHub id, login), the version, the time, `via` (`website` for the session
cookie, `cli` for a bearer token) and the exact statement accepted, "I have read and accept the PrivateCrates private preview
terms (private-preview-2026-09-28-2) on behalf of {org}". The table is append-only; the first acceptance of each version is kept
and a repeat is a no-op. A registry without an acceptance keeps working.

`accept_terms` must be the current version as a string; anything else (missing, another version, another type) is
refused with `account::terms_not_accepted` (400), whose detail names the version and the terms' URL. Neither the
website nor the CLI fills it in on the admin's behalf.

## Session

- Sign-in uses the **reader App's web flow** (needs `READER_APP_CLIENT_SECRET`). The resulting user access token is
  kept only in an encrypted, `HttpOnly`, `Secure`, `SameSite=Lax` cookie `pc_session` (AES-256-GCM, key
  `SESSION_SECRET`), valid until the token expires (8 hours). Nothing is stored server-side.
- State-changing requests (`POST`) must send `Content-Type: application/json` and an `Origin` equal to the apex URL;
  otherwise 403. This is the CSRF protection.
- **Bearer tokens, for tools and agents.** `/api/session` and `/api/orgs/{org}/…` also accept
  `Authorization: Bearer ghu_…`, a reader App user token from the device flow (as `cargo privatecrates login`
  stores it), in place of the cookie. Responses are the same JSON.
  - When an `Authorization` header is present it alone authenticates the request; the cookie is ignored.
  - Bearer requests skip the CSRF check above: a bearer token is not an ambient credential (a browser never attaches
    it by itself, and another site cannot set the header without a CORS preflight, which the server never grants).
  - Only reader App user tokens (`ghu_…`) are accepted. Any other token (`gho_`, `ghp_`, `github_pat_`, `ghs_`,
    registry or OIDC tokens) is refused with `account::token_not_accepted` (401), to keep broad tokens out.
  - An expired or revoked bearer token gets `account::sign_in_required` (401) from `GET /api/session` too, rather
    than the signed-out document.
  - Tokens are never logged.
- `GET /api/v1/auth` on the apex host returns `{"github_client_id": …, "github_url": …}`, the reader App's client ID
  for the device flow, as each registry host does (SPEC §3.1).

## Endpoints (apex host only)

All JSON. Errors use `{"errors":[{"detail": "...", "code": "..."}]}` with the apollo-errors code. `POST` bodies are
`{}`, except for `/settings` (`{"slug": …, "accept_terms": …}`), `/terms` (`{"accept_terms": …}`), and `/trial` and
`/billing-email` (`{"billing_email": …}`).

### `GET /auth/github/login?return_to=/account`
Redirects to GitHub's authorisation page for the reader App, with a signed `state` (includes `return_to`, which must be
a same-site path).

### `GET /auth/github/callback?code=…&state=…`
Exchanges the code, sets `pc_session`, redirects to `return_to`.

### `POST /auth/logout`
Clears the cookie. `204`.

### `POST /auth/logout-everywhere`
For a lost or stolen device. Revokes the reader App's grant for the signed-in user (`DELETE
/applications/{client_id}/grant`), which ends every token GitHub issued them for the App, on every device, refresh
tokens included; drops their cached permissions; clears the cookie. `204`. Other users are unaffected.

### `GET /api/session`
```json
{
  "user": { "login": "alice", "avatar_url": "https://…", "name": "Alice" },
  "orgs": [
    { "id": 100, "login": "acme", "avatar_url": "https://…", "role": "admin",
      "members": 12, "free_member_limit": 5,
      "plan": "trial", "monthly_price_usd": 70, "trial_ends_at": "2026-12-26T00:00:00Z", "has_payment_method": false,
      "billing_email_missing": false, "current_period_end": null, "trial_available": false,
      "tenant": { "slug": "acme", "registry_url": "https://acme.privatecrates.dev", "status": "trialing",
                  "trial_ends_at": "2026-12-26T00:00:00Z", "current_period_end": null },
      "terms_accepted": true, "invited": true, "personal": false }
  ],
  "install_url": "https://github.com/apps/privatecrates-reader/installations/new",
  "preview": false,
  "invite_only": false,
  "invitations_requested": [],
  "terms": { "version": "private-preview-2026-09-28-2", "url": "https://privatecrates.dev/legal/terms" }
}
```
`preview` is `true` during the preview (then every `plan` is `free`). `terms` (also when signed out) is the version an
admin accepts now and where to read it. Per organisation, `terms_accepted` says whether an admin has accepted that
version (looked up in our records, cached for a minute; `false` if the lookup fails).
Per organisation, `plan` is one of `free` (at or under the member limit), `trial`, `paid`, `past_due` or `inactive`
(ended or never started while over the limit); `members` is `null` when the count is unknown (the reader App is not
installed yet). `trial_available` says whether `POST /trial` will work. `billing_email_missing` is `true` when the
organisation has a subscription whose Stripe customer has no email, as a trial started automatically does not (an
admin sets one with `POST /billing-email`); otherwise `false`. `tenant.status` keeps the raw Stripe subscription
status, or `null`.

`install_url` (always present) installs the reader App on a new organisation: a GitHub App user token only sees
organisations that have installed the App, so a new organisation appears in `orgs` only after that.
`user` is `null` when signed out (then `orgs` is `[]`). `orgs` lists organisations the user belongs to (from the user
token); `tenant` is `null` when the org is not set up. `role` is `admin` or `member`.
The user's own account is listed first, with `"personal": true`, once the reader App is installed on it or it has a
registry (SPEC §6.8): its `role` is `admin`, its `plan` is always `free`, and every `/api/orgs/{org}/…` endpoint
accepts its login. `invited`, `invite_only` and `invitations_requested` are the private preview's (docs/preview.md §5).

### `GET /api/orgs/{org}/onboarding`
For an org the user belongs to. Each step is `done`, `todo` or `blocked`, with a URL for the action where there is one.
```json
{
  "org": { "id": 100, "login": "acme" },
  "steps": [
    { "id": "reader_app", "status": "done", "action_url": "https://github.com/apps/privatecrates-reader/installations/new" },
    { "id": "storage_repo", "status": "todo", "detail": "Create a private repository, e.g. acme/crates-store, and enable immutable releases in Settings → General → Releases.", "action_url": "https://github.com/organizations/acme/repositories/new" },
    { "id": "storage_app", "status": "todo", "action_url": "https://github.com/apps/privatecrates-storage/installations/new" },
    { "id": "settings", "status": "todo", "detail": "Choose your registry name." },
    { "id": "plan", "status": "todo", "detail": "12 members: start your 3-month free trial, no card needed." }
  ],
  "suggested_slug": "acme",
  "terms": { "version": "private-preview-2026-09-28-2", "url": "https://privatecrates.dev/legal/terms", "accepted": false }
}
```
Only org admins can act; members see the same checklist with `blocked` steps and a note to ask an admin.

### `POST /api/orgs/{org}/settings` `{"slug": "acme", "accept_terms": "private-preview-2026-09-28-2", "allow_manual_publish": false}`
Admin only. `allow_manual_publish` (optional, default `false`) is the admin's choice of whether crates may also be
published from developers' machines; it is written to `privatecrates.toml` as the default for every repository. Validates the slug (format, reserved), then `accept_terms` (`account::terms_not_accepted` 400), then that
the organisation has no registry and the slug is not taken. It records the admin's acceptance of the terms first (if
that fails, `records::unavailable` 503 and nothing is created), then has the storage App **create**
`privatecrates.toml` in the storage repository (creation only; refused if it exists). Returns the onboarding document.

### `POST /api/orgs/{org}/terms` `{"accept_terms": "private-preview-2026-09-28-2"}`
Admin only. For a registry set up before the terms, or before their current version: records the admin's acceptance.
Accepting again succeeds and records nothing new. Refused with `account::terms_not_accepted` 400 for anything but the
current version, and `account::not_set_up` 409 if the organisation has no registry (its terms are accepted at set-up).
Returns the onboarding document.

The `plan` step (formerly `subscription`) is `done` when the organisation is free or has an active, trialing or
past-due subscription; otherwise `todo` with a `detail` saying which action applies.

### `POST /api/orgs/{org}/trial` `{"billing_email": "billing@acme.example"}`
Admin only. Starts the 3-month, no-card trial. `billing_email` is required: it is set as the Stripe customer's email,
so that Stripe's reminder before the trial ends (and later invoices) reach someone; the server does not log it. A
missing or invalid address (a basic check: `local@domain.tld`, one address, no spaces) is refused with
`billing::email_invalid` 400, before anything else is checked. Otherwise refused with `billing::trial_used` 409 if the
organisation had a trial, `billing::already_subscribed` 409 if subscribed, `billing::free_plan` 409 if at or under the
member limit. Returns the onboarding document. A trial started automatically (the organisation grew past the limit)
has no billing email until an admin sets one (below).

### `POST /api/orgs/{org}/billing-email` `{"billing_email": "billing@acme.example"}`
Admin only. Sets the email of the organisation's Stripe customer, where Stripe sends the reminder before the trial
ends and invoices. The same check as `/trial` (`billing::email_invalid` 400); refused with `billing::no_subscription`
409 if the organisation has no subscription, and `billing::not_configured` 503 without Stripe. The session shows
`billing_email_missing: false` at once. Returns the onboarding document.

### `POST /api/orgs/{org}/checkout`
Admin only; body `{}`. For organisations that cannot have a trial (a returning organisation): a Stripe Checkout
session (subscription, the configured price, card required, no trial, metadata `github_org_id`/`github_org_login`,
`client_reference_id` = org id). Refused with `billing::already_subscribed` 409 if subscribed, `billing::free_plan`
409 if at or under the member limit, and `billing::trial_available` 409 if the organisation can still start its
trial. Returns `{"url": "https://checkout.stripe.com/…"}`. Success URL:
`/account?org=acme&checkout=success`; cancel URL: `/account?org=acme`.

### `POST /api/orgs/{org}/portal`
Admin only; body `{}`. Returns `{"url": …}` for the Stripe customer portal (add a card, cancel, invoices).

### `GET /api/orgs/{org}/compliance?before={sha}`
Any active member of the organisation (not only admins); the same cookie or bearer authentication. Refused with
`account::org_not_found` 404 for anyone else, and `compliance::not_set_up` 404 when the organisation has no registry.
Built from the storage repository with the storage App's read access, cached for 5 minutes per organisation and
discarded by a `push` webhook for the storage repository. `Cache-Control: no-store`.
```json
{
  "org": { "id": 100, "login": "acme" },
  "generated_at": "2026-09-27T14:30:00Z",
  "integrity": {
    "versions": 142, "immutable": 142, "digest_matches": 142, "provenance": 139, "manual": 3,
    "problems": [
      { "code": "digest_mismatch", "crate": "tools", "version": "0.3.0",
        "detail": "the .crate file does not match the index checksum" },
      { "code": "manual_publish", "crate": "tools", "version": "0.2.0",
        "detail": "published manually, without provenance; check that its publisher meant to" }
    ]
  },
  "publishers": [
    { "crate": "story_engine", "repository": "acme/story-engine", "workflows": ["release.yml"],
      "environment": "crates", "manual_publish": false }
  ],
  "risks": [
    { "code": "missing_provenance", "crate": "tools", "version": "0.2.0", "detail": "published without provenance, …" },
    { "code": "manual_publish_allowed", "crate": "tools", "detail": "versions may be published from a developer's machine, …" },
    { "code": "name_clash", "crate": "serde_utils", "detail": "a crate with this name exists on crates.io; …" },
    { "code": "no_verify_workflow", "detail": "no workflow in acme/crates-store runs privatecrates-verify, …" }
  ],
  "audit": [
    { "at": "2026-09-27T14:20:01Z", "action": "publish", "crate": "story_engine", "version": "0.2.0",
      "by": "alice via workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42, attempt 1), triggered by push on refs/tags/v0.2.0",
      "provenance": true, "commit": "abc1234…" }
  ],
  "audit_next_before": "9f8e7d6…"
}
```
- **`integrity`** runs `privatecrates-verify`'s own per-version checks (its `check` module): the release exists and is
  immutable, the `.crate` digest matches the index `cksum`, and the provenance was signed by GitHub for the owning
  repository, an allowed workflow and trigger (`push`, `release` or `workflow_dispatch`), and the required environment
  (checked against the crate's current owners file). Whether the run's actor could create releases is not checked
  here: GitHub answers for current permissions only.
  `immutable`, `digest_matches` and `provenance` count the versions passing each check; `manual` counts versions
  published manually where the crate allows it. `problems` lists what the checks found, per version, with the
  verifier's message as `detail` and one of these `code`s:
  - `release_missing`: the version is in the index but its release is not (nothing else is checked);
  - `release_mutable`: the release is a draft or not immutable, so its files could have been changed;
  - `digest_mismatch`: GitHub's digest of the `.crate` file differs from the index `cksum`;
  - `crate_missing`: the release has no `.crate` file, or GitHub reports no digest for it;
  - `provenance_missing`: no provenance where it is required (a crate's first version, or a crate that does not
    allow manual publishing);
  - `provenance_invalid`: the provenance is not signed by GitHub, or names another repository, workflow, environment,
    crate, version or checksum, or a trigger other than `push`, `release` or `workflow_dispatch`;
  - `manual_publish`: published manually, without provenance, as the crate allows; worth a look, not a failure;
  - `index_invalid`: an index file, or a line of it, cannot be read (`version` is `null`).

  A version that passed (anything but `manual_publish` counts against it) is not checked again: releases are
  immutable. The organisation's own verifier, run in its storage repository, remains the independent check.
- **`publishers`**: one per owners file, sorted by crate: who may publish it.
- **`risks`**, each with a `code` and a `detail`, and `crate` and `version` where they apply: `missing_provenance` (a
  version without provenance, allowed or not), `manual_publish_allowed` (manual publishing allowed: with no `crate` or `repository`, by the default in `privatecrates.toml`; with `repository`, by that repository's own setting; with `crate`, by its owners file), `name_clash`
  (a crate whose name exists on crates.io), `no_verify_workflow` (no file in the storage repository's
  `.github/workflows/` mentions `privatecrates-verify`).
- **`audit`**: the storage repository's commits that changed `index/`, `owners/` or `privatecrates.toml`, newest
  first, 50 per page. `commit` is the full sha; `audit_next_before` is the sha to pass as `?before=` for the next
  page, or `null` when there are no older entries (an unknown `before` gives an empty page). `at` is the commit date. `action` is one of `publish`,
  `yank`, `unyank`, `owners_change` (including the owners file a crate's first publish creates), `settings_change`,
  and `index_change` (a person edited the index directly, which only the storage App should do). For the storage
  App's commits (by its bot and signed by GitHub) `crate`, `version` and `by` come from the commit message: `by` is
  what follows `Published by ` in the message: `alice via workflow …@ref (run N, attempt M), triggered by push on
  refs/tags/v1` or `alice (manual publish, no provenance)`, or for publishes made before workflow actors were
  checked, `workflow …@ref (run N)` or `user alice (manual publish, no provenance)`, or the login that yanked,
  unyanked or set the registry up. A person's commit is classified by what it changed, with `by` its author's GitHub
  login (or the name git recorded), and `crate` and `version` `null`. `provenance` is set for publishes only: whether
  the release has a provenance asset.

### `GET /api/orgs/{org}/compliance/audit.csv?before={sha}`
The same access. The whole audit trail (or what is older than `before`), as `text/csv` (RFC 4180, CRLF line ends,
`Content-Disposition: attachment; filename="acme-audit.csv"`), with the columns
`at,action,crate,version,by,provenance,commit`; empty fields for `null`. A field starting with `=`, `+`, `-` or `@`
is prefixed with `'`, so a spreadsheet does not run it as a formula.

### `GET /api/status`
Unauthenticated, `Cache-Control: no-store`: the server's own health, for the status page
(docs/trust-and-status.md §2). Nothing about any customer: no organisation names, URLs or tokens.
```json
{
  "version": "0.1.0",
  "started_at": "2026-09-27T14:13:23Z",
  "github": {
    "window_seconds": 300,
    "requests": 1840, "errors": 3, "rate_limited": 0,
    "latency_ms_p50": 200, "latency_ms_p95": 750,
    "last_error_at": "2026-09-27T14:20:01Z"
  },
  "stripe": { "configured": true, "requests": 12, "errors": 0 },
  "tenants": 14
}
```
- `version` is the server's crate version; `started_at` when the process started; `tenants` how many organisations
  are served.
- `github` counts every call the server made to GitHub (API, sign-in and release downloads) in the last 5 minutes
  (to within 10 seconds), by outcome: `errors` are calls with no answer or a 5xx; `rate_limited` are 429s and 403s
  with `x-ratelimit-remaining: 0`; every other answer, including a 404 meaning "not found", is fine. The latencies
  are the upper bounds of histogram bins (10 ms to 10 s), `null` without calls. `last_error_at` is the last error or
  rate limit since the server started, or `null`.
- `stripe` the same for Stripe, when `configured`; its `errors` include rate limits.

### `GET /api/errors`
The error catalog from `apollo_errors::error_catalog()`: `[{"code": "publish::ci_only", "message": "…", "http_status": 403}]`,
for the docs' error reference page. Among the publish refusals (SPEC §6.4):
- `publish::trigger_not_allowed` (403): the publishing workflow was triggered by an event other than `push`,
  `release` or `workflow_dispatch`; the detail names the event and why others (`pull_request_target`,
  `issue_comment`, `workflow_run`, `schedule`, …) are refused.
- `publish::actor_cannot_release` (403): the account that started the workflow run does not have Write access to the
  owning repository (permission to create releases), or is a bot not listed in the crate's `publish_bots`; the detail
  names the actor and repository, and the extension `repository` carries the repository.

### `POST /webhooks/stripe`
Stripe webhooks, verified with `Stripe-Signature` and `STRIPE_WEBHOOK_SECRET`: `checkout.session.completed`,
`customer.subscription.created|updated|deleted`.

### `POST /webhooks/github`
GitHub webhooks (SPEC §7), verified with `X-Hub-Signature-256` and `WEBHOOK_SECRET`.

## Website pages

`/` (landing), `/pricing`, `/docs` (+ `/docs/setup`, `/docs/ci`, `/docs/publishing`, `/docs/verify`, `/docs/security`,
`/docs/errors`), `/account` (sign-in, org list, onboarding wizard, billing), `/legal/terms`, `/legal/privacy`
(clearly marked drafts). Static pages are prerendered; `/account` is client-rendered against the API.
