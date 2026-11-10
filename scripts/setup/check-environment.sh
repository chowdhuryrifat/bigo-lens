#!/usr/bin/env bash
# Reports which development tools are available. Changes nothing.
set -uo pipefail

status=0
check() {
  local name="$1" required="$2"; shift 2
  if command -v "$1" >/dev/null 2>&1; then
    printf '  %-14s %s\n' "$name" "$("$@" 2>&1 | head -n 1)"
  else
    printf '  %-14s %s\n' "$name" "not found$([[ $required == yes ]] && echo ' (required)')"
    [[ $required == yes ]] && status=1
  fi
}

echo "BigO Lens development environment"
check cmake yes cmake --version
check ninja no ninja --version
check c++ yes c++ --version
check node yes node --version
check npm yes npm --version
check git yes git --version
check clang-format no clang-format --version
check clang-tidy no clang-tidy --version
check llvm-config no llvm-config --version

found_clang=""
for dir in /usr/lib/llvm-*/lib/cmake/clang /usr/lib/cmake/clang-* /opt/homebrew/opt/llvm/lib/cmake/clang /usr/local/opt/llvm/lib/cmake/clang; do
  [[ -f "$dir/ClangConfig.cmake" ]] && found_clang="$dir"
done
if [[ -n "$found_clang" ]]; then
  echo "  Clang CMake    $found_clang"
else
  echo "  Clang CMake    not found: the Clang frontend needs Clang/LLVM development packages"
  echo "                 (or configure with the no-clang preset)"
fi
exit $status
