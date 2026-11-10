#!/usr/bin/env bash
# Builds a self-contained Linux .vsix: the engine linked statically against
# Clang/LLVM, Clang's builtin headers, and the compiled extension (ADR 0011).
#
# Usage: scripts/build/package-extension.sh [extra cmake configure args...]
#   e.g. scripts/build/package-extension.sh -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang
# Output: extension/bigo-lens.vsix
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

cmake --preset package "$@"
cmake --build --preset package --parallel
ctest --preset package

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
cmake --install build/package --prefix "$staging" --strip

engine="$staging/bin/bigolens-engine"
if ldd "$engine" | grep -E -q 'libLLVM|libclang'; then
  echo "error: the packaged engine still links LLVM/Clang shared libraries:" >&2
  ldd "$engine" >&2
  exit 1
fi
test -f "$staging/bin/clang-resource/include/stddef.h" || {
  echo "error: Clang builtin headers missing from $staging/bin/clang-resource" >&2
  exit 1
}

rm -rf extension/bin
mkdir -p extension/bin
cp -a "$staging/bin/." extension/bin/
chmod 0755 extension/bin/bigolens-engine

(cd extension && npm ci && npm run package)
ls -l extension/bigo-lens.vsix
echo "engine: $(du -h extension/bin/bigolens-engine | cut -f1), headers: $(du -sh extension/bin/clang-resource | cut -f1)"
