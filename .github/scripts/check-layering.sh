#!/usr/bin/env bash
# Core crates under `crates/` must never depend on a plugin under `plugins/`, and a plugin may depend on no crate under `crates/` except the `telar` facade, in any dependency kind.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

metadata=$(cargo metadata --format-version 1 --no-deps)

violations=$(jq -r '
  .workspace_root as $root
  | .packages[]
  | (.manifest_path | startswith($root + "/crates/")) as $core
  | (.manifest_path | startswith($root + "/plugins/")) as $plugin
  | select($core or $plugin)
  | .name as $pkg
  | .dependencies[]
  | select(.path != null)
  | (.kind // "normal") as $kind
  | if $core and (.path | startswith($root + "/plugins/")) then
      "core crate depends on a plugin: \($pkg) -> \(.name) (\($kind))"
    elif $plugin and (.path | startswith($root + "/crates/")) and .path != ($root + "/crates/telar") then
      "plugin depends on a kernel crate instead of the telar facade: \($pkg) -> \(.name) (\($kind))"
    else
      empty
    end
' <<<"$metadata")

if [[ -n "$violations" ]]; then
  while read -r violation; do
    echo "::error::$violation"
  done <<<"$violations"
  exit 1
fi
