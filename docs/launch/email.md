# Email to Rust friends

Send with Reply-To set to contact@privatecrates.dev (Cloudflare Email Routing only receives mail, so send from a
mailbox that can send), one email per person (no long CC lists), on launch day after the Show HN post
is up. Add one line of your own at the top if you know the person well.

## Subject

```
PrivateCrates: private Rust crates, stored in your GitHub org
```

## Body

```
Hi,

We've launched PrivateCrates, a private Cargo registry for teams that keep their code on GitHub. Your crates are stored as releases in a private repository your organisation owns, anyone who can read a crate's source repository can use it, and GitHub Actions publishes with its OIDC token, so there is no secret to store: push the tag v0.2.0, the workflow runs cargo publish, and in another repo cargo add story_engine --registry acme picks it up after a one-time GitHub sign-in. It is free for personal accounts and for organisations with up to 5 members; past that it is $10 a month for each member past 5, never more than $100 a month (a team of 12 pays $70), with the first 3 months free and no card. If you share internal crates between repositories, we'd value ten minutes of your time trying the setup at https://privatecrates.dev/docs/setup, and a reply telling us where it got in your way.

Thanks,
privatecrates.dev
```

## Claims checked

| Claim | Source |
|---|---|
| Releases in an org-owned private repo; read the repo, use the crate | README.md (intro) |
| Actions OIDC publishing, no secret | README.md (intro); website/src/routes/docs/ci/+page.svelte |
| Tag → `cargo publish`; `cargo add`; one-time sign-in | website/src/lib/snippets.ts (`pushTag`, `ciPublish`, `loginCommands`) |
| Pricing, 12 → $70, 3 months, no card | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (PREVIEW=false copy) |
| About ten minutes of setup | website/src/routes/docs/setup/+page.svelte ("About ten minutes for an organisation admin") |
