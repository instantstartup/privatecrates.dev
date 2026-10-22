# Show HN

Post from the privatecrates.dev account. Submit the URL, then post the first comment straight away. Timing is in
[launch-day-checklist.md](launch-day-checklist.md).

## Title

```
Show HN: PrivateCrates – a private Cargo registry that stores crates in GitHub
```

78 characters (HN's limit is 80).

## URL

```
https://privatecrates.dev
```

## First comment

```
PrivateCrates is a hosted private registry for Rust crates. It is for teams that keep their code on GitHub and want to share internal crates between repositories with version ranges, `cargo publish` and `cargo add`, instead of git dependencies.

How it works: we implement Cargo's sparse registry protocol, but GitHub does the rest.

- Storage: each version is a release asset in a private repository your organisation owns, and the index is files in that repository. We keep no registry data or code of our own, only caches.
- Permissions: anyone who can read a crate's source repository on GitHub can use the crate. There is no second user list. Removing someone from the org or a team reaches us by webhook and cuts off their access within seconds.
- Developers: a Cargo credential provider signs you in with GitHub's device flow and keeps the token in the system keyring.
- CI: GitHub Actions jobs exchange their OIDC token for a read-only token that lasts an hour. No secrets to store.
- Publishing: trusted publishing from Actions. You push a tag, the workflow runs `cargo publish`, and the version carries GitHub-signed provenance. Publishing from laptops is off unless an admin turns it on.

A concrete example: you push the tag v0.2.0 in acme/story-engine. The workflow publishes story_engine 0.2.0 with no stored secret. A developer in another repo runs `cargo add story_engine --registry acme`, approves a GitHub sign-in once, and builds.

Price: free for personal GitHub accounts and for organisations with up to 5 members. Past that, $10 a month for each member past 5, and never more than $100 a month. A team of 12 pays $70 a month. The first 3 months are free, with no card to start. Every feature, SSO included, is in both plans.

Limits, so you know before you try:
- GitHub.com and GitHub Actions only. No GitHub Enterprise Server. Other CI can vendor crates in an Actions job or use a narrow read-only GitHub token.
- No crates.io mirror and no download stats.
- No SOC 2 or ISO 27001 yet.
- Hosted in US East.

The client tools (credential provider, `cargo privatecrates` CLI, and a verifier you run in your own CI against your storage repository) are MIT OR Apache-2.0. The server is source-available under BSL 1.1: you can read it and self-host it for your own organisation, and each version becomes Apache-2.0 after four years. Code: https://github.com/instantstartup/privatecrates.dev

Setup takes about ten minutes for an org admin, then one config file and one install for each developer: https://privatecrates.dev/docs/setup

What we'd like to hear: where setup was confusing, what your CI or deploy setup needs that we don't cover, and whether "permissions are GitHub's" fits how your team works.

– privatecrates.dev
```

## Claims checked

| Claim | Source |
|---|---|
| Implements Cargo's sparse registry protocol | README.md (intro) |
| Releases in a private repo the org owns; index is files in it | README.md; SPEC.md §5; website/src/routes/docs/setup/+page.svelte |
| "No registry data or code of our own, only caches" | website/src/routes/docs/security/+page.svelte, l.41 ("What we store durably") |
| Read a crate's repo, use the crate | README.md ("anyone who can read a crate's repository can use the crate") |
| Removal by webhook cuts access within seconds | website/src/routes/docs/security/+page.svelte, l.167–169 |
| Device flow, token in system keyring | README.md (layout table); crates/cargo-privatecrates/src/main.rs (`login` help) |
| Actions OIDC → read-only token, 1 hour, no secrets | website/src/routes/docs/ci/+page.svelte; README.md |
| Trusted publishing, tag → workflow, provenance | README.md; website/src/lib/snippets.ts (`ciPublish`, `pushTag`) |
| Laptop publishing off by default | website/src/lib/snippets.ts (`ownersToml` `allow_manual_publish = false`, `publishRefused`) |
| `cargo add` + one-time sign-in | website/src/lib/snippets.ts (`loginCommands`: "the first build does it too") |
| Pricing: free ≤5 and personal, $10/member past 5, cap $100, 12 → $70 | crates/privatecrates-server/src/billing.rs (`billed_members`, `MEMBER_PRICE_USD`, `MAX_BILLED_MEMBERS`); website/src/routes/pricing/+page.svelte (PREVIEW=false copy) |
| 3 months free, no card | website/src/lib/site.ts (`TRIAL_MONTHS`); crates/privatecrates-server/src/config.rs (`TRIAL_DAYS` = 90); pricing page |
| Every feature, SSO included, in both plans | website/src/routes/pricing/+page.svelte |
| No GHES, no mirror, no download stats | SPEC.md (non-goals) |
| Other CI: vendor in Actions or narrow read-only token | website/src/routes/docs/deploy/+page.svelte |
| No SOC 2 / ISO 27001 yet | website/src/routes/trust/+page.svelte |
| US East | website/src/lib/trust.ts (Railway, US East) |
| Licences, BSL → Apache-2.0 after four years | README.md (Licences) |
| About ten minutes for an admin | website/src/routes/docs/setup/+page.svelte |
| Repo URL | README.md / Cargo.toml `repository` — confirm the repo is public before posting (checklist) |
