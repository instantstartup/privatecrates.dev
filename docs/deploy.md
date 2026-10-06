# Deploying PrivateCrates

This guide sets up the two hosted environments from nothing: one Railway project with a `dev` and a `production`
environment, DNS on Cloudflare, two GitHub Apps per environment, and Stripe. Every command is meant to be copied
and pasted. Values in `<angle brackets>` are yours to fill in.

| | dev | production |
|---|---|---|
| Deploys from | `dev` branch | `main` branch |
| Apex (website, account API, webhooks) | `https://dev.privatecrates.dev` | `https://privatecrates.dev` (+ `www.` redirect) |
| Tenant registries | `https://{slug}.dev.privatecrates.dev` | `https://{slug}.privatecrates.dev` |
| GitHub Apps | PrivateCrates Dev Reader / Dev Storage | PrivateCrates Reader / Storage |
| Stripe | test mode | live mode |

Contents:

1. [Decisions](#1-decisions)
2. [Accounts you need](#2-accounts-you-need)
3. [Secrets you generate](#3-secrets-you-generate)
4. [GitHub Apps](#4-github-apps)
5. [Stripe](#5-stripe)
6. [Railway](#6-railway)
7. [Environment variables](#7-environment-variables)
8. [Custom domains and Cloudflare DNS](#8-custom-domains-and-cloudflare-dns)
9. [Verify a deployment](#9-verify-a-deployment)
10. [Promote to production](#10-promote-to-production)
11. [Roll back](#11-roll-back)
12. [Rotate secrets](#12-rotate-secrets)
13. [Release the client tools](#13-release-the-client-tools)
14. [Status page](#14-status-page)
15. [Sources](#15-sources)

---

## 1. Decisions

**Dev deploys from the `dev` branch and production from `main`, not from tags.** Railway's GitHub integration
deploys the latest commit of one branch per environment; it has no tag trigger [R3]. Work lands on `dev` first;
promoting is a fast-forward of `main` to a commit on `dev` that already runs in dev (§10), and with Railway's *Wait
for CI* the deploy starts only once CI has passed on that commit [R3]. Tags (`vX.Y.Z`) are used only to release the open-source client tools (§13).

**Exactly one replica per environment.** The server keeps tenants, permission caches and subscription state in
memory and updates them from webhooks; a second replica would miss the webhooks the first received. Do not raise
the replica count or enable App Sleeping (serverless).

**One small Postgres database per environment, for terms acceptances only.** Everything else the server knows comes
from GitHub or Stripe and is rebuilt at start-up. The record that an organisation admin accepted the terms
(docs/preview.md §2) is our evidence, so it cannot live in the customer's storage repository, which they can delete:
it is kept in Railway Postgres, in the same project and region (§6.3). The `terms_acceptances` table is
**append-only**: the server only inserts (the first acceptance of each version is kept, and a repeat is a no-op) and
never updates or deletes a row. Deployments refuse to start without `DATABASE_URL`, so that a registry is never
created without its acceptance recorded.

**Cloudflare DNS only ("grey cloud") for every record; Railway terminates TLS.** Railway supports custom domains and
wildcard custom domains at any single level, including nested ones such as `*.dev.privatecrates.dev`, and issues
their certificates itself; for a wildcard it needs an `_acme-challenge` CNAME delegated to `authorize.railwaydns.net`,
which must not be proxied [R1]. With Cloudflare proxying instead:

- Railway requires SSL/TLS mode **Full**, and says Full (Strict) "will not work as intended" [R1], so the hop
  from Cloudflare to Railway would not verify certificates.
- Cloudflare's free Universal SSL certificate covers only one subdomain level, so `*.dev.privatecrates.dev` would
  need Advanced Certificate Manager (a paid add-on) with Universal SSL disabled [R1][R6].
- Cloudflare's bot and WAF features can challenge `cargo`, CI and webhook traffic, which cannot solve challenges.

What DNS-only gives up: Cloudflare's caching and DDoS shielding in front of the origin, and hiding the origin. The
registry's responses are per-user and authenticated, so there is little to cache. If shielding is ever needed,
proxy only the apex and `www` (first-level names, covered by Universal SSL) and set SSL/TLS to Full.

**Railway plan.** Production needs three custom domains on one service (`privatecrates.dev`, `www.privatecrates.dev`,
`*.privatecrates.dev`). Railway allows two custom domains per service on Hobby and 20 on Pro [R1], so the project
must be on **Pro**.

## 2. Accounts you need

- **Railway**, Pro plan, with the Railway GitHub App installed on `worldbuilding-dev` with access to
  `worldbuilding-dev/privatecrates.dev` (Railway → Account → Integrations → GitHub).
- **Cloudflare**, with the `privatecrates.dev` zone active (nameservers moved to Cloudflare).
- **GitHub**: an owner of the `worldbuilding-dev` organisation (the Apps belong to it), plus a throwaway test
  organisation for onboarding tests, e.g. `privatecrates-test`.
- **Stripe**, one account; test mode for dev, live mode (account activated) for production.
- **crates.io**, for the client tools (§13).

Local tools: `git`, `curl`, `jq`, `openssl`, `python3` (to serve the App manifest page), and the Railway CLI:

```sh
brew install railway        # or: npm install -g @railway/cli
railway login
```

## 3. Secrets you generate

Generate these once per environment; never reuse a dev secret in production. Keep a copy in the team's password
manager: Railway is where they live, but it is not a backup.

| Name | Generate with | Notes |
|---|---|---|
| `REGISTRY_TOKEN_SECRET` | `openssl rand -base64 48` | Signs read-only registry tokens (`pcr_…`). At least 32 bytes. Rotating it invalidates every outstanding `pcr_` token (CI re-exchanges within the hour). |
| `SESSION_SECRET` | `openssl rand -base64 48` | Encrypts the `pc_session` cookie (AES-256-GCM). Rotating it signs everyone out. |
| `WEBHOOK_SECRET` | `openssl rand -hex 32` | Verifies GitHub webhook deliveries. The server has one secret, so **both** Apps of an environment must be set to it (§4.3). |

For example:

```sh
umask 077; mkdir -p ~/privatecrates-secrets/dev ~/privatecrates-secrets/production
for env in dev production; do
  openssl rand -base64 48 | tr -d '\n' > ~/privatecrates-secrets/$env/REGISTRY_TOKEN_SECRET
  openssl rand -base64 48 | tr -d '\n' > ~/privatecrates-secrets/$env/SESSION_SECRET
  openssl rand -hex 32    | tr -d '\n' > ~/privatecrates-secrets/$env/WEBHOOK_SECRET
done
```

## 4. GitHub Apps

Each environment has a **reader** App and a **storage** App (SPEC §6.1), owned by `worldbuilding-dev` and
installable by any organisation. Their manifests are in [`deploy/github-apps/`](../deploy/github-apps):

| File | App | Permissions | Webhook events |
|---|---|---|---|
| `dev-reader.json`, `prod-reader.json` | reader | Metadata: read; Organisation members: read | `member`, `membership`, `organization`, `repository`, `team` |
| `dev-storage.json`, `prod-storage.json` | storage | Contents: read and write (Metadata: read is added by GitHub for every App) | `push` |

`installation`, `installation_repositories` and `github_app_authorization` (SPEC §7) are delivered to every App
automatically and cannot be subscribed to, so they are not listed [G3]. `marketplace_purchase` is not used (billing
is Stripe). Every event the reader subscribes to needs the *Members* organisation permission, except `repository`,
which needs *Metadata* [G3].

What each manifest sets [G1]: name, homepage (`url`), `hook_attributes.url` = `https://{apex}/webhooks/github`,
`callback_urls` = `https://{apex}/auth/github/callback` (reader only), `setup_url` = `https://{apex}/account` (where
GitHub sends an admin after installing), `request_oauth_on_install: false`, `public: true`, `default_permissions`,
`default_events`. What a manifest **cannot** set is done by hand in §4.3: device flow, the webhook secret, and the
expiry of user tokens (on by default for new Apps [G4]).

### 4.1 Create the Apps (manifest flow)

GitHub's manifest flow: POST the manifest to
`https://github.com/organizations/worldbuilding-dev/settings/apps/new`, confirm the name on GitHub, and GitHub
redirects to `redirect_url` with a one-time `code` valid for one hour [G1]. `create.html` does the POST and, as its
own `redirect_url`, shows the command to convert the code.

```sh
cd deploy/github-apps
python3 -m http.server 8765 --bind 127.0.0.1
# open http://127.0.0.1:8765/create.html in a browser signed in to GitHub as a worldbuilding-dev owner
```

For each of `dev-reader`, `dev-storage` (and later `prod-reader`, `prod-storage`): choose it, press **Create this
App on GitHub**, confirm on GitHub. Back on the page, run the command it shows. It is equivalent to:

```sh
umask 077 && mkdir -p ~/privatecrates-secrets && cd ~/privatecrates-secrets
curl -fsS -X POST \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  https://api.github.com/app-manifests/<code>/conversions > dev-reader.json
jq -r .pem dev-reader.json > dev-reader.private-key.pem
jq '{id, slug, name, client_id, html_url}' dev-reader.json
```

The conversion needs no authentication (the code is the credential) and returns `id`, `slug`, `client_id`,
`client_secret`, `webhook_secret` and `pem` (the private key) [G2]. The key cannot be downloaded again; if it is lost,
generate a new one in the App's settings.

If GitHub gives an App a different slug than expected (for example because the name is taken), use the name it
accepted everywhere below: the slug goes into `READER_APP_SLUG` / `STORAGE_APP_SLUG`, and the storage App's bot login
(`<slug>[bot]`) is what `privatecrates-verify --storage-app` must be given.

### 4.2 Without the helper page

The same form works from any page: an HTML form with `method="post"`,
`action="https://github.com/organizations/worldbuilding-dev/settings/apps/new?state=<random>"` and one field named
`manifest` holding the JSON [G1]. Or create the App by hand under *Organisation settings → Developer settings →
GitHub Apps → New GitHub App* with the values from the JSON file.

### 4.3 Settings a manifest cannot express

Open each App at `https://github.com/organizations/worldbuilding-dev/settings/apps/<slug>`.

Reader App, *General*:
- Tick **Enable Device Flow** (the credential provider signs developers in with it; GitHub requires it to be enabled
  in the App's settings [G4]).
- Check **Expire user authorization tokens** is ticked (the default; user tokens then last 8 hours with a refresh
  token [G4]).
- Check **Request user authorization (OAuth) during installation** is not ticked.
- Callback URL is `https://{apex}/auth/github/callback`.

Both Apps, *General → Webhook*:
- **Webhook secret**: replace the generated one with this environment's `WEBHOOK_SECRET` (§3), because the server
  verifies both Apps' deliveries with one secret. Active, URL `https://{apex}/webhooks/github`.

Both Apps: *Where can this GitHub App be installed?* already shows **Any account** (`public: true`). Optionally
upload a logo under *Display information*.

## 5. Stripe

`scripts/stripe-setup.sh` creates, or updates if they exist: the product `privatecrates` ("PrivateCrates"), a
USD 100.00 monthly price with lookup key `privatecrates_org_monthly`, a webhook endpoint for
`https://{apex}/webhooks/stripe` with `checkout.session.completed` and `customer.subscription.created|updated|deleted`,
and the default customer portal configuration (cancel at period end, update payment method, invoice history). It
pins Stripe API version `2026-08-26.dahlia` for its calls and for the webhook payloads (override with
`STRIPE_API_VERSION`); keep that equal to the version the server's Stripe client sends (`Stripe-Version`, pinned in
`crates/privatecrates-server/src/billing.rs`).

Organisations with up to `FREE_MEMBER_LIMIT` members (default 5) never touch Stripe. Larger ones get a no-card trial
of `TRIAL_DAYS` (default 90), which the server creates directly through the API: a customer, then a subscription
with `trial_period_days`, `payment_settings[save_default_payment_method]=on_subscription` and
`trial_settings[end_behavior][missing_payment_method]=cancel`, so a trial that ends with no card is cancelled [S5].
The trial is set per subscription, not on the price. **Checkout is no longer used for new trials**: it is only for an
organisation that already had its trial, and then requires a card and has no trial. Cards are added in the customer
portal.

1. In the Stripe Dashboard, **test mode**: open *Settings → Billing → Customer portal*
   (`https://dashboard.stripe.com/test/settings/billing/portal`) and click **Save** once. Stripe creates the default
   portal configuration then; the API can update it but cannot make one the default [S4].
2. Run the script with a test-mode secret key (Developers → API keys):

   ```sh
   read -rs STRIPE_SECRET_KEY && export STRIPE_SECRET_KEY   # paste sk_test_…
   scripts/stripe-setup.sh dev
   ```

   It prints `STRIPE_PRICE_ID`, `STRIPE_WEBHOOK_SECRET` and the `railway variable set` commands. The webhook signing
   secret is returned only when the endpoint is created [S3]; on a re-run it is left unchanged (reveal it in the
   Dashboard, or pass `--recreate-webhook` and update Railway).
3. For production, repeat in **live mode** with an `sk_live_…` key once the Stripe account is activated:
   `scripts/stripe-setup.sh production`.
4. Turn on Stripe's trial-ending reminder email, in each mode: *Settings → Billing → Subscriptions and emails*
   (`https://dashboard.stripe.com/settings/billing/automatic`), under **Email notifications and customer
   management**, turn on **Send a reminder email 7 days before a free trial ends**, and set the email's payment link
   to **Link to a Stripe-hosted page** so the customer can add a card from the email [S6][S7]. The API cannot change
   this setting. Stripe sends no customer emails from a sandbox or test mode except to addresses of your verified
   email domain or team members [S7], and only to customers with an email address: the server creates trial
   customers with the organisation's name and metadata but no email, so an admin receives the reminder only once
   an email is on the customer (for example entered in the customer portal). Publishes in the last 14 days of a
   trial with no card carry a Cargo warning either way.

For the server's key you may use a restricted key (`rk_…`) instead of the secret key: it needs write access to
Customers, Subscriptions, Checkout Sessions and Customer portal, and read access to Prices and Products.

## 6. Railway

### 6.1 Project, service and environments

```sh
cd <your clone of worldbuilding-dev/privatecrates.dev>
railway init --name privatecrates                 # new project; its first environment is "production"
railway add --service privatecrates --repo worldbuilding-dev/privatecrates.dev
railway add --database postgres                   # the terms acceptances (§6.3); service "Postgres"
railway environment new dev --duplicate production
railway link                                      # choose privatecrates / dev / privatecrates
```

Then in the dashboard, for the `privatecrates` service, **in each environment** (the environment switcher is at the
top):

| Setting | dev | production |
|---|---|---|
| Settings → Source → Branch | `dev` | `main` |
| Settings → Source → Wait for CI | on | on |
| Settings → Deploy → Replicas | 1 | 1 |
| Settings → Deploy → Serverless (App Sleeping) | off | off |
| Settings → Region | US East (Virginia), close to GitHub's API | US East (Virginia) |

The build and deploy settings come from [`railway.json`](../railway.json) in the repository and override the
dashboard [R2]: Dockerfile builder, health check `GET /healthz` (up to 120 s; the server answers once tenant
discovery at start-up has finished), restart on failure (up to 10 times), 30 s draining, and watch patterns so that
changes to docs alone do not redeploy. Railway sets `PORT` and routes the domains to it; the image defaults to 8080.

Duplicating production gives dev its own Postgres, with its own data. In a project whose environments already exist,
run `railway add --database postgres` in each (`railway link` to the environment first). Put the Postgres service in
the same region as the server (US East (Virginia)) in both environments.

Connect production's source only once its variables are set (§6.2), or its first deploy fails to start. The
Railway CLI's `service source connect` sets the source for every environment at once; set per-environment branches
in the dashboard.

### 6.2 Set the variables

Set the variables in §7 for each environment. With the CLI, `--stdin` keeps values out of the process list and shell
history, and `--skip-deploys` avoids a deploy per variable [R4]. For dev:

```sh
E=dev S=privatecrates D=~/privatecrates-secrets
railway variable set -e $E -s $S --skip-deploys BASE_DOMAIN=dev.privatecrates.dev
# A reference to the environment's own Postgres service, resolved by Railway (single quotes: no shell expansion)
railway variable set -e $E -s $S --skip-deploys 'DATABASE_URL=${{Postgres.DATABASE_URL}}'
railway variable set -e $E -s $S --skip-deploys REGISTRY_TOKEN_SECRET --stdin < $D/$E/REGISTRY_TOKEN_SECRET
railway variable set -e $E -s $S --skip-deploys SESSION_SECRET        --stdin < $D/$E/SESSION_SECRET
railway variable set -e $E -s $S --skip-deploys WEBHOOK_SECRET        --stdin < $D/$E/WEBHOOK_SECRET

# From the manifest conversions (§4.1)
R=$D/dev-reader.json T=$D/dev-storage.json
railway variable set -e $E -s $S --skip-deploys READER_APP_ID="$(jq -r .id $R)" READER_APP_SLUG="$(jq -r .slug $R)" READER_APP_CLIENT_ID="$(jq -r .client_id $R)"
jq -r .client_secret $R | tr -d '\n' | railway variable set -e $E -s $S --skip-deploys READER_APP_CLIENT_SECRET --stdin
jq -r .pem $R           | railway variable set -e $E -s $S --skip-deploys READER_APP_PRIVATE_KEY --stdin
railway variable set -e $E -s $S --skip-deploys STORAGE_APP_ID="$(jq -r .id $T)" STORAGE_APP_SLUG="$(jq -r .slug $T)"
jq -r .pem $T           | railway variable set -e $E -s $S --skip-deploys STORAGE_APP_PRIVATE_KEY --stdin

# From scripts/stripe-setup.sh (§5)
railway variable set -e $E -s $S --skip-deploys STRIPE_PRICE_ID=<price_…>
printf '%s' '<whsec_…>'        | railway variable set -e $E -s $S --skip-deploys STRIPE_WEBHOOK_SECRET --stdin
printf '%s' "$STRIPE_SECRET_KEY" | railway variable set -e $E -s $S STRIPE_SECRET_KEY --stdin   # this one deploys
```

For production, repeat with `E=production`, `BASE_DOMAIN=privatecrates.dev`, the `prod-*.json` conversions and the
live-mode Stripe values. Check with `railway variable list -e $E -s $S` (it prints values: mind your screen).

Once the variables are in, delete the local copies of the private keys and client secrets, or move them into the
password manager.

### 6.3 The database and its backups

The server runs its migrations (embedded in the binary, `crates/privatecrates-server/migrations`) at start-up, so
there is nothing to run by hand. `DATABASE_URL` is the private-network URL, so the server's queries never leave
Railway. The only table, `terms_acceptances`, is append-only (§1): nobody should update or delete its rows, not even
by hand, except to honour a legal obligation, and then with a note of why.

Backups, in each environment's Postgres service → **Backups** [R7][R8]:

- Turn on the **Daily** (kept 6 days), **Weekly** (kept about a month) and **Monthly** (kept about 3 months)
  schedules; several can run at once, and a backup can also be taken by hand, for instance before a migration [R7].
  They are incremental, copy-on-write snapshots, billed like volume storage for the data they alone hold [R7].
- A backup restores only into the same project and environment, restoring removes the backups newer than it, and
  **wiping the volume deletes every backup** [R7][R8]. So in production, also keep an offsite logical dump from time
  to time (the only copy that survives deleting the project [R8]), in the password manager's secure file storage:

  ```sh
  railway link                                   # privatecrates / production / Postgres
  railway connect postgres --tunnel-only         # prints a local connection; in another terminal:
  pg_dump "<the tunnel's URL>" --format=custom --no-owner --file=terms-$(date +%F).dump
  ```

- Try a restore into a scratch database once, so that it is known to work before it is needed [R8].

## 7. Environment variables

`dev` / `production` values; *secret* means treat as a credential (Railway stores all variables encrypted, but
secrets must never be logged, committed or shared).

| Variable | Secret | dev | production | Purpose, source |
|---|---|---|---|---|
| `BASE_DOMAIN` | no | `dev.privatecrates.dev` | `privatecrates.dev` | Apex host; tenants are `{slug}.BASE_DOMAIN`. |
| `DATABASE_URL` | **yes** | `${{Postgres.DATABASE_URL}}` | `${{Postgres.DATABASE_URL}}` | §6.3. A Railway reference to the environment's Postgres; the terms acceptances. Required: the server refuses to start without it (except for local development over `http`). |
| `REGISTRY_TOKEN_SECRET` | **yes** | generated | generated | §3. Signs `pcr_` tokens; ≥ 32 bytes. |
| `SESSION_SECRET` | **yes** | generated | generated | §3. Encrypts the `pc_session` cookie. |
| `WEBHOOK_SECRET` | **yes** | generated | generated | §3. GitHub webhook signatures; set on both Apps. |
| `READER_APP_ID` | no | conversion `id` | conversion `id` | Reader App ID. |
| `READER_APP_SLUG` | no | `privatecrates-dev-reader` | `privatecrates-reader` | Conversion `slug`; builds `https://github.com/apps/{slug}/installations/new`. |
| `READER_APP_CLIENT_ID` | no | conversion `client_id` | conversion `client_id` | Web sign-in and the credential provider's device flow. |
| `READER_APP_CLIENT_SECRET` | **yes** | conversion `client_secret` | conversion `client_secret` | Web sign-in code exchange. |
| `READER_APP_PRIVATE_KEY` | **yes** | conversion `pem` | conversion `pem` | PEM, multi-line; App JWTs. |
| `STORAGE_APP_ID` | no | conversion `id` | conversion `id` | Storage App ID. |
| `STORAGE_APP_SLUG` | no | `privatecrates-dev-storage` | `privatecrates-storage` | Conversion `slug`; install link in onboarding. |
| `STORAGE_APP_PRIVATE_KEY` | **yes** | conversion `pem` | conversion `pem` | PEM, multi-line; App JWTs. |
| `STRIPE_SECRET_KEY` | **yes** | `sk_test_…` / `rk_test_…` | `sk_live_…` / `rk_live_…` | Stripe API key (§5). |
| `STRIPE_WEBHOOK_SECRET` | **yes** | `whsec_…` (test endpoint) | `whsec_…` (live endpoint) | From `stripe-setup.sh`. |
| `STRIPE_PRICE_ID` | no | `price_…` (test) | `price_…` (live) | From `stripe-setup.sh`. |

Leave these unset (the defaults are right for both environments):

| Variable | Default | Notes |
|---|---|---|
| `PORT` | set by Railway (image default 8080) | The server binds `0.0.0.0:$PORT`. |
| `WEBSITE_DIR` | `/app/website` (set in the image) | The static website build. |
| `PUBLIC_SCHEME` | `https` | `http` only for local testing. |
| `GITHUB_API_URL`, `GITHUB_WEB_URL` | `https://api.github.com`, `https://github.com` | Tests point these at a fake. |
| `STRIPE_API_URL` | Stripe's API | Tests point this at a fake. |
| `PREVIEW` | `true` | The preview (docs/preview.md): free for everyone, billing off whatever Stripe configuration is set. `false` from general availability. |
| `TELEMETRY_CONFIG` | built in | A YAML file for apollo-opentelemetry that replaces the built-in configuration (`crates/privatecrates-server/src/telemetry.yaml`: JSON logs to standard output, at most 20 a second). Use it to export logs, spans and metrics over OTLP. |
| `GRAFANA_CLOUD_INSTANCE_ID`, `GRAFANA_CLOUD_API_KEY`, `GRAFANA_CLOUD_OTLP_ENDPOINT` | none | With the API key set (a sealed variable), logs, request spans and metrics also go to Grafana Cloud (`telemetry-grafana-cloud.yaml`). The endpoint is the stack's OTLP gateway, e.g. `https://otlp-gateway-prod-eu-west-2.grafana.net/otlp`. |
| `RUST_LOG` | `info` | Which logs are kept, by level and module, e.g. `info,privatecrates_server=debug`. |
| `FREE_MEMBER_LIMIT` | 5 | Organisations with at most this many members (active members, from the reader App; not outside collaborators or pending invitations) are free. |
| `TRIAL_DAYS` | 90 | Length of the no-card trial larger organisations get once. |
| `OIDC_ISSUER`, `OIDC_JWKS_URL` | GitHub Actions' | |
| `CRATES_IO_API_URL` | `https://crates.io` | |
| `MAX_CRATE_BYTES` | 20 MiB | |
| `PUBLISH_RATE_PER_MINUTE` | 30 | Per token. |
| `PERMISSION_TTL_SECS`, `TENANT_REFRESH_SECS`, `STORAGE_REFRESH_SECS` | 300, 600, 60 | Cache lifetimes (SPEC §8). |
| `RUST_LOG` | `info,tower_http=info` (set in the image) | e.g. `debug` while investigating. |

## 8. Custom domains and Cloudflare DNS

### 8.1 Add the domains in Railway

```sh
railway domain dev.privatecrates.dev   -e dev        -s privatecrates
railway domain '*.dev.privatecrates.dev' -e dev      -s privatecrates

railway domain privatecrates.dev       -e production -s privatecrates
railway domain www.privatecrates.dev   -e production -s privatecrates
railway domain '*.privatecrates.dev'   -e production -s privatecrates
```

(Or *Settings → Networking → Custom Domain* in the dashboard.) Each command prints the records to create: a CNAME to
a Railway target such as `<random>.up.railway.app`, a TXT verification record, and for wildcards a second CNAME for
`_acme-challenge` [R1][R5]. Both the CNAME and the TXT record are required; the domain does not verify with the CNAME
alone [R1].

### 8.2 Create the records in Cloudflare

In Cloudflare → `privatecrates.dev` → *DNS → Records*. **Every record: Proxy status = DNS only (grey cloud).** Names
are relative to the zone; the targets are what Railway printed.

| Type | Name | Target | For |
|---|---|---|---|
| CNAME | `@` | `<prod-apex>.up.railway.app` | `privatecrates.dev` (Cloudflare flattens a CNAME at the apex [R1]) |
| CNAME | `www` | `<prod-www>.up.railway.app` | `www.privatecrates.dev` |
| CNAME | `*` | `<prod-wildcard>.up.railway.app` | `*.privatecrates.dev` |
| CNAME | `_acme-challenge` | `authorize.railwaydns.net` (as printed) | certificate for `*.privatecrates.dev` |
| TXT | as printed by Railway | as printed | production domain verification (one per domain) |
| CNAME | `dev` | `<dev-apex>.up.railway.app` | `dev.privatecrates.dev` |
| CNAME | `*.dev` | `<dev-wildcard>.up.railway.app` | `*.dev.privatecrates.dev` |
| CNAME | `_acme-challenge.dev` | `authorize.railwaydns.net` (as printed) | certificate for `*.dev.privatecrates.dev` |
| TXT | as printed by Railway | as printed | dev domain verification (one per domain) |

Notes:
- The explicit `dev` records take precedence over the `*` wildcard in DNS, and `*.privatecrates.dev` does not match
  `x.dev.privatecrates.dev`, so the two environments never overlap. `dev` and `www` are reserved slugs, so no tenant
  can claim them.
- SSL/TLS settings in Cloudflare do not apply to DNS-only records; nothing to change there.
- If you add CAA records to the zone, they must allow the CA Railway uses for both `issue` and `issuewild`
  (Let's Encrypt at the time of writing [R5]), or wildcard issuance fails.

Railway shows each domain as verified, then issues its certificate; wildcard certificates can take a few minutes
after the `_acme-challenge` record resolves. Until then Railway serves its own `*.up.railway.app` certificate and
every registry host fails TLS. If the domain shows "Failed to issue TLS certificate" (seen on production's first
deploy, 28 September 2026), press **Try Again** in *Settings → Networking*; the new certificate reaches all of
Railway's edge servers within a minute or two. Check:

```sh
dig +short CNAME _acme-challenge.dev.privatecrates.dev
dig +short CNAME anything.dev.privatecrates.dev
echo | openssl s_client -connect dev.privatecrates.dev:443 -servername probe.dev.privatecrates.dev 2>/dev/null \
  | openssl x509 -noout -subject -ext subjectAltName
```

## 9. Verify a deployment

Run for `A=https://dev.privatecrates.dev` (then `A=https://privatecrates.dev`):

```sh
A=https://dev.privatecrates.dev
curl -fsS $A/healthz; echo                                   # "ok"; no GitHub calls
curl -fsS $A/api/session                                     # {"user":null,"orgs":[]}
curl -sS -o /dev/null -w '%{http_code}\n' $A/                # 200, the website
curl -sS -o /dev/null -w '%{http_code}\n' -X POST $A/webhooks/github -d '{}'   # 4xx: unsigned delivery refused
curl -sS -o /dev/null -w '%{http_code}\n' -X POST $A/webhooks/stripe -d '{}'   # 4xx: unsigned delivery refused
curl -sS -o /dev/null -w '%{http_code}\n' https://nosuchtenant.dev.privatecrates.dev/index/config.json  # 401, as for a real registry (SPEC §6.7)
echo | openssl s_client -connect dev.privatecrates.dev:443 -servername probe.dev.privatecrates.dev 2>/dev/null \
  | openssl x509 -noout -subject                             # CN=*.dev.privatecrates.dev, not *.up.railway.app
curl -sSI https://www.privatecrates.dev | grep -i '^location'  # production only: redirect to the apex
railway logs -e dev -s privatecrates                         # JSON logs; "tenants discovered", no errors
```

Webhooks:
- GitHub: each App's *Advanced → Recent Deliveries* shows the `ping` sent at creation (it failed if the service was
  not up yet: press **Redeliver**; expect 2xx).
- Stripe: Dashboard → *Developers → Webhooks* → the endpoint → **Send test event**
  (`customer.subscription.updated`); expect 2xx.

Sign-in and onboarding, with the test organisation (dev first; in production use a real card and cancel afterwards,
or a 100% coupon):

1. Open `$A/account`, **Sign in with GitHub**; authorise the reader App. The account page lists your organisations.
2. Choose the test organisation. Follow the checklist:
   1. install the reader App on all repositories;
   2. create a private storage repository (e.g. `privatecrates-test/crates-store`) and enable **immutable
      releases** in its *Settings → General → Releases*;
   3. install the storage App on **only** that repository;
   4. choose the registry name (slug); this creates `privatecrates.toml` in the storage repository (check the
      commit is by the storage App and verified);
   5. the plan: with at most `FREE_MEMBER_LIMIT` members the step is already done ("Free: 3 of 5 members"). To
      test billing on dev, set `FREE_MEMBER_LIMIT=0` for the dev service, then **Start free trial**: no card is
      asked for, and the status becomes *trialing* at once. Add the test card `4242 4242 4242 4242` (any future
      expiry and CVC) in the billing portal.
3. The registry answers:

   ```sh
   curl -fsS -H "Authorization: $(gh auth token)" https://<slug>.dev.privatecrates.dev/index/config.json
   ```

4. With the credential provider (built from this repository: `cargo install --path crates/cargo-credential-privatecrates`),
   in a scratch project:

   ```toml
   # .cargo/config.toml
   [registries.test]
   index = "sparse+https://<slug>.dev.privatecrates.dev/index/"
   credential-provider = ["cargo-credential-privatecrates"]
   ```

   `cargo login --registry test` runs the device flow; `cargo search --registry test hello` then succeeds.
   Publishing uses a GitHub Actions workflow in a repository of the test organisation (SPEC §3.4).
5. Billing: *Manage billing* opens the Stripe portal; cancel the subscription and check the account page shows it
   ending at the period end.
6. Remove a member from the test organisation and check they lose access at once (the `organization` webhook).

## 10. Promote to production

After a commit has run in dev and CI is green on it:

```sh
git fetch origin
git push origin origin/dev:main      # fast-forward only; fails if main has diverged
```

Protect `main` in *GitHub → Settings → Rules*: restrict updates to maintainers, block force pushes and deletion,
and require the CI status checks. Railway deploys production once CI has passed on the commit (Wait for CI). Then
run §9 against `https://privatecrates.dev`.

To promote an older commit: `git push origin <sha>:main` (must still be a fast-forward).

## 11. Roll back

The service is stateless (all durable state is on GitHub and Stripe), so rolling back is always safe.

- **Fastest:** Railway dashboard → the service → *Deployments* → the last good deployment → **⋮ → Rollback**. This
  restores that deployment's image **and its variables** [R3]; deployments older than the plan's retention window
  cannot be rolled back.
- **Then make git agree**, or the next push redeploys the bad commit: revert on `main`, and promote the revert
  (`git revert <bad-sha>` on `main`, push, then §10). Avoid force-pushing `production`.
- A bad variable: fix it with `railway variable set …`; Railway redeploys.
- A deployment that fails its health check never receives traffic; the previous one keeps serving.

Railway keeps the previous deployment serving until the new one passes `/healthz`, so a failed deploy needs no
action beyond fixing it.

## 12. Rotate secrets

| Secret | How | Effect |
|---|---|---|
| `REGISTRY_TOKEN_SECRET` | new value, set, deploy | outstanding `pcr_` tokens stop working; CI jobs re-exchange |
| `SESSION_SECRET` | new value, set, deploy | everyone is signed out of the website |
| `WEBHOOK_SECRET` | set the new value on both Apps and in Railway, close together | deliveries in between fail and are redelivered from *Recent Deliveries* |
| App private key | App settings → *Generate a private key*; set; deploy; then delete the old key in the App settings | none |
| `READER_APP_CLIENT_SECRET` | App settings → *Generate a new client secret*; set; deploy; delete the old one | none |
| `STRIPE_WEBHOOK_SECRET` | Dashboard → webhook → *Roll secret* (Stripe keeps the old one valid for a period you choose) | none |
| `STRIPE_SECRET_KEY` | Dashboard → API keys → *Roll key* | none if set before the old key expires |

## 13. Release the client tools

`cargo-credential-privatecrates`, `cargo-privatecrates` and `privatecrates-verify` (with `privatecrates-common` and
`privatecrates-auth`) are released by
[`.github/workflows/release.yml`](../.github/workflows/release.yml) when a tag `vX.Y.Z` is pushed: binaries for Linux
(x86_64, aarch64), macOS (aarch64, x86_64) and Windows (x86_64) with GitHub build-provenance attestations, a GitHub
release with the archives and `SHA256SUMS` (where `cargo binstall` finds them), then crates.io.

One-time setup:

1. **crates.io requires the first version of each crate to be published by hand**; trusted publishing can only be
   configured for a crate that exists [C1][C2]. From a clean checkout of the release commit, with a crates.io API
   token that has the `publish-new` scope (crates.io → Account Settings → API Tokens; expire it after a day):

   ```sh
   cargo login                     # paste the token
   cargo publish --locked -p privatecrates-common -p privatecrates-auth -p cargo-credential-privatecrates \
     -p cargo-privatecrates -p privatecrates-verify
   cargo logout
   ```

   Then push the tag `v0.1.0`; the workflow builds the binaries and release and skips the crates already on crates.io.
2. On crates.io, for **each** of the five crates, *Settings → Trusted Publishing → Add*: GitHub, repository owner
   `worldbuilding-dev`, repository `privatecrates.dev`, workflow `release.yml`, environment `crates-io` [C1].
3. In GitHub → repository *Settings → Environments*, create `crates-io`; optionally add required reviewers (a manual
   approval before anything is published) and restrict it to tags `v*`.
4. Revoke the API token; from now on no crates.io secret exists anywhere.

Each release:

```sh
# bump version = "X.Y.Z" in crates/privatecrates-common, privatecrates-auth, cargo-credential-privatecrates,
# cargo-privatecrates and privatecrates-verify (and the shared crates' versions in the others' [dependencies]),
# and RELEASE_VERSION in website/src/lib/snippets.ts; commit on main; CI green
git tag -s vX.Y.Z -m vX.Y.Z && git push origin vX.Y.Z
```

The workflows the server and website hand out (publish, CI, verifier) install the binaries of their own version
from its release, so tag the release as soon as the bump reaches `main`: until its binaries are published, those
workflows fail at the install step. The workflow refuses a tag that does not match the five crate versions. Verify an artefact with
`gh attestation verify <file> --repo worldbuilding-dev/privatecrates.dev`.

## 14. Status page

`https://status.privatecrates.dev` is a Cloudflare Worker in `status/` (docs/trust-and-status.md §1), deliberately
not on Railway so it stays up when we are down. Every minute a Cron Trigger probes both environments (`/healthz`,
`/api/status`, and the canary registry's `/index/config.json`, which must answer `401` with `WWW-Authenticate`) and
reads GitHub's and Stripe's status pages [W6]; it keeps 90 days of per-minute results and per-day summaries in D1. `/` is
production, `/dev` is dev; each has `/feed.xml` (incidents, Atom) and `/api/summary.json`. It also pages the owner
when something of ours breaks (§14.5). It is deployed by hand, not by Railway or CI.

You need a Cloudflare account with the `privatecrates.dev` zone (already there for DNS), Node 24 and pnpm (via
corepack). Wrangler is a dev dependency; log in once with `pnpm exec wrangler login` (or set `CLOUDFLARE_API_TOKEN`
to a token with *Workers Scripts: Edit*, *D1: Edit*, *Workers Routes: Edit* and, for the zone, *DNS: Edit*).

### 14.1 The canary registry

Each environment's canary is a registry slug on that apex (`canary` in `status/wrangler.jsonc` under `vars`,
`ENVIRONMENTS[].canary`). Since every name answers like a registry without access (SPEC §6.7), any slug answers 401,
so the probe proves the server, DNS, TLS and tenant routing, not that a particular registry works; a broken App key
shows instead as GitHub errors in `/api/status`, which the page reads. Prefer a real registry we own (dev uses
`uxlint-net`), so that the probe will catch more if it ever authenticates. Check:

```sh
curl -si https://canary.dev.privatecrates.dev/index/config.json | grep -Ei '^(HTTP|www-authenticate)'
curl -si https://canary.privatecrates.dev/index/config.json     | grep -Ei '^(HTTP|www-authenticate)'
# HTTP/2 401 and a www-authenticate header, for each
```

### 14.2 First deploy

```sh
cd status
pnpm install --frozen-lockfile

# 1. The database. Prints a database_id: paste it into wrangler.jsonc (d1_databases[0].database_id) and commit it
#    (it is an identifier, not a secret).
pnpm exec wrangler d1 create privatecrates-status        # [W1][W5]

# 2. Its tables (every file in migrations/: the probe history, and the alerting state).
pnpm exec wrangler d1 migrations apply privatecrates-status --remote

# 3. Test, then deploy. `deploy` runs the build (incidents, fonts), uploads the Worker and its static assets, sets
#    the every-minute Cron Trigger, and creates the status.privatecrates.dev Custom Domain with its DNS record and
#    certificate.
pnpm test
pnpm exec wrangler deploy
```

**DNS.** Do not create a record for `status` yourself: the Custom Domain (`routes` with `custom_domain: true` in
`wrangler.jsonc`) makes it, and refuses if a CNAME for `status` already exists, so delete any such record first
[W3]. The record it creates is **proxied** (orange cloud), which Workers Custom Domains require; that is fine for
this host, unlike the Railway hosts in §8, because no tokens or customer data ever reach it. The explicit `status`
record takes precedence over the `*` wildcard to Railway, and `status` is a reserved slug, so no tenant can claim
it. A Custom Domain matches the whole host (every path), so it is the equivalent of the route
`status.privatecrates.dev/*`; it takes no path pattern [W3].

The first check runs within a minute (new Cron Triggers can take up to 15 minutes to reach the whole network [W4]).
Check:

```sh
curl -s https://status.privatecrates.dev/api/summary.json | jq '{checked_at, state}'
curl -s https://status.privatecrates.dev/dev/api/summary.json | jq '.components[] | {id, state, cause}'
pnpm exec wrangler tail privatecrates-status        # live logs, including the scheduled runs
```

To try it locally first: `pnpm exec wrangler d1 migrations apply privatecrates-status --local`, then `pnpm dev`, and
trigger a run with `curl 'http://localhost:8787/__scheduled?cron=*+*+*+*+*'` (it probes the real environments).

### 14.3 Post or update an incident

Incidents are Markdown files in `status/incidents/`, deployed with the Worker; `status/incidents/README.md` has the
format and `_example.md` every field.

```sh
cd status
cp incidents/_example.md incidents/2026-10-02-publish-errors.md   # then edit: remove `example: true`
pnpm build && pnpm test                                             # a mistake in the file fails here
pnpm exec wrangler deploy
git add incidents/2026-10-02-publish-errors.md && git commit -m "Status: publish errors" && git push
```

For each update, add an entry under `updates` (and `end` once it is over) and deploy again. While an incident has no
`end`, its components are shown as at least its `impact`, whatever the probes say.

### 14.4 Changes and rollback

Change the probed environments, the canary slug or the upstream URLs in `vars` in `wrangler.jsonc` and deploy.
Schema changes go in a new `migrations/000N_*.sql`, applied with the `migrations apply --remote` command above
before deploying. `pnpm exec wrangler rollback` returns to the previous version; `pnpm exec wrangler deployments
list` shows them. CI runs the tests and `wrangler deploy --dry-run` on every change under `status/`.

Storage is D1, not KV: every minute the Worker adds a row per environment and updates that day's summary, and prunes
rows older than 90 days by range; D1 does that with one SQL statement each and is strongly consistent, while KV
allows one write per second per key, is eventually consistent (a read-modify-write of a day's summary could lose
minutes) and cannot delete by range [W2][W5]. At about 8,600 rows written a day, it is well inside D1's free
allowance of 100,000 a day [W5]; KV's free plan allows 1,000 writes a day [W2].

**Stripe's status.** `STRIPE_STATUS_URL` is Statuspage's `/api/v2/summary.json` as planned, but
`status.stripe.com` is not a Statuspage site and answers 404 there (September 2026); its older `/current` JSON has
not changed since February 2024. The page therefore shows Stripe's status as "could not be read automatically"
with a link, and Billing is judged from the server's own Stripe error rate in `/api/status`. If Stripe publishes a
Statuspage-compatible API, point the var at it.

### 14.5 Alerts: ntfy and email

The status Worker pages the owner (the only responder). Urgent pages go to the **ntfy** phone app; **email** gets
every page and notification as a second channel, with the same text. The logic is `status/src/alerting.ts`; the
channels are `status/src/notify.ts`.

**What pages, and what doesn't.** After each minute's checks, for each environment and component:

| Situation | Message | ntfy priority |
|---|---|---|
| One of *ours* degraded or down for **2 consecutive runs** (our probes fail, or errors calling GitHub or Stripe that they have not reported) | Page: what, whose fault, since when, why | production **5** (urgent); dev 3 |
| It gets worse (degraded to down) | Page | production 5; dev 3 |
| It gets better (down to degraded) | Update | 3 |
| Still ours and still broken, in production | Reminder, every **30 minutes** | 5 |
| It recovers (the first good run) | Resolved, with how long it lasted | 3 |
| A problem attributed to **GitHub or Stripe** (e.g. a GitHub Actions incident), for 2 runs | Notice: "Publishing degraded: GitHub Actions is having an incident: … (link). Nothing to fix on our side." | 2 (no sound) |
| That upstream problem ends | Notice | 2 |

It does **not** page for a problem seen in only one run; for GitHub trouble that is not reaching us (the page's
"GitHub degraded but we're fine" note); for a component raised only by an incident we wrote ourselves
(`status/incidents/`); or more than once per component in **10 minutes** (flap protection: a change held back is sent
on the first run after the window if it still holds; recoveries are never held back). Dev is never urgent and has
no reminders. Everything that changes in one environment in one run is **one** message ("Production: Registry reads,
Downloads and Publishing down"), with a link to that environment's status page. What was last said, per environment
and component, is kept in D1 (`migrations/0002_alerts.sql`) and saved before sending, so a restart, a retried cron run
or two overlapping runs never page twice. If ntfy or email fails, the Worker logs it (`wrangler tail`) and carries
on; the probes and the page never depend on it. With neither channel configured, alerting is silently off (as in
`wrangler dev`).

**1. ntfy (the phone app)** [N1][N2]. Install ntfy from Google Play, F-Droid or the App Store. Make up a long random
topic name: on ntfy.sh anyone who knows it can read and post to it, so it is the credential.

```sh
openssl rand -hex 16 | sed 's/^/privatecrates-/'     # e.g. privatecrates-3f9c…; keep it in the password manager
```

In the app, tap **+**, enter the topic (server `https://ntfy.sh`), and subscribe. On **Android**, turn on *Instant
delivery* for the subscription, then in the app's notification settings open the *Max priority* channel and allow
it to **override Do Not Disturb** [N2]. On **iOS**, the app does not yet mark urgent messages as critical or time
sensitive (ntfy issue #1235) [N4], so allow the ntfy app in the Do Not Disturb Focus (*Settings → Focus → Do Not
Disturb → Apps → Add ntfy*); that lets every ntfy message through, including the priority 2 notices. Then:

```sh
cd status
pnpm exec wrangler secret put NTFY_TOPIC     # paste the topic name
```

`NTFY_URL` (a var in `wrangler.jsonc`, default `https://ntfy.sh`) points at another server; for a self-hosted server
or a reserved topic on a paid ntfy.sh account, also `pnpm exec wrangler secret put NTFY_TOKEN` with an access token
(`tk_…`), sent as `Authorization: Bearer` [N1].

**2. Email (Cloudflare Email Routing)** [W7][W8][W9]. In the Cloudflare dashboard, for the `privatecrates.dev` zone:
*Email → Email Routing*, enable it (it adds its MX and SPF records), then under *Destination addresses* add the
owner's address and click the link in the verification email. The Worker sends as `alerts@privatecrates.dev`
(`ALERT_EMAIL_FROM`) through the `send_email` binding `ALERT_EMAIL` in `wrangler.jsonc`, which may only send from
that address; with Email Routing alone it can only send to verified destination addresses, and those sends are free
[W8]. Put the verified address in `wrangler.jsonc`:

```jsonc
"ALERT_EMAIL_TO": "owner@example.com"
```

and deploy (`pnpm exec wrangler deploy`). Subjects read `[PrivateCrates production] Publishing down`, and
`[resolved] [PrivateCrates production] Publishing recovered after 12 min`. The message is a small hand-built RFC 5322
plain-text email with a `Message-ID`, `Date` and `Auto-Submitted: auto-generated` header, passed to `EmailMessage`
from `cloudflare:email` [W7].

**3. Test it.** After deploying:

```sh
cd status
pnpm alert:test                                   # sets the ALERT_TEST secret to a fresh value
pnpm exec wrangler tail privatecrates-status      # "alert test: ntfy sent, email sent" within a minute or so
```

The next cron run sees an `ALERT_TEST` value it has not sent before, records it in D1 and sends one **urgent test
page** (priority 5, like a production outage) and one email, through the real bindings. Try it with the phone on
Do Not Disturb. It cannot be triggered over HTTP; only someone who can deploy the Worker can set a secret. Each value
is sent once; remove it afterwards with `pnpm exec wrangler secret delete ALERT_TEST`.

**Limits.** ntfy.sh limits publishing per client IP [N3]; Workers share outgoing IPs, so an unlucky 429 is possible.
It is logged, and the email still goes. A reserved topic with a token, or a self-hosted server, avoids it. Alerting
adds one D1 row written per environment per minute, plus a few when something changes (about 2,900 a day).

### 14.6 Railway notifications

The status page sees outages from outside; Railway knows about failed builds and crashes first. There is no
per-project email toggle to switch on (September 2026): Railway emails the project's members, at their account
address, when a deployment **crashes** after reaching its restart limit [R3], and reports a **failed** build or deploy
(and crashes) through project webhooks [R7]. So:

- make sure the owner is a member of the Railway project and that mail from Railway is not filtered
  away;
- for failed deploys, add a webhook under *Project → Settings → Webhooks* for *Failed* and *Crashed* deployments
  [R7], pointing at something that emails or pushes (a Slack or Discord channel, or a small receiver);
- a process that dies after a successful deploy and is restarted sends nothing [R8]; the status page pages for that
  if it lasts two minutes.

## 15. Sources

Consulted in September 2026.

- [R1] Railway, *Working with Domains* (custom domains, wildcard domains, Cloudflare, plan limits):
  <https://docs.railway.com/networking/domains/working-with-domains>
- [R2] Railway, *Config as Code*: <https://docs.railway.com/reference/config-as-code>
- [R3] Railway, *GitHub Autodeploys* (trigger branch, Wait for CI) and *Deployment actions* (rollback):
  <https://docs.railway.com/deployments/github-autodeploys>, <https://docs.railway.com/deployments/deployment-actions>
- [R4] Railway CLI: <https://docs.railway.com/cli/variable>, <https://docs.railway.com/cli/domain>,
  <https://docs.railway.com/cli/environment>, <https://docs.railway.com/cli/add>
- [R5] Railway Help Station, wildcard certificates with Cloudflare (`_acme-challenge` DNS only, CAA for Let's Encrypt):
  <https://station.railway.com/questions/wildcard-domain-ratioiq-app-525-ssl-ha-951f62cd>,
  <https://station.railway.com/questions/wildcard-custom-domain-brimwise-com-fa-4721f68b>,
  and *Troubleshooting SSL*: <https://docs.railway.com/networking/troubleshooting/ssl>
- [R6] Cloudflare, Universal SSL covers the apex and one level of subdomain; Advanced Certificate Manager for deeper
  names: <https://developers.cloudflare.com/ssl/edge-certificates/universal-ssl/limitations/>
- [R7] Railway, *Backups* (schedules and retention, restore, same project and environment, wiping deletes backups,
  incremental billing): <https://docs.railway.com/volumes/backups>
- [R8] Railway, *Back Up and Restore Postgres* (volume backups, point-in-time recovery, `pg_dump`, restore drills):
  <https://docs.railway.com/guides/postgres-backups-restores>
- [G1] GitHub, *Registering a GitHub App from a manifest*:
  <https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest>
- [G2] GitHub REST, *Create a GitHub App from a manifest*:
  <https://docs.github.com/en/rest/apps/apps#create-a-github-app-from-a-manifest>
- [G3] GitHub, *Webhook events and payloads* (availability and required permissions per event):
  <https://docs.github.com/en/webhooks/webhook-events-and-payloads>
- [G4] GitHub, *Generating a user access token for a GitHub App* (device flow must be enabled in settings; expiring
  tokens): <https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app>
- [S1] Stripe API: products <https://docs.stripe.com/api/products/create>, prices
  <https://docs.stripe.com/api/prices/create>
- [S2] Stripe, API versions: <https://docs.stripe.com/upgrades>
- [S3] Stripe API, *Create a webhook endpoint* (secret returned on creation, `api_version`):
  <https://docs.stripe.com/api/webhook_endpoints/create>
- [S4] Stripe API, customer portal configurations and sessions (default configuration):
  <https://docs.stripe.com/api/customer_portal/configurations/create>,
  <https://docs.stripe.com/api/customer_portal/sessions/create>
- [S5] Stripe, *Use free trial periods on subscriptions* (trials without a payment method,
  `missing_payment_method=cancel`, which defaults Stripe checks): <https://docs.stripe.com/billing/subscriptions/trials/free-trials>
- [S6] Stripe, *Manage compliance requirements for trials and promotions* (trial-end reminder emails, 7 days before):
  <https://docs.stripe.com/billing/subscriptions/trials/manage-trial-compliance>
- [S7] Stripe, *Automate customer emails* (Trial ending reminders; Link to a Stripe-hosted page; emails in a sandbox):
  <https://docs.stripe.com/billing/revenue-recovery/customer-emails>
- [C1] crates.io, *Trusted Publishing*: <https://crates.io/docs/trusted-publishing>, and
  `rust-lang/crates-io-auth-action`: <https://github.com/rust-lang/crates-io-auth-action>
- [C2] RFC 3691, *Trusted Publishing for crates.io*:
  <https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html>
- [W1] Cloudflare, *Wrangler configuration* (`wrangler.jsonc` recommended for new projects; `triggers.crons`,
  `d1_databases`, `routes` with `custom_domain`, `vars`, `assets`, `build`):
  <https://developers.cloudflare.com/workers/wrangler/configuration/>
- [W2] Cloudflare, *Workers KV limits* (1 write per second per key; 1,000 writes a day on the Free plan):
  <https://developers.cloudflare.com/kv/platform/limits/>
- [W3] Cloudflare, *Custom Domains* (DNS records and certificate created for you; no wildcard or path; not on a host
  with an existing CNAME): <https://developers.cloudflare.com/workers/configuration/routing/custom-domains/>
- [W4] Cloudflare, *Cron Triggers* (UTC; up to 15 minutes to propagate; local testing):
  <https://developers.cloudflare.com/workers/configuration/cron-triggers/>
- [W5] Cloudflare, *D1 limits* and *D1 pricing* (rows written: 100,000 a day Free, 50 million a month Paid):
  <https://developers.cloudflare.com/d1/platform/limits/>, <https://developers.cloudflare.com/d1/platform/pricing/>
- [W6] Statuspage public API used by GitHub (`/api/v2/summary.json`): <https://www.githubstatus.com/api>
- [W7] Cloudflare, *Send emails from Workers* (the `send_email` binding; `EmailMessage` from `cloudflare:email` with a
  raw MIME message; also a newer structured `send({ to, from, subject, text })`):
  <https://developers.cloudflare.com/email-routing/email-workers/send-email-workers/>
- [W8] Cloudflare, *Configure send bindings* (`allowed_sender_addresses`, `destination_address`,
  `allowed_destination_addresses`; without a sending domain, only verified destination addresses, and those sends
  are free): <https://developers.cloudflare.com/email-service/configuration/send-bindings/>
- [W9] Cloudflare, *Email routing rules and addresses* (verifying a destination address):
  <https://developers.cloudflare.com/email-service/configuration/email-routing-addresses/>
- [N1] ntfy, *Publishing* (`POST /<topic>`, `Title`, `Priority` 1–5, `Tags`, `Click`, `Authorization: Bearer`,
  RFC 2047 for non-ASCII headers, 4,096-byte message limit): <https://docs.ntfy.sh/publish/>
- [N2] ntfy, *From your phone* (per-priority notification channels on Android, overriding Do Not Disturb):
  <https://docs.ntfy.sh/subscribe/phone/>
- [N3] ntfy, *Configuration: rate limiting* (per-visitor request bucket and daily message limits):
  <https://docs.ntfy.sh/config/#rate-limiting>
- [N4] ntfy issue #1235, *iOS: Enable critical alerts* (open): <https://github.com/binwiederhier/ntfy/issues/1235>
- [R7] Railway, *Set Up Alerts for Crashes, Restarts, and Failed Deploys* (project webhooks: Failed, Crashed):
  <https://docs.railway.com/guides/alerts-crashes-failed-deploys>
- [R8] Railway Help Station, *No email notification on server crash* (June 2025):
  <https://station.railway.com/questions/no-email-notification-on-server-crash-f352a21b>
- RFC 5322 (message format), RFC 2045 (MIME), RFC 2047 (encoded words), RFC 3834 (`Auto-Submitted`):
  <https://www.rfc-editor.org/rfc/rfc5322>, <https://www.rfc-editor.org/rfc/rfc2045>,
  <https://www.rfc-editor.org/rfc/rfc2047>, <https://www.rfc-editor.org/rfc/rfc3834>
