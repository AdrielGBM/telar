#!/usr/bin/env bash
# Core crates under `crates/` must never depend on a plugin under `plugins/`, in any dependency kind.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

metadata=$(cargo metadata --format-version 1 --no-deps)

violations=$(jq -r '
  .workspace_root as $root
  | .packages[]
  | select(.manifest_path | startswith($root + "/crates/"))
  | .name as $pkg
  | .dependencies[]
  | select(.path != null and (.path | startswith($root + "/plugins/")))
  | "\($pkg) -> \(.name) (\(.kind // "normal"))"
' <<<"$metadata")

if [[ -n "$violations" ]]; then
  while read -r violation; do
    echo "::error::core crate depends on a plugin: $violation"
  done <<<"$violations"
  exit 1
fi
