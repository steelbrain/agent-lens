#!/usr/bin/env bash
set -euo pipefail

echo "==> Checking formatting"
cargo fmt --all --check

echo "==> Running clippy"
cargo clippy --all-targets --all-features

echo "==> All checks passed"
