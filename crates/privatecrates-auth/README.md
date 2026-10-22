# privatecrates-auth

Signing in to [PrivateCrates](https://privatecrates.dev), shared by the credential provider
([`cargo-credential-privatecrates`](https://crates.io/crates/cargo-credential-privatecrates)) and
[`cargo-privatecrates`](https://crates.io/crates/cargo-privatecrates):

- GitHub's device flow for the registry's reader App, and refreshing its eight-hour user tokens directly with GitHub;
- the token store: the operating system's keyring or, where there is none, a file readable only by you
  (`PRIVATECRATES_CREDENTIAL_STORE=file` forces it; `PRIVATECRATES_CONFIG_DIR` moves it).

On Linux the keyring is the Secret Service (GNOME Keyring, KWallet), reached through libdbus. libdbus is built from
source and linked statically, so building needs a C compiler but no `libdbus-1-dev`, and the binaries start without
libdbus installed (using the file). That copy of libdbus is under the Academic Free License 2.1 (dbus's `COPYING`,
included in the third-party notices shipped with the binaries).

It is published so that those tools can be installed from crates.io; it has no stability guarantees of its own.

## Licence

MIT OR Apache-2.0, at your option. The statically linked libdbus is AFL-2.1 (or GPL-2.0-or-later).
