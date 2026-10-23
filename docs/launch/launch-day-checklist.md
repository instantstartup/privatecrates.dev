# Launch-day checklist

What to post, in what order, what to watch while it is up, and prepared answers to the first questions. Nothing in
this kit has been posted; every post is done by hand from the files beside this one.

## Before launch

- [ ] **Every launch blocker in [readiness.md](readiness.md) is resolved.**
- [ ] Production runs the paid launch: the pricing page's heading reads "Free for small teams, never more than $100
      a month", and it shows "3 months free, and no card to start". Every price in this kit matches that page on
      the day. If any differs, the page wins: fix the posts.
- [ ] https://github.com/instantstartup/privatecrates.dev is public and its README renders. Every post links it.
- [ ] A dry run on production with a fresh demo organisation: set up, `cargo privatecrates init`, publish from a
      tag, `cargo add` from a second repository (the steps in [demo-script.md](demo-script.md)). Then add a sixth
      member and check the trial starts and `cargo publish` warns that no card is set.
- [ ] `cargo binstall cargo-credential-privatecrates` and `cargo binstall cargo-privatecrates` install 0.2.10 on a
      clean machine.
- [ ] The demo video is recorded and uploaded, if you are using it.
- [ ] Accounts ready, all named privatecrates.dev, none personal: Hacker News, Reddit, X, Bluesky, and a GitHub
      account for the TWiR pull request. A brand-new Reddit account can be filtered by r/rust's spam rules; post
      from an account with some history, or ask the moderators first.
- [ ] Read the current rules: https://news.ycombinator.com/showhn.html,
      https://www.reddit.com/r/rust/about/rules, and the README of https://github.com/rust-lang/this-week-in-rust.
      We wrote the posts without network access, so these were not re-checked.
- [ ] Someone can answer for the whole of day 1 and the morning of day 2.

## Order and timing

Launch on a Tuesday, Wednesday or Thursday, not on a US holiday. Times are US Eastern (ET), with UK time beside
them. **Check the offset**: in 2026 the UK's clocks go back on 25 October and the US's on 1 November, so between
those dates the UK is 4 hours ahead of ET, not the usual 5. The UK times below assume 5.

**Day 1**

| ET | UK | What | File |
|---|---|---|---|
| 07:30 | 12:30 | Smoke test: `/healthz`, sign in on the website, `cargo add` against the demo registry. Stop here if anything fails. | — |
| 08:00 | 13:00 | Submit Show HN, then post the first comment within a minute. Most of HN's US readers arrive between 08:00 and 11:00 ET. | [show-hn.md](show-hn.md) |
| 08:15 | 13:15 | Post the thread on X and on Bluesky. | [thread.md](thread.md) |
| 08:15–12:00 | 13:15–17:00 | Answer every HN comment, within about 20 minutes. | below |
| 10:00 | 15:00 | Send the emails, one per person. Don't ask anyone to upvote or comment on HN: HN treats that as voting rings. | [email.md](email.md) |
| 11:00 | 16:00 | Open the TWiR pull request. | [twir.md](twir.md) |
| 17:00 | 22:00 | Write down what broke, what people asked, and any fixes for day 2. | — |

**Day 2**

| ET | UK | What | File |
|---|---|---|---|
| 09:00 | 14:00 | Post to r/rust. A separate day so each thread gets full answers; 09:00 ET is mid-afternoon in Europe, where much of r/rust reads. | [reddit-rust.md](reddit-rust.md) |
| 09:00–13:00 | 14:00–18:00 | Answer every r/rust comment. Keep answering HN. | below |

Don't repost to HN if the post doesn't take off. Don't post to r/rust and HN in the same hour.

## What to watch

Keep these open all of day 1. The service and the posts matter more than any single number.

