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

- $100 per GitHub organisation per month, unlimited users, 14-day free trial (Stripe Checkout `trial_period_days`).
- Stripe is the source of truth. Each subscription's metadata holds `github_org_id` and `github_org_login`. The server
  loads subscriptions at start-up and keeps them current from Stripe webhooks; there is still no database.
- A tenant is **active** when its subscription status is `trialing` or `active`. `past_due` keeps working (Stripe
  retries payment). When a subscription is `canceled`/`unpaid`/absent: publishing is refused at once with
  `billing::subscription_inactive` (HTTP 402); reads keep working for 14 days after the period ended, then are refused
  with the same error. Messages point to `https://{BASE_DOMAIN}/account`.

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
      "tenant": { "slug": "acme", "registry_url": "https://acme.privatecrates.dev", "status": "trialing",
                  "trial_ends_at": "2026-10-11T00:00:00Z", "current_period_end": null } }
  ]
}
```
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
    { "id": "subscription", "status": "todo" }
  ],
  "suggested_slug": "acme"
}
```
Only org admins can act; members see the same checklist with `blocked` steps and a note to ask an admin.

### `POST /api/orgs/{org}/settings` `{"slug": "acme"}`
Admin only. Validates the slug (format, reserved, not taken) and has the storage App **create** `privatecrates.toml`
in the storage repository (creation only; refused if it exists). Returns the onboarding document.

### `POST /api/orgs/{org}/checkout`
Admin only. Creates a Stripe Checkout session (subscription, the configured price, 14-day trial, metadata
`github_org_id`/`github_org_login`, `client_reference_id` = org id). Returns `{"url": "https://checkout.stripe.com/…"}`.
Success URL: `/account?org=acme&checkout=success`; cancel URL: `/account?org=acme`.

### `POST /api/orgs/{org}/portal`
Admin only. Returns `{"url": …}` for the Stripe customer portal.

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
