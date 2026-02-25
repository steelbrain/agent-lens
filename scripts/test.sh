#!/usr/bin/env bash
set -euo pipefail

echo "==> Running tests"
cargo test --all-features "$@"

echo "==> All tests passed"
