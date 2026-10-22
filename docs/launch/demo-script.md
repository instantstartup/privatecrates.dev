# 60-second demo

A screen recording with on-screen captions and no voice. It shows one crate going from a repository to another
repository's `Cargo.toml`: set up with `cargo privatecrates init`, publish from GitHub Actions with no stored
secret, then `cargo add` with a one-time GitHub sign-in through the credential provider.

The commands use the organisation `acme` and the registry `acme`, as the docs do. Record it with a real demo
organisation you own and replace `acme` everywhere, including in the URLs. Don't show a customer's organisation.

## Before recording

Do these off camera, and check each one:

1. A demo GitHub organisation with PrivateCrates set up: the registry, the storage repository `crates-store` with
   immutable releases on, and both GitHub Apps installed (https://privatecrates.dev/docs/setup).
2. Two repositories in it, cloned side by side: `story-engine` (a library crate, `version = "0.2.0"` in its
   `Cargo.toml`, nothing published yet) and `web-app` (a binary crate).
3. In `web-app`, the registry already in `.cargo/config.toml` (run `cargo privatecrates init --registry acme
   --no-workflow --yes` and commit it).
4. The credential provider and the CLI installed:
   ```sh
   cargo binstall cargo-credential-privatecrates
   cargo binstall cargo-privatecrates
   ```
5. Signed out, so the sign-in shows on camera: `cargo logout --registry acme`.
6. `story-engine` has no repository secrets (Settings → Secrets and variables → Actions is empty).
7. Terminal: a large font (at least 18 pt), a short prompt that shows the directory, 100 columns wide. Browser:
   signed in to GitHub as a member of the demo organisation, zoomed to 125%.
8. Rehearse once end to end and note how long the Actions run takes. The edit cuts the wait.

## Script

| Time | Shot | Typed or clicked | Caption |
|---|---|---|---|
| 0:00–0:04 | The privatecrates.dev home page | — | Private Rust crates, stored in your GitHub organisation. |
| 0:04–0:14 | Terminal in `story-engine` | `cargo privatecrates init --registry acme`, then `y` at the prompt | One command sets up a crate repository. It shows each change and asks first. |
| 0:14–0:19 | Terminal | `cat .cargo/config.toml` | The registry, and the credential provider that signs you in with GitHub. |
| 0:19–0:25 | Terminal | `git add -A && git commit -m "Publish to acme" && git push` then `git tag v0.2.0 && git push origin v0.2.0` | Push a tag to publish. |
| 0:25–0:33 | Browser: the `publish` run in `story-engine`'s Actions tab, then the `cargo publish --registry acme` step's log (cut the wait) | Click the run, then the step | GitHub Actions publishes with its OIDC token. |
| 0:33–0:36 | Browser: `story-engine` → Settings → Secrets and variables → Actions | — | No stored secrets. |
| 0:36–0:41 | Browser: `crates-store` → Releases → `story_engine-0.2.0` | — | The crate is a release in your own repository, with its provenance. |
| 0:41–0:47 | Terminal in `web-app` | `cargo add story_engine --registry acme` | In another repository: cargo add. |
| 0:47–0:53 | The credential provider's prompt in the terminal, then the browser at github.com/login/device: enter the code, Authorize | Copy the code, paste, click Authorize | The first time, approve a GitHub sign-in. Access follows your GitHub permissions. |
| 0:53–0:56 | Terminal: `cargo add` finishes; then `cargo build` | `cargo build` | Built. |
| 0:56–1:00 | End card | — | Free up to 5 members. $10 a month per member past 5, never more than $100. privatecrates.dev |

Notes for the edit:

- At 0:04 the CLI lists its edits: the registry in `.cargo/config.toml`, `repository` and `publish = ["acme"]` in
  `Cargo.toml`, and `.github/workflows/publish.yml`. Hold the frame long enough to read them.
- In a real team the `init` changes go through a pull request. Pushing to the default branch keeps the demo short;
  that is fine for a demo repository.
- At 0:47 the prompt reads "To use https://acme.privatecrates.dev, sign in with GitHub: open … and enter the code …".
  Blur nothing: the code is single-use and expires.
- Export at 1920×1080, 30 fps, with captions burned in, under 60 seconds. Upload where the posts link to it; the
  posts don't depend on it.

## Claims checked

| Claim | Source |
|---|---|
| `init --registry` edits `.cargo/config.toml`, `repository`, `publish`, adds `publish.yml`; asks first | crates/cargo-privatecrates/src/main.rs (`Init`, `workflow_name` default `publish.yml`); crates/cargo-privatecrates/src/init.rs (module doc; `publish` and `repository` edits, l.561–707) |
| `--no-workflow`, `--yes` | crates/cargo-privatecrates/src/main.rs (`Init` flags) |
| Config file contents | website/src/lib/snippets.ts (`cargoConfig`) |
| Install commands | website/src/lib/snippets.ts (`installProvider`, `installCli`) |
| `cargo logout --registry acme` | website/src/lib/snippets.ts (`loginCommands`) |
| Tag `v*` triggers `cargo publish --registry acme` with `id-token: write` | website/src/lib/snippets.ts (`ciPublish`, `pushTag`) |
| No stored secrets | README.md (intro) |
| Release `story_engine-0.2.0` in `crates-store`, with provenance | SPEC.md §5 (release tag `{name}-{version}`, `.crate` and `.provenance.jwt` assets); website/src/lib/snippets.ts (`STORAGE_REPO`) |
| Device-flow prompt text | crates/privatecrates-auth/src/device.rs l.105 |
| Sign-in on first use | website/src/lib/snippets.ts (`loginCommands`: "the first build does it too") |
| Pricing on the end card | crates/privatecrates-server/src/billing.rs; website/src/routes/pricing/+page.svelte (PREVIEW=false copy) |
