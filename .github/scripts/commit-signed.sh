#!/usr/bin/env bash
# Goes through the GitHub API because it signs the commit as the token's owner, which `git commit` on a runner cannot; needs `GH_TOKEN` with contents write access and `GITHUB_REPOSITORY`.
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: commit-signed.sh <branch> <message>" >&2
  exit 1
fi
branch=$1
message=$2

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

git add --all

# Each file's contents go through a file rather than an argument, since `Cargo.lock` in base64 is past the kernel's limit for one argument.
: >"$scratch/additions"
while IFS= read -r -d '' path; do
  base64 -w0 "$path" >"$scratch/contents"
  jq -nc --arg path "$path" --rawfile contents "$scratch/contents" '{path: $path, contents: $contents}' >>"$scratch/additions"
done < <(git diff --cached --name-only --diff-filter=d -z)

git diff --cached --name-only --diff-filter=D -z | jq -Rsc 'split("\u0000") | map(select(. != "") | {path: .})' >"$scratch/deletions"

if [[ ! -s $scratch/additions && $(cat "$scratch/deletions") == "[]" ]]; then
  echo "commit-signed: nothing to commit" >&2
  exit 1
fi

# The expected head makes the API refuse the commit when the branch moved since checkout, instead of overwriting what was pushed meanwhile.
jq -n \
  --arg repository "$GITHUB_REPOSITORY" \
  --arg branch "$branch" \
  --arg head "$(git rev-parse HEAD)" \
  --arg message "$message" \
  --slurpfile additions "$scratch/additions" \
  --slurpfile deletions "$scratch/deletions" \
  '{
    query: "mutation($input: CreateCommitOnBranchInput!) { createCommitOnBranch(input: $input) { commit { oid } } }",
    variables: {input: {
      branch: {repositoryNameWithOwner: $repository, branchName: $branch},
      expectedHeadOid: $head,
      message: {headline: $message},
      fileChanges: {additions: $additions, deletions: $deletions[0]}
    }}
  }' >"$scratch/request.json"

gh api graphql --input "$scratch/request.json" --jq '.data.createCommitOnBranch.commit.oid'
