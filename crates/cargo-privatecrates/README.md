# cargo-privatecrates

`cargo privatecrates` sets up [PrivateCrates](https://privatecrates.dev) private registries from the command line.
It is built for people and for coding agents: every command takes `--json` and exits non-zero on failure.

```sh
cargo privatecrates login                                   # GitHub device flow; prints a code and a link
cargo privatecrates logout
cargo privatecrates setup <org> [--slug <name> --accept-terms <version>] [--start-trial --billing-email <email>] [--json]
cargo privatecrates terms <org> [--accept <version>] [--json]
cargo privatecrates init --registry <name> [--workflow-name publish.yml] [--force]
cargo privatecrates doctor [--registry <name> …] [--crate <name> …] [--json]
```

- **`login` / `logout`**: signs in with the registry's reader App, sharing the token store of
  [`cargo-credential-privatecrates`](https://crates.io/crates/cargo-credential-privatecrates) (the system keyring,
  or a file readable only by you).
- **`setup <org>`**: prints the organisation's onboarding checklist, each step's status and, where GitHub needs a
  person (installing an App), the link. For the storage repository it prints the `gh` commands to run with your own
  login. `--slug` saves the registry name, and needs `--accept-terms` with the version of the PrivateCrates terms
  shown with them: passing it accepts the terms on behalf of the organisation, so only an admin who has read them
  passes it (an agent asks the admin first, and never accepts for them). Without it, `setup --slug` prints the terms'
  URL and the exact flag to add, and exits 1; the command never fills the version in itself. `--start-trial` starts
  the no-card trial (not during the preview, which is free), with `--billing-email` the address that gets the
  reminder before it ends. `--json` prints the account API's onboarding document, with `commands` and `needs_person`
  added to each step and `terms: { version, url, accepted }`.
- **`terms <org>`**: whether the organisation has accepted the current terms (exits 1 while it has not), and, with
  `--accept <version>`, accepts them for a registry set up before them or before their current version.
- **`init --registry <name>`**, in a crate repository or workspace: adds the registry to `.cargo/config.toml`
  (keeping its formatting and comments), sets `package.repository` from the `origin` remote where it is missing
  (in `[workspace.package]`, with `repository.workspace = true` in members, for a workspace), and writes
  `.github/workflows/publish.yml`, which publishes on tags `v*` with GitHub Actions' OIDC token (in a workspace,
  also on `<crate>-v*`, publishing just that crate, while `v*` publishes them all). Running it again
  changes nothing; a file it did not write is replaced only with `--force`.
- **`doctor`**: checks that the credential provider is installed, the registry is configured and answers, you can
  read it, `package.repository` matches the `origin` remote, publishing is restricted to the registry, a workflow
  publishes with `id-token: write`, and the current version is in the index. Each check passes, warns or fails,
  with how to fix it.
  With `--crate <name>`, it checks as a developer depending on that crate instead: whether you can see it, and if
  not, that it does not exist or you cannot read the repository it is published from (the registry does not say
  which).

Every command takes `--domain` for another deployment (`--domain dev.privatecrates.dev`, or a URL such as
`http://localhost:8080` for a local server), or `--url` with a registry's URL.

## Install

```sh
cargo binstall cargo-privatecrates   # prebuilt, attested binary
# or
cargo install --locked cargo-privatecrates
```

## Licence

MIT OR Apache-2.0, at your option.