| What | Where | Look for | If it goes wrong |
|---|---|---|---|
| Status page | https://status.privatecrates.dev (Atom feed at `/feed.xml`) | Every check green | Find the failing check, then Railway |
| Health check | `watch -n 60 curl -fsS https://privatecrates.dev/healthz` | Success each minute | Railway: deploy state, restarts, memory |
| Server logs | Railway → the production service → Logs; Grafana Cloud for search | 5xx responses, panics, "rate limit" from GitHub | Post a short note in the HN thread saying what's wrong; fix, then say it's fixed |
| Error rate | Grafana Cloud: 5xx per minute | More than 1% of requests for 5 minutes | Same as above |
| GitHub Apps | Each App's settings page → Install count, and Advanced → Recent Deliveries | New installs; failed webhook deliveries | Failed deliveries delay access removal: redeliver them and find the cause first |
| GitHub rate limits | Server logs | An organisation hitting its 5,000-an-hour limit | Its limit is its own; tell them, and look at caching |
| Stripe | Dashboard → Customers and Subscriptions; Developers → Webhooks | Trials starting; webhook delivery failures | Failed Stripe webhooks mean wrong billing state: redeliver from the dashboard |
| Inboxes | contact@privatecrates.dev, security@privatecrates.dev | Anything | Answer security reports the same day |
| The threads | HN, r/rust, X, Bluesky | Questions, bug reports | Answers below |

## Prepared answers

Write in the thread as "we", short and factual. If the true answer is "we don't do that yet", say so. Never
promise a date that isn't on the website.

**Why not a git registry or git dependencies?**
Git dependencies have no semver ranges, no `cargo publish` and no search, and every fetch is a clone. A git
index fixes some of that, but you still host the `.crate` files, give CI credentials, and build publishing
yourself. We give you a sparse registry with `cargo publish`, `cargo add`, `cargo search`, yank, and sign-in
through GitHub.

**Why not `cargo vendor`?**
Vendoring copies every dependency into each repository. It works for one build, but there are no versions to
share between repositories and updates are by hand. We use `cargo vendor` ourselves in one place: to hand private
crates to deploy platforms that build outside Actions.

