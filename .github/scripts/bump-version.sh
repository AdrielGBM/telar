#!/usr/bin/env bash
# Moves every crate to the next patch version, the way a release leaves the tree ready for the one after it.
#
# All crates share one version: `[workspace.package]`, the matching entries in `[workspace.dependencies]`
# and the test fixtures, the VS Code extension, the install snippets in the docs, and `Cargo.lock`. Until the
# tree moves past a published version, cargo resolves that version from crates.io instead of the workspace.
#
#   bump-version.sh            bump the patch, print the new version
#   bump-version.sh --current  print the version in `[workspace.package]` and change nothing
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

manifests=(Cargo.toml crates/telar/tests/fixtures/embed-guest/Cargo.toml crates/telar/tests/fixtures/hot-panic-guest/Cargo.toml)
extension=editors/vscode/telar/package.json

current=$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version = "\([^"]*\)"$/\1/p}' Cargo.toml)
if [[ ! $current =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "bump-version: [workspace.package] version '$current' is not major.minor.patch" >&2
  exit 1
fi

if [[ ${1:-} == --current ]]; then
  echo "$current"
  exit 0
fi

next="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.$((BASH_REMATCH[3] + 1))"

stale=$(grep -hE 'path = ".*version = "|version = ".*path = "' "${manifests[@]}" | grep -vF "version = \"$current\"" || true)
extension_version=$(sed -n 's/^  "version": "\([^"]*\)",$/\1/p' "$extension")
if [[ -n $stale || $extension_version != "$current" ]]; then
  echo "bump-version: entries disagree with $current, fix them before bumping:" >&2
  [[ -z $stale ]] || echo "$stale" >&2
  [[ $extension_version == "$current" ]] || echo "$extension: $extension_version" >&2
  exit 1
fi

escaped=${current//./\\.}

sed -i "/^\[workspace\.package\]/,/^\[/s/^version = \"$escaped\"\$/version = \"$next\"/" Cargo.toml
sed -i "s/^  \"version\": \"$escaped\",\$/  \"version\": \"$next\",/" "$extension"

# Prose and install snippets name a crate beside its version; other crates that happen to share the number stay.
mapfile -t documented < <(git grep -lF "$current" -- . ':!Cargo.lock' ':!Cargo.toml' ":!$extension")
for file in "${documented[@]}"; do
  sed -i -E "/telar/I{/paranoid-android|unicode-width/!s/([\"\`=])$escaped([\"\`])/\1$next\2/g}" "$file"
done
sed -i -E "/path = /s/version = \"$escaped\"/version = \"$next\"/" "${manifests[@]}"

cargo update --workspace --offline --quiet

echo "$next"
