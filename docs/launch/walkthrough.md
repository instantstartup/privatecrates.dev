# Live walkthrough before the paid launch

A script for the founder, run once on **dev** (`https://dev.privatecrates.dev`, Stripe test mode) with `PREVIEW=false`
and then once on **production** after the switch-over (founder-steps.md §4). Use a throwaway GitHub organisation
(for example `pc-launch-check`, created at <https://github.com/account/organizations/new>, Free plan) and a second
machine or a fresh container for "another machine". Tick each box. When a result differs, send back the step
number, what you saw (a screenshot or the exact text), the URL, and the time (UTC), so it can be found in
`railway logs`.

Set before starting:

```sh
A=https://dev.privatecrates.dev     # then https://privatecrates.dev
ORG=pc-launch-check
```

## 0. Dev only: switch billing on

- [ ] 0.1 `railway variable set -e dev -s privatecrates PREVIEW=false FREE_MEMBER_LIMIT=0`. This makes even a
      one-member organisation billable, so §6 can be tested without 6 accounts.
      **Expect:** a deploy, then `curl -fsS $A/healthz` prints `ok`.
      **Send back** if the deploy fails: the last 30 lines of `railway logs -e dev -s privatecrates`.
- [ ] 0.2 Afterwards (end of §7): `railway variable set -e dev -s privatecrates PREVIEW=true FREE_MEMBER_LIMIT=5`.

## 1. Public pages

- [ ] 1.1 Open `$A/pricing`. **Expect:** no "preview" or "planned" wording; "$10 a month for each member past 5, never
      more than $100"; the trial length; a way to the terms. **Send back:** a screenshot of anything that says
      preview, or a price that differs.
- [ ] 1.2 Open `$A/legal/terms`, `$A/legal/privacy`, `$A/legal/dpa`, the refund page (PC-29), `$A/trust` and
      `$A/security`. **Expect:** paid terms with a version that isn't `preview-…`, Stripe named as a processor, the
      DPA in force, and no "Draft for review" banner. **Send back:** the URL and the sentence that's wrong.
- [ ] 1.3 Open `https://status.privatecrates.dev`. **Expect:** all components operational, with recent probe
      times. **Send back:** a screenshot.
- [ ] 1.4 `curl -fsS $A/.well-known/security.txt`. **Expect:** `Contact: mailto:security@privatecrates.dev` and an
      `Expires` in the future.
- [ ] 1.5 On a phone, or the browser's device mode at 390 px wide: home, pricing, docs/setup. **Expect:** no
      sideways scrolling and nothing cut off (PC-10). **Send back:** a screenshot of each page that scrolls sideways.

## 2. Sign-up and organisation setup

- [ ] 2.1 Open `$A/account` → **Sign in with GitHub** and authorise. **Expect:** the account page lists `$ORG`.
- [ ] 2.2 Choose `$ORG`. **Expect:** the checklist, with the first step *install the reader App*. Install it on
      **all repositories**. **Expect:** the step turns done after **Refresh** or by itself.
- [ ] 2.3 Storage repository: use the step's link to create a private `crates-store` repository, then in its
      *Settings → General → Releases* turn on **immutable releases**. **Expect:** the step turns done.
- [ ] 2.4 Storage App, the wrong way first (PC-13): install it on **all repositories**. **Expect:** the step stays
      open and says "The storage App is installed on N repositories. Install it on the storage repository only."
      Then change the installation to **only** `crates-store`. **Expect:** the step turns done. **Send back:** the
      text if it differs, or if the step turned done on all repositories.
- [ ] 2.5 Registry name and terms: choose a slug, tick the terms checkbox. **Expect:** the checkbox names the paid
      terms version and links to the paid terms; the registry is created; `crates-store` gets a commit of
      `privatecrates.toml`, by the storage App, marked **Verified**.
- [ ] 2.6 At 390 px wide, reload `$A/account`. **Expect:** the checklist and the registry panel fit the screen
      (PC-10).
- [ ] 2.7 `curl -fsS -H "Authorization: $(gh auth token)" https://<slug>.<apex>/index/config.json`, where `<apex>` is
      `dev.privatecrates.dev` or `privatecrates.dev`. **Expect:** JSON with `dl` and `api`.

## 3. First publish from GitHub Actions

- [ ] 3.1 In `$ORG`, create a repository `hello-private` with `cargo new --lib hello-private`. In `Cargo.toml`, set
      `publish = ["<slug>"]` and `repository = "https://github.com/$ORG/hello-private"`. Add `.cargo/config.toml`
      and the publish workflow exactly as `$A/docs/setup` and `$A/docs/ci` show them, including
      `permissions: id-token: write`.
- [ ] 3.2 `git tag v0.1.0 && git push origin v0.1.0`. **Expect:** the workflow is green, and the publish step says
      `Published hello-private v0.1.0`. **Send back:** the workflow run URL and the failing step's log.
- [ ] 3.3 On the account page, under the registry, **expect** `hello-private 0.1.0` in the guide or the crate list
      within a minute.

## 4. Use it from another machine

On a machine that has never used PrivateCrates (a fresh container works: `docker run -it rust:latest`):

- [ ] 4.1 `cargo install cargo-binstall && cargo binstall cargo-credential-privatecrates` (or
      `cargo install --locked cargo-credential-privatecrates`). **Expect:** version 0.2.10 or later.
- [ ] 4.2 `cargo new app && cd app`, add `.cargo/config.toml` with the `[registries.<slug>]` block from
      `$A/docs/setup`, and add `hello-private = { version = "0.1", registry = "<slug>" }` to `Cargo.toml`.
- [ ] 4.3 `cargo login --registry <slug>`. **Expect:** a GitHub device code and URL. After you approve, cargo says
      you're logged in.
- [ ] 4.4 `cargo build`. **Expect:** it downloads `hello-private v0.1.0 (registry <slug>)` and builds.
- [ ] 4.5 `cargo publish --registry <slug>` from the `hello-private` checkout on this machine. **Expect:** it's
      refused with the message that the crate is published from GitHub Actions only (test
      `cargo_publish_of_a_ci_only_crate_explains_what_to_do`).
- [ ] 4.6 Read from CI too: a workflow in another `$ORG` repository that depends on `hello-private` and runs
      `cargo build`, with `id-token: write` and no secrets. **Expect:** green.

## 5. Members and access

- [ ] 5.1 Invite a second GitHub account to `$ORG`. Once it has accepted, have it run 4.3–4.4. **Expect:** it builds.
- [ ] 5.2 Remove it from `$ORG`, then run `cargo update && cargo build` again as that account, with
      `rm -rf ~/.cargo/registry` first. **Expect:** a 404 or 403 for the crate within a minute.

## 6. Billing

On dev, with `FREE_MEMBER_LIMIT=0` (0.1), the organisation is billable. On production, run this section only with
an organisation of more than 5 members; otherwise, check only that the plan step says "Free: n of 5 members".

- [ ] 6.1 Account page → the plan step. **Expect:** a **Billing email** field and a button to start the free trial,
      with no card asked for. Enter an email you can read and start the trial. **Expect:** the plan reads as a
      trial with the days left and the price after it.
- [ ] 6.2 In the Stripe Dashboard (test mode on dev), *Customers*. **Expect:** a customer named after `$ORG` with your
      email, and a subscription in **trialing**, with quantity equal to the billed members.
- [ ] 6.3 **Add a card** opens the Stripe portal. On dev, add `4242 4242 4242 4242`, any future expiry, any CVC.
      **Expect:** back on the account page, no "no card on file" warning.
- [ ] 6.4 End the trial now: Dashboard → the subscription → *Actions → Update subscription* → end the trial now
      (test mode only). **Expect:** an invoice of USD 10 × billed members (at most USD 100), paid; the email
      receipt arrives (on dev only if your address is on the Stripe team or a verified domain). The account page
      shows an active plan.
- [ ] 6.5 Invite one more member (on dev). **Expect:** within about 10 minutes (the periodic refresh), the subscription quantity goes up by
      one, and the next invoice preview shows the new amount with no proration line.
- [ ] 6.6 Failed payment (dev): in the portal, replace the card with `4000 0000 0000 0341` (it attaches, then
      declines on charge), then end the next period from the Dashboard (*Advance time* with a test clock, or
      *Update subscription* → bill now). **Expect:** the subscription goes **past_due**; the account page
      says so; Stripe sends the failed-payment email. **Send back:** the account page's text.
- [ ] 6.7 Invoices: **Manage billing** → the portal lists the invoices, with the company name, number and
      registered office in the footer (founder-steps.md §2.3). On production, also the VAT line if you chose to
      charge tax.

## 7. Cancel

- [ ] 7.1 **Manage billing** → cancel in the portal → confirm. **Expect:** the portal says it ends at the period end; the
      account page shows the end date; publishing still works until then.
- [ ] 7.2 Dev only: end it now from the Dashboard (*Cancel subscription* → immediately). **Expect:** the account page
      offers **Subscribe, $N a month** (Stripe Checkout, card first, no second trial), and says when publishing
      and then reading stop. Once publishing has stopped, a new tag push as in 3.2 fails with a message about the plan.
      **Send back:** the account page's text and the publish error.
- [ ] 7.3 Subscribe again through Checkout with `4242…`. **Expect:** back to active; publishing works.
- [ ] 7.4 Production: cancel the real subscription you made, and refund it from the Dashboard (*Payments → the
      payment → Refund*) if you paid with your own card.

## 8. Clean up

- [ ] 8.1 Uninstall both Apps from `$ORG`, then delete `$ORG`.
- [ ] 8.2 Dev: revert 0.1 (step 0.2).
- [ ] 8.3 Send back the list of unticked steps, each with what you saw.
