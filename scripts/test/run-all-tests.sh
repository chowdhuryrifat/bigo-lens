#!/usr/bin/env bash
# Runs every local test suite. Assumes scripts/build/build-all.sh has run.
# Usage: scripts/test/run-all-tests.sh [cmake-preset]   (default: dev)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
preset="${1:-dev}"
cd "$repo_root"
ctest --preset "$preset"
cd extension
npm run lint
npm run validate:schemas
npm test
npm run test:integration
