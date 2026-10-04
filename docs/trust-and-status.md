# Trust centre, status page and compliance dashboard

Plan, 27 September 2026. Companies evaluating PrivateCrates ask three things: what do you hold and who else touches
it (trust centre), is it working and whose fault is it when it is not (status page), and can we prove what happened
in our registry (compliance dashboard). We hold no durable customer data, which keeps all three small.

## 1. Status page: `status.privatecrates.dev`

A Cloudflare Worker in `status/` (TypeScript, Wrangler), deliberately **independent of Railway** so it stays up when
we are down. DNS is already on Cloudflare.

- **Probes** (Cron Trigger, every minute), from outside, against production (`privatecrates.dev`), and dev on a
  separate page `status.privatecrates.dev/dev`:
  - `GET https://{apex}/healthz` (the service is up);
  - `GET https://{apex}/api/status` (the server's own view, below);
  - `GET https://{canary}.{apex}/index/config.json` expecting `401` with the `WWW-Authenticate` header (the registry
    answers Cargo); `canary` is configurable.
- **GitHub**: `https://www.githubstatus.com/api/v2/summary.json` (the public Statuspage API), cached per minute. The
  components that matter to us: *API Requests*, *Git Operations*, *Webhooks*, *Actions*.
- **Components shown**, each `operational | degraded | outage`, with the reason:

  | Our component | Depends on |
  |---|---|
  | Registry reads (index, `cargo build`) | our server; GitHub API Requests (cold caches only); GitHub Webhooks (access changes take effect at once; without them, within the 5-minute cache lifetime) |
  | Downloads | our server; GitHub API Requests; release asset CDN (GitHub publishes no status component for it, so the page cannot attribute a CDN problem by name) |
  | Publishing | our server; GitHub API Requests, Git Operations; GitHub Actions (where customers publish from) |
  | Sign-in | our server; GitHub API Requests |
  | Website and account | our server |
  | Billing | Stripe (`https://status.stripe.com/api/v2/summary.json`) |

- **Attribution**: when one of ours is degraded and a component it depends on is degraded at GitHub (or Stripe), the
  page says so first and links GitHub's incident: "Publishing degraded: GitHub Actions is having an incident (link).
  Registry reads are unaffected." When the server's own GitHub error rate is high but GitHub reports nothing yet, it
  says "Errors calling GitHub's API" rather than blaming us or them.
- **History**: 90 days of per-minute probe results, summarised per day (a bar per day, like Statuspage), in
  Cloudflare D1 (or KV). **Incidents**: written by us, as Markdown files in `status/incidents/` deployed with the
  Worker (title, start, end, affected components, updates); shown on the page, with an Atom feed at `/feed.xml`.
- The page itself is static HTML from the Worker, with the website's look (tokens copied, no framework), light and
  dark, accessible, no third-party requests.

## 2. `GET /api/status` (server, apex host, unauthenticated)

The server's own view, with nothing about any customer:

```json
{
  "version": "0.2.0",
  "started_at": "2026-09-27T14:13:23Z",
  "github": {
    "window_seconds": 300,
    "requests": 1840, "errors": 3, "rate_limited": 0,
    "latency_ms_p50": 180, "latency_ms_p95": 640,
    "last_error_at": "2026-09-27T14:20:01Z"
  },
  "stripe": { "configured": true, "requests": 12, "errors": 0 }
}
```

Counts from a sliding 5-minute window of GitHub (and Stripe) calls made by the server, classified by outcome
(`ok`, `error`, `rate_limited`; 404s used as "not found" answers count as ok). No organisation names, no URLs, no
tokens, and not the number of customers.

## 3. Trust centre: `/trust` (website, public)

- **What we hold**: durably, only terms acceptance records (`docs/preview.md`). What passes through us (tokens in transit, crate bytes during publish), what we
  cache in memory (permissions by token hash, index files, member counts), what we log (GitHub logins, organisation
  names, request paths and error codes; never tokens) and for how long (Railway's log retention; state it).
- **Where it runs**: Railway (region), Cloudflare (DNS only, not proxied), GitHub (customer data stays in the
  customer's own organisation), Stripe (billing).
- **Subprocessors**: Railway, GitHub, Stripe, Cloudflare, with purpose and data for each.
- **Security**: the threat model (SPEC §10), least-privilege Apps, immutable releases and provenance, the verifier,
  encryption in transit (HTTPS, HSTS), session cookies (encrypted, `HttpOnly`), secrets handling.
- **Certifications**: honest: no SOC 2 or ISO 27001 yet; the roadmap.
- **Incident response**: how we detect, communicate (status page, email to admins with a billing email) and our
  target times. Link `status.privatecrates.dev`.
- **Vulnerability disclosure**: `/.well-known/security.txt` (RFC 9116: `Contact: mailto:security@privatecrates.dev`,
  `Expires`, `Policy`, `Preferred-Languages: en`, `Canonical`), and `/security` policy text.
- **Documents**: a DPA template (Markdown, clearly a draft for legal review, no invented company details), the
  subprocessor list, licences.

## 4. Compliance dashboard (per organisation, on `/account`)

`GET /api/orgs/{org}/compliance` (any member who can read the registry; the account API's session or bearer auth).
Built by the server from the tenant's storage repository with the storage App's existing read access: no new
permissions. Each caller sees only the crates whose source repositories they can read: publishers, risks, integrity
problems, the audit log and its CSV are filtered by crate, so the dashboard never names a crate the registry itself
would hide from them.

```json
{
  "org": { "id": 100, "login": "acme" },
  "generated_at": "2026-09-27T14:30:00Z",
  "integrity": {
    "versions": 142, "immutable": 142, "digest_matches": 142,
    "provenance": 139, "manual": 3, "problems": []
  },
  "publishers": [
    { "crate": "story_engine", "repository": "acme/story-engine", "workflows": ["release.yml"],
      "environment": "crates", "manual_publish": false }
  ],
  "risks": [
    { "code": "name_clash", "crate": "serde_utils", "detail": "a crate with this name exists on crates.io" },
    { "code": "manual_publish_allowed", "crate": "tools" }
  ],
  "audit": [
    { "at": "2026-09-27T14:20:01Z", "action": "publish", "crate": "story_engine", "version": "0.2.0",
      "by": "workflow acme/story-engine/.github/workflows/release.yml@refs/tags/v0.2.0 (run 42)",
      "provenance": true, "commit": "abc1234" }
  ]
}
```

- **Integrity** reuses the verifier's checks (`privatecrates-verify`'s library: release present and immutable, digest
  matches `cksum`, provenance signed by GitHub for the right repository, workflow and environment), run server-side
  and cached per version forever once passed (releases are immutable). The customer's own verifier remains the
  independent check; the dashboard says so.
- **Audit** comes from the storage repository's commit history (publish, yank, unyank, owners and settings changes),
  newest first, paginated (`?before=`), and `GET /api/orgs/{org}/compliance/audit.csv` for auditors.
- **Risks**: crates.io name clashes, crates allowing manual publishing, versions without provenance, and whether a
  verify workflow exists in the storage repository (`.github/workflows/*` mentioning `privatecrates-verify`).

## 5. Billing email for the trial

`POST /api/orgs/{org}/trial` takes `{"billing_email": "…"}` (required, validated), set on the Stripe customer so
Stripe's trial-ending reminder reaches someone. The website asks for it in the trial step, prefilled with nothing
(we do not have the user's email).
