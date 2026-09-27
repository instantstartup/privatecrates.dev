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

## Billing model

- **Free** for organisations with **5 or fewer members** (`FREE_MEMBER_LIMIT`, default 5): every feature, no Stripe
  customer, no card. Members are the organisation's active members (`GET /orgs/{org}/members` with the reader App's
  installation token, which has Members read); outside collaborators and pending invitations do not count. The count
  is cached for 24 hours and kept current by the `organization` member_added/member_removed webhooks.
- **Larger organisations: $100 per organisation per month**, unlimited users, after a **3-month free trial with no
  card** (`TRIAL_DAYS`, default 90). The trial is one click: the server creates the Stripe customer and subscription
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
  current from Stripe webhooks; there is still no database.
- A tenant is **active** when it is free (at or under the limit), or its subscription is `trialing`, `active` or
  `past_due` (Stripe retries payment). Otherwise (subscription `canceled`/`unpaid`, e.g. a trial that ended with no
  card): publishing is refused at once with `billing::subscription_inactive` (HTTP 402); reads keep working for 14 days
  after the period ended, then are refused with the same error. Messages point to `https://{BASE_DOMAIN}/account`.
- **Reminders**: in the last 14 days of a trial with no card, successful publishes return a Cargo warning ("the free
  trial ends on …; add a card at …/account") and the account page shows a banner. Stripe's own trial-ending email is
  enabled in the Dashboard (see `docs/deploy.md`).
- An organisation that shrinks to the limit becomes free again at once; the account page tells admins with a
  subscription that they can cancel it in the billing portal (we do not cancel automatically: counts can fluctuate).

## Session

- Sign-in uses the **reader App's web flow** (needs `READER_APP_CLIENT_SECRET`). The resulting user access token is
  kept only in an encrypted, `HttpOnly`, `Secure`, `SameSite=Lax` cookie `pc_session` (AES-256-GCM, key
  `SESSION_SECRET`), valid until the token expires (8 hours). Nothing is stored server-side.
- State-changing requests (`POST`) must send `Content-Type: application/json` and an `Origin` equal to the apex URL;
  otherwise 403. This is the CSRF protection.

## Endpoints (apex host only)

All JSON. Errors use `{"errors":[{"detail": "...", "code": "..."}]}` with the apollo-errors code.

### `GET /auth/github/login?return_to=/account`
Redirects to GitHub's authorisation page for the reader App, with a signed `state` (includes `return_to`, which must be
a same-site path).

### `GET /auth/github/callback?code=…&state=…`
Exchanges the code, sets `pc_session`, redirects to `return_to`.

### `POST /auth/logout`
Clears the cookie. `204`.

### `GET /api/session`
```json
{
  "user": { "login": "alice", "avatar_url": "https://…", "name": "Alice" },
  "orgs": [
    { "id": 100, "login": "acme", "avatar_url": "https://…", "role": "admin",
      "members": 12, "free_member_limit": 5,
      "plan": "trial", "trial_ends_at": "2026-12-26T00:00:00Z", "has_payment_method": false,
      "current_period_end": null, "trial_available": false,
      "tenant": { "slug": "acme", "registry_url": "https://acme.privatecrates.dev", "status": "trialing",
                  "trial_ends_at": "2026-12-26T00:00:00Z", "current_period_end": null } }
  ],
  "install_url": "https://github.com/apps/privatecrates-reader/installations/new"
}
```
Per organisation, `plan` is one of `free` (at or under the member limit), `trial`, `paid`, `past_due` or `inactive`
(ended or never started while over the limit); `members` is `null` when the count is unknown (the reader App is not
installed yet). `trial_available` says whether `POST /trial` will work. `tenant.status` keeps the raw Stripe
subscription status, or `null`.

`install_url` (always present) installs the reader App on a new organisation: a GitHub App user token only sees
organisations that have installed the App, so a new organisation appears in `orgs` only after that.
`user` is `null` when signed out (then `orgs` is `[]`). `orgs` lists organisations the user belongs to (from the user
token); `tenant` is `null` when the org is not set up. `role` is `admin` or `member`.

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
  "suggested_slug": "acme"
}
```
Only org admins can act; members see the same checklist with `blocked` steps and a note to ask an admin.

### `POST /api/orgs/{org}/settings` `{"slug": "acme"}`
Admin only. Validates the slug (format, reserved, not taken) and has the storage App **create** `privatecrates.toml`
in the storage repository (creation only; refused if it exists). Returns the onboarding document.

The `plan` step (formerly `subscription`) is `done` when the organisation is free or has an active, trialing or
past-due subscription; otherwise `todo` with a `detail` saying which action applies.

### `POST /api/orgs/{org}/trial`
Admin only; body `{}`. Starts the 3-month, no-card trial (refused with `billing::trial_used` 409 if the organisation
had one, `billing::already_subscribed` 409 if subscribed, `billing::free_plan` 409 if at or under the member limit).
Returns the onboarding document.

### `POST /api/orgs/{org}/checkout`
Admin only; body `{}`. For organisations that cannot have a trial (a returning organisation): a Stripe Checkout
session (subscription, the configured price, card required, no trial, metadata `github_org_id`/`github_org_login`,
`client_reference_id` = org id). Refused with `billing::already_subscribed` 409 if subscribed, `billing::free_plan`
409 if at or under the member limit, and `billing::trial_available` 409 if the organisation can still start its
trial. Returns `{"url": "https://checkout.stripe.com/…"}`. Success URL:
`/account?org=acme&checkout=success`; cancel URL: `/account?org=acme`.

### `POST /api/orgs/{org}/portal`
Admin only; body `{}`. Returns `{"url": …}` for the Stripe customer portal (add a card, cancel, invoices).

### `GET /api/errors`
The error catalog from `apollo_errors::error_catalog()`: `[{"code": "publish::ci_only", "message": "…", "http_status": 403}]`,
for the docs' error reference page.

### `POST /webhooks/stripe`
Stripe webhooks, verified with `Stripe-Signature` and `STRIPE_WEBHOOK_SECRET`: `checkout.session.completed`,
`customer.subscription.created|updated|deleted`.

### `POST /webhooks/github`
GitHub webhooks (SPEC §7), verified with `X-Hub-Signature-256` and `WEBHOOK_SECRET`.

## Website pages

`/` (landing), `/pricing`, `/docs` (+ `/docs/setup`, `/docs/ci`, `/docs/publishing`, `/docs/verify`, `/docs/security`,
`/docs/errors`), `/account` (sign-in, org list, onboarding wizard, billing), `/legal/terms`, `/legal/privacy`
(clearly marked drafts). Static pages are prerendered; `/account` is client-rendered against the API.
