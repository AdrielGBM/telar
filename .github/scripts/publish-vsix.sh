#!/usr/bin/env bash
# Runs one VSIX publish command, treating "this version is already up" as done rather than as a failure.
#
# A tag that gets re-released — the fix for a half-published release, say — republishes variants that already
# landed on the first attempt, and both registries exit non-zero for those. The artifact they were asked to
# place is there, so the job has nothing left to do and no reason to go red.
#
# Every other failure still propagates. The distinction matters more than the tidiness: a gallery timeout
# publishes nothing and looks, to a job that swallowed it, exactly like a success — which is how a platform
# ends up silently missing from a release.
set -uo pipefail

out=$("$@" 2>&1)
status=$?
printf '%s\n' "$out"

((status == 0)) && exit 0

if grep -qiE 'already (exists|published|uploaded)' <<<"$out"; then
  echo "Already published at this version — nothing to do."
  exit 0
fi

exit "$status"
