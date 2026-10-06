# The preview: terms, acceptance, and the road to general availability

Decided 27 September 2026. There is no legal entity yet, so PrivateCrates runs as a **preview**:

- It is operated by **Bryn Dyllan Cooke, as an individual**. A company will be formed before general availability.
- It is **free**: no payments are taken, in any environment, until the entity exists. The planned prices are shown as
  planned, not charged.
- It is provided **as is, at the user's own risk**: no warranty, no service commitment, no liability beyond what law
  forbids excluding, and it may change or stop.
- An organisation admin must **accept the preview terms** before a registry is created for their organisation.
- It is **open to any GitHub organisation or personal account** since 6 October 2026. From 28 September it was a
  private preview by invitation only (§5).

## 1. Preview mode (server)

`PREVIEW=true` (default `true` until general availability; `false` restores billing):

- Billing is off whatever Stripe configuration is present: no Checkout, trials, portal or subscription loading, and no
  enforcement. `/trial`, `/checkout`, `/portal` and `/billing-email` answer `409 billing::preview` ("PrivateCrates is
  free during the preview").
- Every organisation's `plan` is `"free"`, with `members` still reported. The onboarding `plan` step is `done`, with
  detail "Free during the private preview."
- `GET /api/session` gains a top-level `"preview": true` and `"terms": { "version": "preview-2026-09-27", "url":
  "https://{apex}/legal/terms" }`.
- Publish warnings (trial reminders) are off.

## 2. Accepting the terms (server, CLI)

The acceptance protects us, so **we keep the record ourselves**: a customer can delete their storage repository or
revoke our Apps, and the evidence must survive that. It lives in a small **Postgres database** in the same Railway
project and region (`DATABASE_URL`), with Railway's backups. This is the service's only durable data: registry
contents stay in the customer's organisation.

- **Table** `terms_acceptances`, append-only (the application never updates or deletes a row): organisation (id,
  login), the GitHub user who accepted (id, login), terms version, time, `via` (`website` or `cli`), and the exact
  statement accepted. One row per organisation and version; the first acceptance is kept.
- **At registry creation:** `POST /api/orgs/{org}/settings` requires `{"slug": "…", "accept_terms":
  "preview-2026-09-27"}`, the current version, or refuses with `400 account::terms_not_accepted`. The acceptance is
  recorded first; if that fails, nothing is created.
- **For an existing registry** (created before the terms, or when the version changes): `POST
  /api/orgs/{org}/terms` `{"accept_terms": "<current version>"}`, admin only.
- **Reading:** the session's organisation object gains `"terms_accepted": true | false` (the current version). A
  tenant without acceptance **keeps working** (nothing is interrupted), but the account page asks an admin to accept.
- **Production** refuses to start without `DATABASE_URL`: a registry is never created without a recorded acceptance.
- **The CLI:** `cargo privatecrates setup <org> --slug S --accept-terms <version>` and a new
  `cargo privatecrates terms <org> --accept <version>`. Without `--accept-terms`, `setup --slug` prints the terms URL
  and the exact flag to add, and exits non-zero. `--json` reports the version and URL. The CLI never supplies the
  version by itself: the person (or their agent, having shown them the terms) passes it.
- **Agents:** `llms.txt`, `/docs/agents` and the set-up prompt tell the agent to show the admin the terms link, ask them
  to accept, and only then pass `--accept-terms`. An agent must never accept on the admin's behalf.
- **Retention:** for as long as the organisation uses PrivateCrates, then 6 years.

## 3. Website

- `/legal/terms` becomes the **Preview terms** (version `preview-2026-09-27`), plain English, not a draft banner:
  who operates it (Bryn Dyllan Cooke, as an individual; a company before general availability), free during the
  preview, as is and at your own risk, no warranty, no availability or support commitment, limitation of liability to
  the extent the law allows, the service may change or end with notice where possible, your data stays in your GitHub
  organisation (so ending the service loses nothing), acceptable use, how terms change (a new version to accept),
  contact (`contact@privatecrates.dev`; security reports to `security@`). The privacy notice is in force for the
  preview (the operator as controller, no postal address until the company exists); the DPA stays a draft, marked as
  not yet in force during the preview.
- **Onboarding:** the registry-name step has a required checkbox, "I have read and accept the preview terms
  (link), on behalf of {org}", sending `accept_terms`. Admins of an existing registry without acceptance see a banner
  with the same checkbox and button.
- **Preview everywhere it matters:** a slim site-wide "Preview: free, use at your own risk" notice (linking the
  terms); `/pricing` shows the plans as "planned pricing, from general availability; free during the preview";
  `/account` shows "Free during the preview" instead of plan and trial controls; the billing parts of the FAQ say
  billing starts only after general availability, with notice.

## 4. Roadmap to general availability (2027, if there is interest)

| # | Item | Why |
|---|---|---|
| 1 | Form a company; publish its name, number and address | The operator for terms, DPA and billing |
| 2 | Legal review of the terms, privacy notice and DPA (governing law: England and Wales, decided 28 September 2026) | Terms fit for paying customers |
| 3 | International transfer mechanism (EU SCCs and the UK addendum) | EU customers' data processed in the US |
| 4 | Turn on billing (Stripe live mode), with at least 30 days' notice to existing organisations | Revenue |
| 5 | Incident commitments: time to first status update, breach notification deadline (72 hours) | Enterprise reviews ask |
| 6 | Security response targets by severity; a PGP key or other encrypted channel | Disclosure policy completeness |
| 7 | App keys and the token-signing key in a KMS | Least exposure of the most sensitive keys |
| 8 | Independent penetration test, with a summary on the trust centre | Evidence for reviewers |
| 9 | SOC 2 Type I, then Type II | The usual enterprise gate |

The trust centre shows this roadmap publicly, with "2027, subject to demand".

## 5. The invitations, 28 September to 6 October 2026

The preview was by invitation while the service was run by an individual. It opened to everyone on 6 October 2026,
when the company was being formed: `INVITED_ORGS`, the request form and `POST /api/orgs/{org}/invitation` are gone.

The requests people made are still in Postgres, table `invitation_requests` (organisation, GitHub user ID and login,
email, note, time). They are personal data, kept only to tell those people the preview is open. The server deletes
any older than 12 months at start-up. To read them, and to delete them once everyone has been told:

```sh
railway connect Postgres        # with the project linked to the environment
```
```sql
select requested_at, org_login, user_login, email, note from invitation_requests order by requested_at;
-- once everyone has been told:
delete from invitation_requests;
```

After that, a migration drops the table.
