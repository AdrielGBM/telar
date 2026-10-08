#!/usr/bin/env bash
# Prints the name of every publishable workspace crate whose telar.toml declares `library = true`, one per line.
#
# The per-crate sweep of publish-crates.sh publishes these one at a time with `cargo telar publish -p`, since their package carries a gitignored transpiled artifact that `cargo publish` alone would refuse as uncommitted; every other crate it retries goes through `cargo publish -p`.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo metadata --format-version 1 --no-deps |
  jq -r '.packages[] | select(.publish == null or (.publish | length) > 0) | "\(.name) \(.manifest_path | rtrimstr("/Cargo.toml"))"' |
  while read -r name dir; do
    if [[ -f $dir/telar.toml ]] && grep -Eq '^[[:space:]]*library[[:space:]]*=[[:space:]]*true' "$dir/telar.toml"; then
      echo "$name"
    fi
  done
