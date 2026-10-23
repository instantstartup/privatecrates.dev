# r/rust

Post from the privatecrates.dev Reddit account as a text post (not a link post), with the **🛠️ project** flair.
Timing is in [launch-day-checklist.md](launch-day-checklist.md).

## How this post meets r/rust's norms

We could not fetch the rules while writing this (no network). Read them before posting:
https://www.reddit.com/r/rust/about/rules. The rules we know of, and how the post meets them:

- **Posts must be about Rust.** The post is about Cargo's registry protocol, credential providers and trusted
  publishing, and how to depend on private crates. It names the Cargo features it uses.
- **Self-promotion is allowed when it is relevant and open.** The second line says it is a paid product and what
  it costs. The free tier and the open-source client crates are stated plainly, with no claim that the whole
  thing is open source.
- **Use the right flair.** 🛠️ project.
- **Be there to answer.** Stay in the comments for the first few hours. Answer questions with facts from the
  docs, and say "we don't do that yet" where it is true. Don't argue about the BSL; state what it allows.
- **One post.** Don't cross-post to r/rust again for follow-ups; put changes in the comments.

## Title

```
PrivateCrates: a private Cargo registry where crates live in your GitHub org and CI publishes with no secrets
```

## Body

```
We built a hosted private registry for Rust crates, for teams on GitHub that share internal crates between repositories. It is a paid product with a free tier; prices are below.

**Why.** Git dependencies work until you have a few of them: no semver ranges, no `cargo publish`, no search, and every fetch is a clone. We wanted `cargo add` and `version = "0.2"` for private crates, without running a registry or keeping a second list of who can read what.

**How it works.**

- We implement Cargo's sparse registry protocol (index, download, publish, yank, search) with `auth-required`.
- Crates are release assets, and the index is files, in a private repository your organisation owns.
- Developers sign in through a Cargo credential provider, `cargo-credential-privatecrates`, using GitHub's device flow. The token stays in the system keyring.
- In GitHub Actions, the provider exchanges the job's OIDC token for a read-only registry token. The job needs `id-token: write` and nothing else.
- Publishing is trusted publishing from Actions: the OIDC token is bound to the crate, version and checksum being published, and each version carries GitHub-signed provenance.
- Permissions are GitHub's. If you can read a crate's source repository, you can use the crate. A crate you can't read is a 404, not a 403.
- Search returns your private crates first, then crates.io. Publishing a private crate whose name exists on crates.io is refused by default, to avoid dependency confusion.

**An example.** Each developer adds this to `.cargo/config.toml` once (or `cargo privatecrates init --registry acme` writes it for a repository):

    [registries.acme]
    index = "sparse+https://acme.privatecrates.dev/index/"
    credential-provider = ["cargo-credential-privatecrates"]

In acme/story-engine, `git tag v0.2.0 && git push origin v0.2.0` runs the publish workflow. In any other repo, `cargo add story_engine --registry acme` adds it to `Cargo.toml` with `registry = "acme"`, and the first build asks you to approve a GitHub sign-in.

**Who it's for.** Organisations on github.com that use GitHub Actions. It does not support GitHub Enterprise Server, does not mirror crates.io, and has no download stats.

**Price.** Free for personal GitHub accounts and for organisations with up to 5 members. Past 5, $10 a month for each member past 5, and never more than $100 a month. A team of 12 pays $70 a month. The first 3 months are free with no card. Outside collaborators and CI jobs don't count as members. Both plans have every feature, SSO included.

**Source.** The credential provider, the `cargo privatecrates` CLI and `privatecrates-verify` (a verifier you run in your own CI against your storage repository) are MIT OR Apache-2.0. The server is source-available under BSL 1.1: you may read and audit it, and each version becomes Apache-2.0 four years after release. We run it as a hosted service only; it can't be built from the public repository today. https://github.com/instantstartup/privatecrates.dev

Docs: https://privatecrates.dev/docs

We'd like to hear what your CI or deploy setup needs, and anything in setup that was unclear.

– privatecrates.dev
```

## Claims checked

| Claim | Source |
|---|---|
| Git deps: no semver ranges, no publish, no search, slow clones | SPEC.md §1 |
| Sparse protocol (index, download, publish, yank, search), auth-required | README.md (intro); SPEC.md |
| Releases + index files in an org-owned private repo | README.md; SPEC.md §5 |
| Device flow, keyring | README.md (layout table); crates/cargo-privatecrates/src/main.rs (`login` help) |
| Actions OIDC → read-only token, `id-token: write` | website/src/routes/docs/ci/+page.svelte; website/src/lib/snippets.ts (`ciBuild`) |
| OIDC token bound to crate, version, checksum; provenance | SPEC.md l.232, l.518 (audience `{base_url}/publish/{name}/{version}/{sha256}`); README.md |
| Read the repo, use the crate; 404 not 403 | README.md; website/src/routes/docs/security/+page.svelte, l.168–169 |
| Private first, then crates.io in search | SPEC.md l.283, §9.2 |
| Name clash refused by default | website/src/routes/docs/publishing/+page.svelte, l.202–207 |
| `.cargo/config.toml` block | website/src/lib/snippets.ts (`cargoConfig`) |
| `init --registry` writes the config | crates/cargo-privatecrates/src/main.rs (`Init` help: ".cargo/config.toml, package.repository … and a publish workflow") |
| Tag push publishes | website/src/lib/snippets.ts (`pushTag`, `ciPublish`) |
| `cargo add` writes the dependency with `registry = "acme"` | Cargo's `cargo add --registry` behaviour; website/src/lib/snippets.ts (`dependency`) shows the line |
| First build signs in | website/src/lib/snippets.ts (`loginCommands`) |
| No GHES, no mirror, no download stats | SPEC.md (non-goals) |
| Pricing, 12 → $70, trial, collaborators/CI not counted, SSO in both | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (PREVIEW=false copy and FAQ) |
| Licences | README.md (Licences) |
