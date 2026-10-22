//! Installing our client tools in GitHub Actions: the prebuilt binary from this version's release, checked against
//! the release's checksums and GitHub's build attestation before it runs. No third-party action, and seconds rather
//! than the minutes `cargo install` takes to compile.

/// The repository whose releases carry the binaries, and whose release workflow attests them.
pub const RELEASE_REPO: &str = "instantstartup/privatecrates.dev";

/// A workflow step, indented for a job's `steps:`, that installs `binary` at this version into `~/.cargo/bin` on a
/// Linux runner (x86_64 or aarch64).
pub fn ci_step(binary: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"      - name: Install {binary} {version} (checksummed and attested)
        env: {{ GH_TOKEN: "${{{{ github.token }}}}", VERSION: "{version}" }}
        run: |
          cd "$(mktemp -d)"
          name={binary}-$(uname -m)-unknown-linux-gnu
          base=https://github.com/{RELEASE_REPO}/releases/download/v$VERSION
          curl -fsSL --remote-name-all "$base/$name.tgz" "$base/SHA256SUMS"
          sha256sum --check --ignore-missing SHA256SUMS
          gh attestation verify "$name.tgz" --repo {RELEASE_REPO} \
            --signer-workflow {RELEASE_REPO}/.github/workflows/release.yml --source-ref refs/tags/v$VERSION
          tar -xzf "$name.tgz"
          install -D "$name/{binary}" ~/.cargo/bin/{binary}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_step_pins_this_version_and_checks_the_download() {
        let step = ci_step("cargo-credential-privatecrates");
        assert!(step.contains(&format!("VERSION: \"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(step.contains("GH_TOKEN: \"${{ github.token }}\""));
        assert!(
            step.contains("name=cargo-credential-privatecrates-$(uname -m)-unknown-linux-gnu\n")
        );
        assert!(step.contains("sha256sum --check --ignore-missing SHA256SUMS\n"));
        // Built by our release workflow, from this version's tag: not by any other workflow of the repository.
        // One backslash: the shell joins the two lines into one command.
        assert!(step.contains(
            "gh attestation verify \"$name.tgz\" --repo instantstartup/privatecrates.dev \\\n            --signer-workflow"
        ));
        assert!(step.contains(
            "--signer-workflow instantstartup/privatecrates.dev/.github/workflows/release.yml --source-ref \
             refs/tags/v$VERSION\n"
        ));
        assert!(step.ends_with("~/.cargo/bin/cargo-credential-privatecrates\n"));
    }
}
