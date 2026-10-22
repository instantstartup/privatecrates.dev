# Founder steps before the paid launch

Ticket PC-32. These are the steps only the founder can take, outside the code, in order. Each one gives the page to
do it on. Where this audit couldn't establish a fact, the step says so as an **open question** rather than
guessing. Check the official pages linked here for current figures before acting: thresholds and fees change.

The code blockers (PC-29 legal pages, PC-30 `PREVIEW` off, and PC-31 tax if needed) must be merged before §4.

## 1. The legal entity

The company exists: INSTANTSTARTUP LTD, company number 17505349, England and Wales, registered office 71-75 Shelton
Street, Covent Garden, London, WC2H 9JQ (`website/src/lib/site.ts` `COMPANY`; `trust` page: incorporated 7 October
2026).

- [ ] **Corporation Tax.** Register the company for Corporation Tax within 3 months of starting to do business
      (taking the first payment counts):
      <https://www.gov.uk/limited-company-formation/set-up-your-company-for-corporation-tax>. Skip this if it is
      already done.
- [ ] **Business bank account** in the company's name, for Stripe payouts (Stripe asks for it in §2.1).
- [ ] **ICO data protection fee.** A company that processes personal data (here: GitHub logins, billing contacts)
      generally has to pay it. Use the self-assessment, then pay:
      <https://ico.org.uk/for-organisations/data-protection-fee/>.
- [ ] **Trading disclosures.** The company's name, number, place of registration and registered office must appear
      on its website, invoices and order forms:
      <https://www.gov.uk/running-a-limited-company/signs-stationery-and-promotional-material>. The website already
      shows them. Invoices are covered in §2.3.

Open questions:

- Is the intellectual property in PrivateCrates (the code, the name, the domain) assigned to the company in
  writing? If you wrote it before incorporation, it is yours personally until assigned. The terms in PC-29 name
  the company as the provider.
- Is 71-75 Shelton Street a registered-office service? If so, check that its plan also lets you use the address
  on invoices and accept post there, or choose a separate service address for customers.

## 2. Stripe in live mode

### 2.1 Activate the account

- [ ] Activate the Stripe account for INSTANTSTARTUP LTD as a company, not as an individual:
      <https://docs.stripe.com/get-started/account/activate>. Stripe asks for the company number, the directors,
      the business website (`https://privatecrates.dev`), a product description, and the bank account from §1.
- [ ] Set the **statement descriptor** to something customers will recognise, for example `PRIVATECRATES`:
      <https://docs.stripe.com/get-started/account/statement-descriptors> (Dashboard → *Settings → Business →
      Public details*).

### 2.2 Products, price, portal and webhook

1. [ ] In **live mode**, open <https://dashboard.stripe.com/settings/billing/portal> and click **Save** once. This
       creates the default portal configuration, which the API can't create (docs/deploy.md §5, step 1).
2. [ ] With a live secret key (Developers → API keys, `sk_live_…`), from a clone of the repository:

   ```sh
   read -rs STRIPE_SECRET_KEY && export STRIPE_SECRET_KEY   # paste sk_live_…
   scripts/stripe-setup.sh production
   ```

   It creates the product, the USD 10 monthly per-member price (lookup key `privatecrates_member_monthly`), the
   webhook to `https://privatecrates.dev/webhooks/stripe` and the portal settings. It prints `STRIPE_PRICE_ID`,
   `STRIPE_WEBHOOK_SECRET` and the `railway variable set` commands. The webhook secret is printed only once:
   store it in the password manager straight away.

### 2.3 Invoices and the public details

- [ ] *Settings → Business → Public details*: business name **INSTANTSTARTUP LTD**, support email, website, and the
      registered office as the address.
