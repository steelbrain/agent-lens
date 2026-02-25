#!/usr/bin/env bash
# Runs the full CI suite locally: fmt, clippy, tests, build.
set -euo pipefail

echo "==> Checking formatting"
cargo fmt --all --check

echo "==> Running clippy"
cargo clippy --all-targets --all-features

echo "==> Running tests"
cargo test --all-features

echo "==> Building release"
cargo build --release

echo "==> All checks passed"
