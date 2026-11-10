#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix-shell --run './dev.sh'
fi
exec cargo run --locked
