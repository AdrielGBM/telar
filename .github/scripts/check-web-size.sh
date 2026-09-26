#!/usr/bin/env bash
# Builds `apps/landing` for `web-dom` with `--profile web` and gates its compressed size against the
# recorded baseline in `.github/size-budget/landing-web-dom.json`.
#
# The gate is brotli, not the raw module: brotli is what a modern static host (Cloudflare Pages included)
# serves to any browser that accepts it, so it is the number a visitor's download is actually sized by.
# gzip is recorded alongside it for hosts that fall back to it, but does not gate the build.
#
# Raising the baseline is a deliberate, reviewed change to a checked-in file — see docs/build-tuning.md.
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1

BASELINE_FILE=".github/size-budget/landing-web-dom.json"
THRESHOLD_PERCENT=$(jq -r '.threshold_percent' "$BASELINE_FILE")
BASELINE_BROTLI=$(jq -r '.baseline.brotli_bytes' "$BASELINE_FILE")
BASELINE_GZIP=$(jq -r '.baseline.gzip_bytes' "$BASELINE_FILE")
BASELINE_RAW=$(jq -r '.baseline.raw_bytes' "$BASELINE_FILE")

cargo run -q -p cargo-telar -- transpile
cargo run -q -p cargo-telar -- build --target web --renderer dom -p landing -- --release

OUT_DIR="target/telar-dist/web"
MANIFEST="$OUT_DIR/asset-manifest.json"
MODULE=$(jq -r '."app_bg.wasm" // empty' "$MANIFEST" 2>/dev/null)
if [[ -z "$MODULE" ]]; then
  echo "::error::$MANIFEST does not name the wasm module (\"app_bg.wasm\")"
  exit 1
fi
WASM="$OUT_DIR/$MODULE"
BROTLI="${WASM}.br"
GZIP="${WASM}.gz"
for f in "$WASM" "$BROTLI" "$GZIP"; do
  if [[ ! -f "$f" ]]; then
    echo "::error::expected artifact missing: $f (was this a release build?)"
    exit 1
  fi
done

RAW_BYTES=$(stat -c%s "$WASM")
BROTLI_BYTES=$(stat -c%s "$BROTLI")
GZIP_BYTES=$(stat -c%s "$GZIP")

DELTA_BYTES=$((BROTLI_BYTES - BASELINE_BROTLI))
DELTA_PERCENT=$(awk -v d="$DELTA_BYTES" -v b="$BASELINE_BROTLI" 'BEGIN { printf "%.2f", (d / b) * 100 }')
LIMIT_BYTES=$(awk -v b="$BASELINE_BROTLI" -v t="$THRESHOLD_PERCENT" 'BEGIN { printf "%d", b * (1 + t / 100) }')

human() { awk -v n="$1" 'BEGIN { printf "%.1f KiB", n / 1024 }'; }

REPORT=$(cat <<EOF
## apps/landing web-dom size budget

| | raw | brotli (gated) | gzip |
|---|---:|---:|---:|
| current | $(human "$RAW_BYTES") | $(human "$BROTLI_BYTES") | $(human "$GZIP_BYTES") |
| baseline | $(human "$BASELINE_RAW") | $(human "$BASELINE_BROTLI") | $(human "$BASELINE_GZIP") |
| delta | | ${DELTA_PERCENT}% (limit +${THRESHOLD_PERCENT}%) | |

Budget: brotli may not exceed baseline + ${THRESHOLD_PERCENT}% ($(human "$LIMIT_BYTES")).
EOF
)
echo "$REPORT"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  echo "$REPORT" >> "$GITHUB_STEP_SUMMARY"
fi

if (( BROTLI_BYTES > LIMIT_BYTES )); then
  echo "::error::apps/landing brotli size ($(human "$BROTLI_BYTES")) exceeds the budget ($(human "$LIMIT_BYTES"), baseline + ${THRESHOLD_PERCENT}%). Either shrink the build or raise the baseline in $BASELINE_FILE as a deliberate change (see docs/build-tuning.md)."
  exit 1
fi

echo "apps/landing brotli size is within budget."