**Why not Cloudsmith, Artifactory, Kellnr or shipyard.rs?**
They are good choices; if one works for you, keep it. What we do differently: your crates and index live in a
repository your organisation owns, permissions are your GitHub permissions with no second user list, and CI
publishes and reads with Actions OIDC and no stored secrets. Kellnr you run yourself; we run it for you. We only
do Rust and only GitHub. (Don't make claims about their products or prices in replies; talk about ours.)

**What happens if you shut down?**
Your crates and your index are already in your repository: each version is an immutable GitHub release and the
index is plain files. They stay there if you leave or we stop, and you can download every `.crate` file from
the releases. A build that has run `cargo vendor` doesn't need us at all. We don't offer a self-hosted server
today, so the exit plan is your repository, not running our server.

**What can you see, and what permissions do the GitHub Apps have?**
Two Apps. The reader App has read access to metadata and members. The storage App has read and write access to
contents of the storage repository only. Neither has administration, so neither can turn off immutable releases.
We store no registry data and no code, only caches keyed by hashes of tokens. Developer tokens can list
repository metadata only; CI tokens are read-only and last an hour. Every write to the index is a commit by the storage App, which GitHub marks verified,
and `privatecrates-verify`, which is open source, checks the storage repository from your own CI on each publish
and daily. Details: https://privatecrates.dev/docs/security

**What if a developer's laptop is stolen?**
On GitHub the token lets the thief list repository metadata, nothing else. Through us it reads the crates that
developer could read, until they sign out everywhere, which takes effect within seconds. It can't publish:
publishing is from Actions only unless an admin allows laptops.

**Do you have SOC 2 or ISO 27001?**
Not yet, and no independent penetration test yet. They are on the roadmap on the trust page. We'd rather say so.

**Why BSL and not open source?**
The client side is open source: the credential provider, the CLI and the verifier are MIT OR Apache-2.0, so
everything that runs on your machines and in your CI is open. The server is BSL 1.1: you may read and audit it, and
the licence doesn't let anyone sell it as a hosted service. Each version becomes Apache-2.0
four years after its release. We don't accept code contributions to the BSL parts, because we must hold the rights
to them; bug reports are welcome.

**Can I self-host?**
Not today. The BSL would allow it for your own organisations, but the server depends on a crate we haven't
published, so it can't be built from the public repository. We run it as a hosted service only. Your crates and
index live in your own GitHub repository either way, and `cargo vendor` builds without us.

**How does pricing work?**
Free for personal GitHub accounts, always, and for organisations with up to 5 members. Past 5, $10 a month for
each member past 5, and never more than $100 a month per organisation. A team of 12 pays $70; a team of 200 pays
$100. Members are active members of the GitHub organisation; outside collaborators, pending invitations and CI
jobs don't count. When an organisation grows past 5 it gets 3 months free with no card. With no card at the end,
publishing stops and reads continue for 14 days. Every feature, SSO included, is in both plans.

**Why per member and not per seat or per download?**
Because GitHub already knows your members and we don't count downloads. The cap keeps it a flat price for any
real company.

**GitLab CI, Buildkite, Jenkins, GitHub Enterprise Server?**
GitHub.com only, and no GitHub Enterprise Server. CI outside Actions can run `cargo vendor` in an Actions job and
build from that, or use a fine-grained, read-only GitHub token for platforms that build from Git, such as Railway
or Render: https://privatecrates.dev/docs/deploy

**Is it a crates.io mirror or proxy?**
No. Your public dependencies still come from crates.io. Search shows your private crates first, then crates.io
results.

**Dependency confusion?**
Publishing a private crate whose name exists on crates.io is refused by default, and we check daily for names
claimed on crates.io later. For names that matter, reserve them on crates.io.

**How do I move existing crates in?**
There's no import tool. Run `cargo privatecrates init` in each crate's repository and publish from its workflow.

**Where is it hosted?**
Railway, US East (Virginia). Logs in Grafana Cloud. The full list of subprocessors is on the trust page.

## Claims checked

| Claim | Source |
|---|---|
| Pricing page heading and trial line (paid copy) | website/src/routes/pricing/+page.svelte (PREVIEW=false branches) |
| Trial starts past 5; `cargo publish` warns without a card | website/src/routes/pricing/+page.svelte (paid FAQ, l.195, l.210); SPEC.md §2.2 |
| Client tools 0.2.10, binstall | crates/cargo-privatecrates/Cargo.toml, crates/cargo-credential-privatecrates/Cargo.toml; website/src/lib/snippets.ts (`RELEASE_VERSION`, `installProvider`, `installCli`) |
| Status page and Atom feed | website/src/lib/trust.ts (`STATUS_URL`); docs/trust-and-status.md l.37 |
| `GET /healthz` | README.md (Running the server) |
| Railway, Grafana Cloud, Stripe, US East (Virginia) | website/src/lib/trust.ts (subprocessors); README.md |
| Org's own GitHub rate limit, 5,000 an hour to start | SPEC.md l.650 |
| Webhooks remove access within seconds | website/src/routes/docs/security/+page.svelte l.167–169 |
| Git deps: no ranges, no publish, no search, clones | SPEC.md §1 |
| `cargo vendor` for deploy platforms | website/src/routes/docs/deploy/+page.svelte; SPEC.md l.195 |
| Data stays in your repo if you leave or the service ends | website/src/routes/legal/terms/+page.svelte l.147; website/src/routes/+page.svelte l.260 |
| App permissions; no administration | website/src/routes/docs/security/+page.svelte l.158, l.167 |
| No registry data or code stored, caches keyed by token hashes | website/src/routes/docs/security/+page.svelte l.41–42 |
| Developer token lists metadata only; stolen-laptop answer | website/src/routes/docs/security/+page.svelte l.8 |
| CI tokens read-only, one hour | website/src/routes/docs/ci/+page.svelte |
| Commits by the storage App, marked verified (SPEC.md l.732–733); verifier on publish and daily | crates/cargo-privatecrates/src/main.rs (`verify` help: "on each publish and daily"); SPEC.md §10.3 |
| Laptops can't publish by default | website/src/lib/snippets.ts (`ownersToml`, `publishRefused`) |
| No SOC 2 / ISO 27001 / pen test; on the roadmap | website/src/routes/trust/+page.svelte l.32–33, l.332–333 |
| Licences, BSL grant, four years, no BSL contributions | README.md (Licences); SPEC.md §2.4 |
| Not self-hostable today: server needs an unpublished crate | crates/privatecrates-server/Cargo.toml l.32 (`privatecrates-qos`, `registry = "instantstartup"`) |
| Pricing examples (12 → $70, 200 → $100), who counts, read grace 14 days, SSO | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (paid FAQ, l.195, l.210); SPEC.md §2.2 |
| GitHub.com only, no GHES, no mirror | SPEC.md l.50 and non-goals |
| Fine-grained read-only token for Railway/Render | website/src/routes/docs/deploy/+page.svelte |
| Search: private first, then crates.io | SPEC.md §9.2 |
| Name clash refused by default; daily check | website/src/routes/docs/publishing/+page.svelte l.202–207 |
| No import tool | SPEC.md l.848 |
