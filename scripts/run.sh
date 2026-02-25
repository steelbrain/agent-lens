#!/usr/bin/env bash
# Run the server locally in debug mode.
set -euo pipefail

RUST_LOG="${RUST_LOG:-info,chromiumoxide::conn=off,chromiumoxide::handler=off}"
export RUST_LOG

echo "==> Starting agent-lense (RUST_LOG=$RUST_LOG)"
cargo run -- "$@"
