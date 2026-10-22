# cargo-credential-privatecrates

A [Cargo credential provider](https://doc.rust-lang.org/cargo/reference/credential-provider-protocol.html) for
[PrivateCrates](https://privatecrates.dev) registries. Developers and CI use plain Cargo commands with the same
configuration:

```toml
# .cargo/config.toml
[registries.acme]
index = "sparse+https://acme.privatecrates.dev/index/"
credential-provider = ["cargo-credential-privatecrates"]
```

- **On a developer's machine** it signs in with GitHub's device flow for the registry's reader App and keeps the
  refresh token in the operating system's keyring (or, where there is none, a file readable only by you; set
  `PRIVATECRATES_CREDENTIAL_STORE=file` to force it). The resulting token can only read repository metadata.
- **In GitHub Actions** (with `permissions: id-token: write`) it uses the job's OIDC token: exchanged for a
  one-hour read-only registry token to read, or bound to the exact crate, version and checksum to publish. No
  secrets to store.

`cargo login --registry acme` signs in on demand; `cargo logout --registry acme` forgets the stored token.

## Install

```sh
cargo binstall cargo-credential-privatecrates   # prebuilt, attested binary
# or
cargo install --locked cargo-credential-privatecrates
```

Release binaries carry GitHub build provenance; check one with
`gh attestation verify <file> --repo instantstartup/privatecrates.dev`.

## Licence

MIT OR Apache-2.0, at your option.
