#!/usr/bin/env bash
# Publishes every publishable workspace crate to crates.io at the version in its manifest.
#
# crates.io throttles uploads, so one `cargo publish --workspace` can stop partway through a 30-crate
# release and leave the registry holding a half-published version. The registry is the source of truth
# here: whatever is already uploaded is skipped, whatever is missing is retried, and the run fails loudly
# if anything is still absent at the end.
#
#   PUBLISH_ARGS=--allow-dirty   extra flags for cargo publish (a local tree with untracked files needs this)
#   ROUNDS=10                    how many times to sweep the remaining crates
#   COOLDOWN=70                  seconds to wait after a sweep that published nothing
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1

ROUNDS=${ROUNDS:-10}
COOLDOWN=${COOLDOWN:-70}
PUBLISH_ARGS=${PUBLISH_ARGS:-}
UA="telar-release (https://github.com/AdrielGBM/telar)"

mapfile -t crates < <(
  cargo metadata --format-version 1 --no-deps |
    jq -r '.packages[] | select(.publish == null or (.publish | length) > 0) | "\(.name) \(.version)"'
)

if [[ ${#crates[@]} -eq 0 ]]; then
  echo "No publishable crates found — is cargo metadata working?" >&2
  exit 1
fi

published() {
  [[ $(curl -sS -o /dev/null -w '%{http_code}' -H "User-Agent: $UA" \
    "https://crates.io/api/v1/crates/$1/$2") == 200 ]]
}

missing() {
  local entry name version
  for entry in "${crates[@]}"; do
    read -r name version <<<"$entry"
    published "$name" "$version" || printf '%s\n' "$entry"
  done
}

mapfile -t pending < <(missing)
if [[ ${#pending[@]} -eq 0 ]]; then
  echo "Nothing to do: all ${#crates[@]} crates are on crates.io at their manifest versions."
  exit 0
fi

echo "${#pending[@]} of ${#crates[@]} crates need publishing."
echo "==> cargo publish --workspace"
# Tried whole-workspace first because cargo resolves the publish order itself; the sweep below can only
# approximate that order by retrying what failed.
# shellcheck disable=SC2086
cargo publish --workspace $PUBLISH_ARGS ||
  echo "Workspace publish stopped early — sweeping the remainder one crate at a time."

for ((round = 1; round <= ROUNDS; round++)); do
  mapfile -t pending < <(missing)
  ((${#pending[@]} == 0)) && break

  echo "==> Sweep $round/$ROUNDS — ${#pending[@]} remaining"
  progressed=0
  for entry in "${pending[@]}"; do
    read -r name version <<<"$entry"
    echo "--> $name $version"
    # shellcheck disable=SC2086
    cargo publish -p "$name" $PUBLISH_ARGS && progressed=1
  done

  mapfile -t pending < <(missing)
  ((${#pending[@]} == 0)) && break
  if ((progressed == 0)); then
    echo "Nothing went through this sweep; treating it as the rate limit and waiting ${COOLDOWN}s."
    sleep "$COOLDOWN"
  fi
done

mapfile -t pending < <(missing)
if ((${#pending[@]} > 0)); then
  echo >&2
  echo "FAILED — still absent from crates.io after $ROUNDS sweeps:" >&2
  printf '  %s\n' "${pending[@]}" >&2
  exit 1
fi

echo
echo "Done: all ${#crates[@]} crates are on crates.io."
