#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
if [[ -z "${IN_NIX_SHELL:-}" ]]; then
    exec nix-shell --run './build.sh'
fi
cargo build --release --locked
printf '\nBuilt: %s/target/release/starburst\nRun on NixOS: nix-shell --run ./target/release/starburst\n' "$PWD"
