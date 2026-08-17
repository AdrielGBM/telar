#!/usr/bin/env bash
# Publishes every publishable workspace crate to crates.io at the version in its manifest.
#
# crates.io throttles uploads, so one `cargo publish --workspace` can stop partway through a 30-crate
# release and leave the registry holding a half-published version. The registry is the source of truth
# here: whatever is already uploaded is skipped, whatever is missing is retried, and the run fails loudly
# if anything is still absent at the end.
#
#   PUBLISH_ARGS=--allow-dirty   extra flags for cargo publish (a local tree with untracked files needs this)
#   ROUNDS=10                    how many sweeps that actually attempted an upload this run may spend
#   COOLDOWN=70                  seconds to wait after a fruitless sweep that named no deadline
#   MAX_WAIT=3600                total seconds this run may spend parked on rate-limit deadlines
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1

ROUNDS=${ROUNDS:-10}
COOLDOWN=${COOLDOWN:-70}
MAX_WAIT=${MAX_WAIT:-3600}
PUBLISH_ARGS=${PUBLISH_ARGS:-}
UA="telar-release (https://github.com/AdrielGBM/telar)"

LOG=$(mktemp)
trap 'rm -f "$LOG"' EXIT
waited=0

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

# Keeps the output on stdout and in $LOG at once, so a 429's deadline can be read back after the sweep.
attempt() {
  # shellcheck disable=SC2086
  cargo publish "$@" $PUBLISH_ARGS 2>&1 | tee -a "$LOG"
}

# A 429 names the instant its bucket refills. Reading it is what separates a release that waits the eight
# minutes a brand-new crate costs from one that sleeps 70s ten times and dies four seconds short.
retry_deadline() {
  local stamp
  stamp=$(sed -n 's/.*try again after \(.*GMT\).*/\1/p' "$LOG" | tail -1)
  [[ -n $stamp ]] && date -d "$stamp" +%s 2>/dev/null
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
attempt --workspace ||
  echo "Workspace publish stopped early — sweeping the remainder one crate at a time."

for ((round = 1; round <= ROUNDS; round++)); do
  mapfile -t pending < <(missing)
  ((${#pending[@]} == 0)) && break

  echo "==> Sweep $round/$ROUNDS — ${#pending[@]} remaining"
  : >"$LOG"
  progressed=0
  for entry in "${pending[@]}"; do
    read -r name version <<<"$entry"
    echo "--> $name $version"
    attempt -p "$name" && progressed=1
  done

  mapfile -t pending < <(missing)
  ((${#pending[@]} == 0)) && break
  ((progressed == 1)) && continue

  deadline=$(retry_deadline)
  now=$(date +%s)
  if [[ -n $deadline ]] && ((deadline > now)); then
    # Overshoot the deadline: the server's clock decides, and a request landing on the boundary is refused.
    nap=$((deadline - now + 5))
    if ((waited + nap > MAX_WAIT)); then
      echo "Rate limit asks for ${nap}s more, over MAX_WAIT=${MAX_WAIT} already ${waited}s spent." >&2
      break
    fi
    waited=$((waited + nap))
    echo "Rate limited until $(date -u -d "@$deadline" '+%H:%M:%S UTC') — sleeping ${nap}s."
    sleep "$nap"
    # Parking on the registry's clock is not an attempt, so it does not spend a round; MAX_WAIT bounds it.
    round=$((round - 1))
  else
    echo "Nothing went through this sweep and no deadline was given; waiting ${COOLDOWN}s."
    sleep "$COOLDOWN"
  fi
done

mapfile -t pending < <(missing)
if ((${#pending[@]} > 0)); then
  echo >&2
  echo "FAILED — still absent from crates.io:" >&2
  printf '  %s\n' "${pending[@]}" >&2
  exit 1
fi

echo
echo "Done: all ${#crates[@]} crates are on crates.io."
