# This Week in Rust

This Week in Rust (TWiR) is put together in the open in https://github.com/rust-lang/this-week-in-rust. Anyone
can suggest a link by opening a pull request against the draft of the next issue.

We could not fetch the repository while writing this (no network). Before opening the PR, read its README and
CONTRIBUTING notes and check the points marked **check** below.

## Which section

**Project/Tooling Updates.** It lists new releases and announcements of Rust tools and libraries. A hosted
registry with open-source Cargo client crates fits there. It is not "Rust Walkthroughs" (that is for tutorials)
and not "Official".

TWiR's editors link to articles, not product landing pages. So we link to a page that explains how it works:
the docs overview, https://privatecrates.dev/docs. If the founder writes a launch blog post, link that instead.

## The entry

One line, in the format the draft uses (**check** an existing line in the current draft and copy its exact
shape):

```markdown
* [PrivateCrates: a private Cargo registry backed by GitHub, with trusted publishing from Actions](https://privatecrates.dev/docs)
```

If the editors ask for a sentence of context in the PR description, use this:

```
PrivateCrates is a hosted private Cargo registry (sparse protocol, auth-required). Crates are stored as releases in a private repository the organisation owns, access follows GitHub repository permissions, and GitHub Actions publishes with OIDC and no stored secrets. The credential provider, CLI and verifier are MIT OR Apache-2.0; the server is BSL 1.1. Free for personal accounts and organisations up to 5 members, then $10 a month per member past 5, capped at $100 a month.
```

## Exact steps

1. On GitHub, open https://github.com/rust-lang/this-week-in-rust and find the draft of the next issue in the
   `draft/` directory (**check**: the file name is the issue's date, such as `draft/2026-10-28-this-week-in-rust.md`;
   use whichever draft is newest).
2. Click the pencil icon on that file. GitHub forks the repository to the privatecrates.dev GitHub account for you.
3. Find the `### Project/Tooling Updates` heading and add the entry above as the last line of that list.
4. Commit to a new branch named `privatecrates` with the message `Add PrivateCrates to Project/Tooling Updates`.
5. Open the pull request with the title `Add PrivateCrates to Project/Tooling Updates` and the sentence above as
   its description.
6. Don't open a second PR if the editors move or drop the link: they decide. Answer their questions in the PR.

Timing: TWiR publishes weekly, on Wednesdays. Links merged into the draft before the editors close it go in that
issue (**check** the cut-off in the README). Open the PR on launch day; it is fine if it lands in the following
week's issue.

## Claims checked

| Claim | Source |
|---|---|
| Sparse protocol, auth-required | SPEC.md l.37 |
| Releases in an org-owned private repo | README.md (intro); SPEC.md §5 |
| Access follows GitHub repository permissions | README.md (intro) |
| Actions OIDC publishing, no stored secrets | README.md (intro); website/src/lib/snippets.ts (`ciPublish`) |
| Licences | README.md (Licences) |
| Pricing | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (PREVIEW=false copy) |
| TWiR process, section, Wednesday publication | Known practice of the rust-lang/this-week-in-rust repository; not fetched (no network) — marked **check** |
