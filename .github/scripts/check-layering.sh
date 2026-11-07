#!/usr/bin/env bash
# Core crates under `crates/` must never depend on a plugin under `plugins/`, and a plugin may depend on no crate under `crates/` except the `telar` facade, in any dependency kind.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

workspace=$(cargo metadata --format-version 1 --no-deps)
root=$(jq -r .workspace_root <<<"$workspace")
members=$(jq -r '.packages[].manifest_path' <<<"$workspace")

# A tracked manifest that is not a member is a workspace of its own, such as a guest fixture, which the workspace's metadata never lists.
packages=$(jq '.packages' <<<"$workspace")
while read -r manifest; do
  if [[ -f $manifest ]] && ! grep -qxF "$root/$manifest" <<<"$members"; then
    standalone=$(cargo metadata --format-version 1 --no-deps --manifest-path "$manifest")
    packages=$(jq -s '.[0] + .[1].packages' <(echo "$packages") <(echo "$standalone"))
  fi
done < <(git ls-files --cached --others --exclude-standard --deduplicate -- '*Cargo.toml')

violations=$(jq -r --arg root "$root" '
  .[]
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
' <<<"$packages")

if [[ -n "$violations" ]]; then
  while read -r violation; do
    echo "::error::$violation"
  done <<<"$violations"
  exit 1
fi
