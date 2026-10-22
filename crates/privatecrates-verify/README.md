# privatecrates-verify

Checks everything [PrivateCrates](https://privatecrates.dev) wrote to your storage repository, so you need not
trust the service: "don't trust us, verify us". It reports:

- any index change other than appends by the storage App and `yanked` flips;
- any change by the storage App to an existing `owners/` file or to `privatecrates.toml`;
- any version whose release is missing or mutable, or whose `.crate` digest differs from the index checksum;
- any version whose provenance is missing, not signed by GitHub, bound to other bytes, or from a repository,
  workflow or environment the owners file did not allow;
- crate names that clash with crates.io.

Run it in the storage repository itself, on every push and daily. The simplest way to add the workflow is from
the PrivateCrates account page, or with `cargo privatecrates add-verifier <org>`: an admin commits it with their own
GitHub account, so the service never writes what checks it. The workflow is at
<https://privatecrates.dev/docs/verify>. Its install step downloads this release's binary for the runner and checks
it against the release's `SHA256SUMS` and GitHub's build attestation before it runs. Anywhere else:

```sh
cargo binstall privatecrates-verify   # or: cargo install privatecrates-verify --locked
privatecrates-verify --registry https://acme.privatecrates.dev   # in a clone of the storage repository
```

`privatecrates-verify --help` lists every option. Release binaries carry GitHub build provenance; check one with
`gh attestation verify <file> --repo instantstartup/privatecrates.dev`.

## Licence

MIT OR Apache-2.0, at your option.
