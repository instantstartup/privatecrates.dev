# PrivateCrates

A hosted private Cargo registry that is a thin wrapper over GitHub: [privatecrates.dev](https://privatecrates.dev).

It implements Cargo's sparse registry protocol (index, download, publish, yank, search) while **GitHub stays the
store, the identity provider and the source of truth for permissions**. Crates are GitHub release assets and the
index is a file tree, both in a repository the customer owns; anyone who can read a crate's repository can use the
crate, and publishing is trusted publishing from GitHub Actions with GitHub-signed provenance. Developers sign in
with GitHub's device flow through a Cargo credential provider; CI needs no stored secrets. The full design is in
[`SPEC.md`](SPEC.md).

## Repository layout

| Path | What | Licence |
|---|---|---|
| [`crates/privatecrates-server`](crates/privatecrates-server) | The hosted service: registry, website and account API, webhooks | BUSL-1.1 |
| [`crates/cargo-credential-privatecrates`](crates/cargo-credential-privatecrates) | Cargo credential provider (device flow, Actions OIDC) | MIT OR Apache-2.0 |
| [`crates/cargo-privatecrates`](crates/cargo-privatecrates) | `cargo privatecrates`: sign in, onboard an organisation, configure crate repositories, check them | MIT OR Apache-2.0 |
| [`crates/privatecrates-verify`](crates/privatecrates-verify) | Verifier customers run against their storage repository | MIT OR Apache-2.0 |
| [`crates/privatecrates-auth`](crates/privatecrates-auth) | Device-flow sign-in and the token store, shared by the provider and `cargo privatecrates` | MIT OR Apache-2.0 |
| [`crates/privatecrates-common`](crates/privatecrates-common) | Types and rules shared by the above | MIT OR Apache-2.0 |
| [`crates/privatecrates-testkit`](crates/privatecrates-testkit) | Fake GitHub and fixtures for the tests (not published) | MIT OR Apache-2.0 |
| [`website/`](website) | SvelteKit static site, served by the server on the apex host | BUSL-1.1 |
| [`status/`](status) | Status page: a Cloudflare Worker that probes the service from outside it | BUSL-1.1 |
| [`docs/`](docs) | [`website-api.md`](docs/website-api.md) (website ↔ server contract), [`deploy.md`](docs/deploy.md) (deployment) | |
| [`deploy/github-apps/`](deploy/github-apps) | GitHub App manifests and the page that registers them | |
| [`scripts/stripe-setup.sh`](scripts/stripe-setup.sh) | Creates the Stripe product, price, webhook and portal configuration | |
| [`Dockerfile`](Dockerfile), [`railway.json`](railway.json) | The container image and its Railway configuration | |
| [`.github/workflows/`](.github/workflows) | CI, and the release of the client tools | |

## Building and testing

Requires Rust 1.98 or later and `git` (the verifier's tests build real repositories; some tests drive the real
`cargo` binary).

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The tests run the server against an in-process fake GitHub (`privatecrates-testkit`); they need no network access
or accounts.

The website:

```sh
cd website
corepack enable
pnpm install --frozen-lockfile
pnpm check && pnpm lint && pnpm build     # output in website/build
```

The image, as deployed:

```sh
docker build -t privatecrates .
```

## Running the server

The server is configured from the environment only. It needs, at minimum, `BASE_DOMAIN`, the two GitHub Apps
(`READER_APP_ID`, `READER_APP_CLIENT_ID`, `READER_APP_CLIENT_SECRET`, `READER_APP_PRIVATE_KEY`, `READER_APP_SLUG`,
`STORAGE_APP_ID`, `STORAGE_APP_PRIVATE_KEY`, `STORAGE_APP_SLUG`), `REGISTRY_TOKEN_SECRET`, `SESSION_SECRET`,
`WEBHOOK_SECRET` and the Stripe settings (`STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_ID`). It
listens on `PORT` (default 8080), serves the static website from `WEBSITE_DIR`, and answers `GET /healthz`.
For local testing, `PUBLIC_SCHEME=http` and a `BASE_DOMAIN` such as `localhost:8080` let tenants be reached at
`{slug}.localhost:8080`.

Every variable, what it is for and how to generate it, and the full deployment on Railway with Cloudflare DNS,
GitHub Apps and Stripe, is in [`docs/deploy.md`](docs/deploy.md).

## Licences

- The server (`crates/privatecrates-server`), the website and the status page are source-available under the
  [Business Source License 1.1](LICENSE-BSL): you may read, modify and self-host it to run a private registry for
  your own organisation(s), but not offer it as a hosted registry service to third parties. Each version becomes
  available under the Apache License 2.0 four years after its first public release.
- Everything else, including the credential provider and the verifier that run on customers' machines and CI, is
  licensed under either of the [MIT licence](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE), at your
  option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the MIT/Apache-2.0
crates shall be dual-licensed as above, without any additional terms or conditions.

We do not accept code contributions to the BSL-licensed parts (the server, the website and the status page), because
the Licensor must hold every right in them to offer them under other terms. Bug reports and suggestions for them
are very welcome as issues.

Release binaries include `THIRD-PARTY-LICENSES.txt`, the licences of every crate compiled into them. The website and
the status page serve theirs at `/third-party-licenses.txt`.
