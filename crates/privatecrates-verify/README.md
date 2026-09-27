# privatecrates-verify

Checks everything [PrivateCrates](https://privatecrates.dev) wrote to your storage repository, so you need not
trust the service: "don't trust us, verify us". It reports:

- any index change other than appends by the storage App and `yanked` flips;
- any change by the storage App to an existing `owners/` file or to `privatecrates.toml`;
- any version whose release is missing or mutable, or whose `.crate` digest differs from the index checksum;
- any version whose provenance is missing, not signed by GitHub, bound to other bytes, or from a repository,
  workflow or environment the owners file did not allow;
- crate names that clash with crates.io.

Run it on a schedule in the storage repository itself:

```yaml
on:
  schedule: [{ cron: "17 * * * *" }]
permissions:
  contents: read
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: actions/cache@v4
        with: { path: .privatecrates-verify.json, key: "verify-${{ github.run_id }}", restore-keys: verify- }
      - run: cargo binstall --no-confirm privatecrates-verify
      - run: privatecrates-verify --registry https://acme.privatecrates.dev
        env: { GITHUB_TOKEN: "${{ github.token }}" }
```

`privatecrates-verify --help` lists every option. Release binaries carry GitHub build provenance; check one with
`gh attestation verify <file> --repo worldbuilding-dev/privatecrates.dev`.

## Licence

MIT OR Apache-2.0, at your option.
