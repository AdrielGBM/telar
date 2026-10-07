#!/usr/bin/env bash
# Prints the name of every publishable workspace crate whose telar.toml declares `library = true`, one per line.
#
# Those ship a transpiled artifact that `cargo publish` alone would refuse as uncommitted, so they are
# published with `cargo telar publish`; everything else goes through plain `cargo publish`.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo metadata --format-version 1 --no-deps |
  jq -r '.packages[] | select(.publish == null or (.publish | length) > 0) | "\(.name) \(.manifest_path | rtrimstr("/Cargo.toml"))"' |
  while read -r name dir; do
    if [[ -f $dir/telar.toml ]] && grep -Eq '^[[:space:]]*library[[:space:]]*=[[:space:]]*true' "$dir/telar.toml"; then
      echo "$name"
    fi
  done
