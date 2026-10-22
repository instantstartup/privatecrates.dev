# X / Bluesky thread

Post from the privatecrates.dev accounts on X and Bluesky. Same text on both. Each post is one block below; the
count after it is its length in characters, every character counted (X counts a link as 23, so the real X count
of a post with a link is lower). Every post is under 280, so it fits X and Bluesky (300).

## 1/5

```
PrivateCrates is a private Cargo registry for teams on GitHub.

Your crates live as releases in a private repo your org owns. GitHub decides who can read them. CI publishes with no stored secrets.

https://privatecrates.dev
```

Count: 223

## 2/5

```
Who it's for: Rust teams that share internal crates between repos and are tired of git dependencies.

You get version ranges, cargo add, cargo publish and search, with Cargo's own sparse registry protocol.
```

Count: 205

## 3/5

```
Example: push the tag v0.2.0 in acme/story-engine. A GitHub Actions workflow runs cargo publish, signed in with the job's OIDC token.

In another repo, cargo add story_engine --registry acme, approve a GitHub sign-in once, and build.
```

Count: 233

## 4/5

```
Permissions are GitHub's. If you can read a crate's source repo, you can use the crate.

Remove someone from the org or a team and their access ends within seconds. There is no second user list to keep in sync.
```

Count: 210

## 5/5

```
Price: free for personal accounts and orgs up to 5 members. Then $10 a month per member past 5, never more than $100 a month. A team of 12 pays $70.

3 months free, no card. SSO included.

Setup: https://privatecrates.dev/docs/setup
```

Count: 232

## Claims checked

| Claim | Source |
|---|---|
| Releases in an org-owned private repo; GitHub permissions | README.md (intro); SPEC.md §5 |
| CI publishes with no stored secrets (Actions OIDC) | README.md ("CI needs no stored secrets"); website/src/lib/snippets.ts (`ciPublish`) |
| Version ranges, cargo add, publish, search; sparse protocol | SPEC.md §1, l.37 |
| Tag v0.2.0 → workflow runs `cargo publish` | website/src/lib/snippets.ts (`pushTag`, `ciPublish`) |
| `cargo add … --registry acme`, sign-in on first use | website/src/lib/snippets.ts (`loginCommands`: "the first build does it too") |
| Access ends within seconds on removal | website/src/routes/docs/security/+page.svelte, l.167–169 |
| Pricing, 12 → $70, 3 months, no card, SSO included | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (PREVIEW=false copy) |
| Character counts | Counted with `python3 -c 'len(text)'` over each block |
