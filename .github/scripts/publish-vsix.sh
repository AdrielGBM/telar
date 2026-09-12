#!/usr/bin/env bash
# Runs one VSIX publish command, treating "this version is already up" as done rather than as a failure, and
# retrying anything else before giving up.
#
# A tag that gets re-released — the fix for a half-published release, say — republishes variants that already
# landed on the first attempt, and both registries exit non-zero for those. The artifact they were asked to
# place is there, so the job has nothing left to do and no reason to go red.
#
# Every other failure is tried again and then propagates. The distinction matters more than the tidiness: a
# gallery timeout publishes nothing and looks, to a job that swallowed it, exactly like a success — which is
# how a platform ends up silently missing from a release. Retrying is what the 0.2.1 release wanted: all three
# variants timed out on `/_apis/gallery` within nine minutes, and the same command passed unchanged later.
#
#   ATTEMPTS=3   how many times the command may run before the failure is the answer
#   BACKOFF=45   seconds between attempts
set -uo pipefail

ATTEMPTS=${ATTEMPTS:-3}
BACKOFF=${BACKOFF:-45}

for attempt in $(seq 1 "$ATTEMPTS"); do
  out=$("$@" 2>&1)
  status=$?
  printf '%s\n' "$out"

  ((status == 0)) && exit 0

  if grep -qiE 'already (exists|published|uploaded)' <<<"$out"; then
    echo "Already published at this version — nothing to do."
    exit 0
  fi

  if ((attempt < ATTEMPTS)); then
    echo "Attempt $attempt of $ATTEMPTS failed; retrying in ${BACKOFF}s."
    sleep "$BACKOFF"
  fi
done

exit "$status"
