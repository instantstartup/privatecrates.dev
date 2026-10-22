# Paid-launch readiness

Audit of 22 October 2026, ticket PC-27. On 22 October 2026 the founder decided to launch with billing on: `PREVIEW`
off, free up to 5 members, then USD 10 a member a month, at most USD 100. This audit was done from the code, the
tests and the docs on `dev` at 8362886. Nothing was checked against the live site from here, so what only the live
site can show is a step in [walkthrough.md](walkthrough.md). The founder's own steps are in
[founder-steps.md](founder-steps.md).

## Launch blockers

| Key | What | Kind |
|---|---|---|
| PC-30 | Turn `PREVIEW` off: a paid terms version, a terms check on trial, checkout and portal, and paid copy everywhere the preview is still mentioned | code; needs PC-29 |
| PC-29 | Paid legal pages: paid terms, a refunds and cancellation page, privacy naming Stripe, the DPA in force | code (text), then the founder's legal review in PC-32 |
| PC-32 | Founder pre-launch steps: Stripe live mode, production environment, legal review, ICO fee, notice to existing orgs | founder, outside the code |
| PC-31 | Billing address, tax ID and optional Stripe automatic tax; prices shown "excl. tax" when taxed | code; **blocks only if** the founder decides to charge tax from day one (founder-steps.md §3) |

The launch order is PC-29, then PC-30, then PC-32's switch-over (live Stripe keys and `PREVIEW=false` in
production), and then the live run in [walkthrough.md](walkthrough.md).

## Findings by area

Status: **ready** (works as the paid launch needs), **blocker** (with its ticket), or **not blocking** (worth doing,
not needed for launch).

