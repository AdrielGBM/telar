#!/usr/bin/env bash
# Formats every Rust source in the repository, or checks it with `--check`; CI and local runs go through this one script.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

cargo fmt --all -- "$@"

# A plugin's modules are wired by a macro that rustfmt does not expand, so `cargo fmt` never reaches the files it declares.
git ls-files -z 'plugins/*/src/*.rs' | xargs -0 rustfmt --edition 2024 "$@"
