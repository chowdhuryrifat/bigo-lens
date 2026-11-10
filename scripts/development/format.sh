#!/usr/bin/env bash
# Formats C++ sources with clang-format and extension sources with Prettier.
# Usage: scripts/development/format.sh [--check]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
mapfile -t sources < <(git ls-files '*.cpp' '*.hpp' '*.hpp.in' 2>/dev/null | grep -v '\.in$' || true)
if [[ ${#sources[@]} -eq 0 ]]; then
  mapfile -t sources < <(find engine benchmark tests -name '*.cpp' -o -name '*.hpp')
fi
if [[ "${1:-}" == "--check" ]]; then
  clang-format --dry-run --Werror "${sources[@]}"
  (cd extension && npx prettier --check .)
else
  clang-format -i "${sources[@]}"
  (cd extension && npx prettier --write .)
fi
