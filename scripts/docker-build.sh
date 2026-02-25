#!/usr/bin/env bash
# Build the Docker image locally.
set -euo pipefail

echo "==> Building Docker image"
docker build -t agent-lense:latest .

echo "==> Done — run with: docker run -p 3001:3001 agent-lense:latest"