- [ ] *Settings → Billing → Invoices* (<https://dashboard.stripe.com/settings/billing/invoice>): add the company
      number, place of registration and registered office to the invoice footer, e.g. "INSTANTSTARTUP LTD,
      registered in England and Wales, company number 17505349, registered office 71-75 Shelton Street, Covent
      Garden, London, WC2H 9JQ". Add the VAT number once you have one (§3).

### 2.4 Customer emails

At <https://dashboard.stripe.com/settings/billing/automatic>, in live mode:

- [ ] Turn on **Send a reminder email 7 days before a free trial ends**, with the payment link set to the
      Stripe-hosted page (docs/deploy.md §5, step 4).
- [ ] Under failed payments: turn on the email to customers when a payment fails, with a link to update the card,
      and choose the retry schedule (Smart Retries).
      <https://docs.stripe.com/billing/revenue-recovery/customer-emails> lists the emails.
- [ ] Turn on emailing finalised invoices and receipts for successful payments
      (<https://dashboard.stripe.com/settings/emails>).

Stripe can email only customers that have an email address. Trials the server starts automatically have none until
an admin adds one on the account page or in the portal (readiness.md, "Failed-payment and trial-ending emails").

## 3. Tax

Open questions. This audit can't answer them; answer them with an accountant before you charge:

- [ ] **UK VAT.** Registration is compulsory once VAT-taxable turnover passes the threshold in any rolling 12
      months, and voluntary below it: <https://www.gov.uk/vat-registration/when-to-register>. At a few hundred
      dollars a month you're likely below it, but voluntary registration affects how you price. Decide: register
      now, or not yet.
- [ ] **EU customers.** A UK seller of electronically supplied services to EU **consumers** owes EU VAT from the
      first sale, through the non-Union One Stop Shop:
      <https://taxation-customs.ec.europa.eu/taxation/vat/one-stop-shop_en>. For EU **businesses** with a VAT ID,
      the customer accounts for the VAT under the reverse charge. PrivateCrates sells to GitHub organisations, so
      nearly all customers will be businesses, but today the checkout doesn't collect a VAT ID to prove it (PC-31).
- [ ] **US sales tax.** Whether SaaS is taxable, and the economic-nexus thresholds, differ by state. Stripe Tax can
      monitor the thresholds without collecting any tax: <https://docs.stripe.com/tax/monitoring>.
- [ ] **Decision for PC-31:** either (a) launch charging no tax, with prices as shown, monitoring in Stripe Tax and
      revisiting at the threshold, so PC-31 doesn't block; or (b) charge tax from day one with Stripe Tax
      (<https://docs.stripe.com/tax>, registrations at <https://dashboard.stripe.com/tax/registrations>), so PC-31
      blocks and the pricing page says "excl. tax". Tell the orchestrator which.

## 4. Production environment (the switch-over)

Only after PC-29 and PC-30 are merged and deployed to production with `PREVIEW` still true, and after §2:

- [ ] Set the live Stripe variables on the production service (docs/deploy.md §6.2, with `E=production`):

  ```sh
  E=production S=privatecrates
  railway variable set -e $E -s $S --skip-deploys STRIPE_PRICE_ID=<price_… from §2.2>
  printf '%s' '<whsec_… from §2.2>'  | railway variable set -e $E -s $S --skip-deploys STRIPE_WEBHOOK_SECRET --stdin
  printf '%s' "$STRIPE_SECRET_KEY"   | railway variable set -e $E -s $S --skip-deploys STRIPE_SECRET_KEY --stdin
  railway variable set -e $E -s $S PREVIEW=false      # this one deploys
  ```

  When `STRIPE_SECRET_KEY` is set, the server refuses to start without `STRIPE_WEBHOOK_SECRET` and
  `STRIPE_PRICE_ID` (`config.rs`), so a missing one shows as a failed deploy, not as silent free service.
- [ ] Check: `railway logs -e production -s privatecrates` shows start-up with no errors. Then, in the Stripe
      Dashboard → *Developers → Webhooks* → the endpoint, **Send test event** (`customer.subscription.updated`) and
      expect 2xx.
- [ ] Run [walkthrough.md](walkthrough.md) against production.

Rolling back is `railway variable set -e production -s privatecrates PREVIEW=true`. With it, billing is off
whatever the Stripe configuration (test `billing::the_preview_is_free_and_never_calls_stripe`).

## 5. Notice to existing organisations

The preview terms (`/legal/terms` §4) promise: "Billing starts only after general availability, and only after at
least 30 days' notice. Paying will need new terms, and no charge is ever taken without a card that an admin has
added." §11 says notice is given "on this site and on the status page". The terms also name general availability as
planned for 2027 (`site.ts` `GA_TARGET`), which PC-30 changes.

1. [ ] Count the organisations that accepted the preview terms:

   ```sh
   railway link                      # privatecrates / production / Postgres
   railway connect postgres          # opens psql
   select org_login, version, accepted_at from terms_acceptances order by accepted_at;
   ```

   No email addresses are stored, so this gives only org and user logins.
2. [ ] If there are none, there is nobody to give notice to; go to §6.
3. [ ] Otherwise, post the notice at least 30 days before billing can start for them, on the site and on the
       status page (docs/deploy.md §14.3, "Post or update an incident", used as an announcement). Say: the date
       paid plans start; the price (free up to 5 members, USD 10 a member a month past 5, at most USD 100); that
       an admin has to accept the new terms; and that nothing is charged without a card. Choose the launch date,
       or keep existing orgs free until the notice runs out.

Open question: is a notice on the site and the status page enough, or do you also want to reach the admins
directly (for example an issue or email to the GitHub organisation's public contact)? Nothing in the code does
that, and it would mean contacting them outside the service.

## 6. Legal review

- [ ] Have a solicitor review the PC-29 pages before `PREVIEW=false`: the paid terms, the refund and cancellation
      page, the privacy notice and the DPA (with the SCCs and the UK addendum). In particular, they should settle
      the refund promise: the trust page and DPA promise a "prorated refund of anything prepaid", but Stripe
      cancels at period end without one (readiness.md, "Refund and cancellation page").
- [ ] Consumer rules: the terms should say the service is for businesses. If consumers can subscribe, UK
      consumer law adds a 14-day cancellation right for digital services, with conditions
      (<https://www.gov.uk/accepting-returns-and-giving-refunds>). Ask the solicitor whether that applies.

## 7. Then

- [ ] Run [walkthrough.md](walkthrough.md) end to end on production with a throwaway organisation, and send back
      every step that didn't match.
