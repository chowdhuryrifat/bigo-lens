#!/usr/bin/env bash
# Builds the native engine and the VS Code extension.
# Usage: scripts/build/build-all.sh [cmake-preset]   (default: dev)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
preset="${1:-dev}"
cd "$repo_root"
cmake --preset "$preset"
cmake --build --preset "$preset" --parallel
(cd extension && npm ci && npm run compile)