| Area | Status | Evidence | Ticket |
|---|---|---|---|
| Sign-in with GitHub (reader App, device flow for cargo) | ready | tests `cli_setup::an_admin_sets_up_a_registry`, `credential_e2e::a_developer_signs_in_once`, `credential_e2e::a_domain_wide_sign_in_serves_every_registry` | — |
| Org setup checklist (reader App → storage repo → storage App → registry name → plan) | ready | `crates/privatecrates-server/src/account.rs` onboarding steps; tests `account::onboarding_checklist`, `account::settings_create_the_registry`, `cli_setup::members_see_the_checklist_but_cannot_act` | — |
| Storage-App wizard (PC-13): App installed on more than the storage repository | ready | account.rs storage_app step: Done only on exactly one repository, otherwise "The storage App is installed on {n} repositories. Install it on the storage repository only." | PC-13 (existing) |
| Storage-App wizard (PC-13): immutable releases | not blocking | the storage-repo step advises turning on immutable releases, but they are only checked at the first publish (`publish.rs`, `ImmutableReleasesDisabled`), which then fails with a message that says what to do. Checking them during setup is PC-13's remaining scope | PC-13 (existing) |
| Phone width (PC-10) | not blocking, unverified | can't be checked from a lane (no browser); walkthrough.md step 2.6 checks the account page at 390 px | PC-10 (existing) |
| Cargo: credential provider, sparse index, search | ready | tests `cargo::cargo_consumes_publishes_and_yanks`, `cargo::cargo_search_shows_private_crates_first`; client 0.2.10 (`website/src/lib/snippets.ts`) released to crates.io by `.github/workflows/release.yml` | — |
| Cargo publish from GitHub Actions (OIDC, no secrets) | ready | tests `credential_e2e::publish_and_read_in_github_actions`, `credential_e2e::publish_a_workspace_in_github_actions`, `credential_e2e::a_job_without_id_token_permission_is_told_what_to_add` | — |
| Cargo publish from a laptop | ready (refused by design, explained) | test `cargo::cargo_publish_of_a_ci_only_crate_explains_what_to_do` | — |
| Billing in paid mode: no-card trial, member pricing, cap | ready | `billing.rs` (`billed`, `monthly_price_usd`); tests `the_price_follows_the_members_and_stops_at_100`, `cli_setup::an_admin_starts_the_trial_with_a_billing_email` | — |
| Billing: checkout after the trial, portal (cards, invoices, cancel at period end) | ready | `scripts/stripe-setup.sh` portal configuration; `docs/deploy.md` §5 | — |
| Billing: lapsed or failed payment | ready | test `billing::a_lapsed_subscription_stops_publishing_then_reads`: publishing stops first, then reads, after the grace period | — |
| Billing: webhooks | ready | test `billing::webhooks_must_be_signed_and_fresh`; endpoint created by `stripe-setup.sh` | — |
| Billing: the preview never calls Stripe | ready (preview mode) | test `billing::the_preview_is_free_and_never_calls_stripe` | — |
| Billing: trial, checkout and portal don't check the terms | **blocker** | `account.rs` `check_terms` is called only by the settings and terms handlers; `trial`, `checkout`, `portal` and the automatic trial in `lib.rs` `keep_billing_in_step` don't call it. In paid mode, an org could be billed without anyone accepting the paid terms | PC-30 |
| `PREVIEW` default and terms version | **blocker** | `crates/privatecrates-server/src/config.rs` (`PREVIEW` defaults to true); `privatecrates-common` `TERMS_VERSION = "preview-2026-10-07"`; `website/src/lib/site.ts` `PREVIEW = true`, `TERMS_VERSION = 'preview-2026-10-07'` | PC-30 |
| Copy that isn't gated on `PREVIEW` and still says "preview" | **blocker** | `docs/setup` step 4 ("accept the preview terms"); `trust/+page.svelte` (Stripe "Billing is off during the preview", "in preview: free, and provided as is", "none committed during the preview", "Preview terms"); `lib/trust.ts` Stripe subprocessor line; `security/+page.svelte` ("best-effort basis during the preview", "Draft for review"); `llms.txt` ("Preview terms"); `Footer`, `account/*` (Checklist, OrgList, RegistryPanel, TermsBanner, TermsCheckbox, format.ts), `AgentPrompt`, `docs/agents`, `snippets.ts`. Full list: `grep -rli preview website/src website/static` | PC-30 |
| Terms of service for a paid service | **blocker** | `legal/terms` is the preview terms (free, as is) | PC-29 |
| Refund and cancellation page | **blocker** | no such page. Also, `trust/+page.svelte` and `legal/dpa/+page.svelte` promise a "prorated refund of anything prepaid", while the portal cancels at period end with no proration (`stripe-setup.sh`) and `sync_quantity` uses `proration_behavior=none`: one of them has to change | PC-29 |
| Privacy notice | **blocker** | `legal/privacy` doesn't name Stripe as a processor of billing data (name, email, card via Stripe, invoices) | PC-29 |
| DPA | **blocker** | `legal/dpa` is a draft for review, not in force; for EU and UK customers it needs the SCCs and the UK addendum | PC-29 |
| Pricing page | ready (copy) | `website/src/routes/pricing/+page.svelte` has a paid branch with an FAQ on growth, trial end, cancelling, failed payment and who manages billing. Its "unlimited users" means there is no cap on users (the price stops at USD 100, 15 billed members), which is consistent with per-member pricing | — |
| Tax: prices, invoices, VAT/sales tax | **blocker if taxed** | `billing.rs` sends no `automatic_tax`, `billing_address_collection` or `tax_id_collection`; the pricing page says nothing about tax | PC-31 |
| Failed-payment and trial-ending emails | not blocking | `invoice.*` webhooks are ignored; the emails are Stripe's, turned on by hand (founder-steps.md §2.5). Automatic-trial customers have no email until an admin enters one, so the cargo publish warning in the last 14 days is their only reminder | — |
| USD 0 invoices | not blocking | `sync_quantity` takes a subscription down to quantity 0 when an org shrinks to the free limit, so Stripe issues USD 0 invoices each month. Correct, but noisy | — |
| Company details on the site (UK trading disclosures) | ready | `website/src/lib/site.ts` `COMPANY`: INSTANTSTARTUP LTD, 17505349, England and Wales, registered office | — |
| Company details on Stripe invoices | **blocker (founder)** | set in the Stripe dashboard, not in code (founder-steps.md §2.3) | PC-32 |
| Status page | unverified | `status/` Worker, docs/deploy.md §14; test `status::the_status_shows_counts_and_nothing_about_customers`. Live check in walkthrough.md step 1.3 | — |
| Trust and security pages, security.txt | ready apart from the preview copy above | `website/static/.well-known/security.txt` (expires 2027-09-27, security@privatecrates.dev); tests `website::serves_security_txt_from_the_build`, `website::apex_responses_carry_security_headers` | PC-30 (copy) |
| Docs (setup, CI, deploy) | ready apart from setup step 4 | `website/src/routes/docs/*` | PC-30 |
| docs/preview.md said free "until the entity exists" | fixed here | commit f41806a | — |
| `stripe-setup.sh` product description said "unlimited users" | fixed here | commit 21c5338 | — |
| docs/deploy.md §5 described a USD 100 flat price | fixed here | commit 291d5e4 | — |
| `/docs/security` listed preview invitation requests as stored, though the table was dropped (migration 0004) | fixed here | commit 9ca29e0 | — |
| Home FAQ said only GitHub Actions can read the registry, contradicting `/docs/deploy` | fixed here | commit 237f741 | — |

## Existing organisations

`terms_acceptances` holds every org that accepted the preview terms (`org_login`, `user_login`, `version`,
`accepted_at`), and no email addresses. In paid mode, these orgs see the terms banner until an admin accepts the
paid version (`records.rs` `accepted`), but the banner doesn't stop the registry. Once PC-30 lands, billing waits
for that acceptance. How they get notice before launch is founder-steps.md §5.
