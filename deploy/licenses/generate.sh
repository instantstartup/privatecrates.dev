#!/usr/bin/env bash
# Writes THIRD-PARTY-LICENSES.txt for one client binary: every crate compiled into it, and its licence text.
#   deploy/licenses/generate.sh <binary> <output file>
set -euo pipefail
binary="$1"
out="$2"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cargo about generate --fail --locked -c "$root/about.toml" -m "$root/crates/$binary/Cargo.toml" \
  "$root/deploy/licenses/third-party.hbs" -o "$out.tmp"
sed "s/@BINARY@/$binary/g" "$out.tmp" > "$out"
rm "$out.tmp"

# Apache-2.0 §4(d): a dependency's NOTICE file goes out with it.
cargo metadata --format-version 1 --locked --manifest-path "$root/Cargo.toml" \
  | jq -r '.packages[] | select(.source != null) | "\(.name) v\(.version)\t\(.manifest_path)"' > "$out.paths"
cargo tree --locked --manifest-path "$root/Cargo.toml" -p "$binary" -e normal --prefix none --format '{p}' \
  | sed 's/ (.*//' | sort -u | while read -r package; do
    dir="$(awk -F'\t' -v p="$package" '$1 == p { sub(/\/Cargo.toml$/, "", $2); print $2; exit }' "$out.paths")"
    [ -n "$dir" ] || continue
    for notice in "$dir"/NOTICE*; do
      [ -f "$notice" ] || continue
      printf '\n=====================================================================\nNOTICE of %s\n\n' "$package" >> "$out"
      cat "$notice" >> "$out"
    done
  done
rm "$out.paths"
