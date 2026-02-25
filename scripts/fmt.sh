#!/usr/bin/env bash
set -euo pipefail

echo "==> Formatting code"
cargo fmt --all

echo "==> Done"
