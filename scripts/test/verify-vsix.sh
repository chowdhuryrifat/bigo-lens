#!/usr/bin/env bash
# Verifies a packaged .vsix: contents, then the bundled engine in a clean
# Ubuntu container without LLVM (requires Docker), then the VS Code smoke
# suite against the unpacked extension so the bundled engine is the one used.
#
# Usage: scripts/test/verify-vsix.sh [path/to/bigo-lens.vsix]
#   SKIP_CONTAINER=1   skip the Docker checks
#   SKIP_VSCODE=1      skip the VS Code extension-host run
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
vsix="$(realpath "${1:-$repo_root/extension/bigo-lens.vsix}")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

unzip -q "$vsix" -d "$work/vsix"
ext="$work/vsix/extension"
for required in package.json out/src/extension.js schemas/response.schema.json bin/bigolens-engine \
  bin/clang-resource/include/stddef.h LICENSE.txt node_modules/ajv/package.json; do
  test -e "$ext/$required" || { echo "FAIL: $required missing from the .vsix" >&2; exit 1; }
done
echo "ok   .vsix contains the extension, runtime dependencies, schemas, engine and Clang builtin headers"
chmod +x "$ext/bin/bigolens-engine"  # zip extraction by unzip may drop the mode bit

if [[ "${SKIP_CONTAINER:-0}" != 1 ]]; then
  image="ubuntu:24.04"
  mkdir -p "$work/requests"
  node - "$repo_root/tests/fixtures/cpp/01_linear_loop.cpp" "$work/requests" <<'NODE'
const fs = require('node:fs');
const [fixture, out] = process.argv.slice(2);
const request = (id, path, content) => ({ schema_version: '1.0', request_id: id, action: 'analyze',
  source: { path, language: 'cpp', content } });
fs.writeFileSync(`${out}/header-free.json`, JSON.stringify(request('header-free', 'a.cpp',
  '#include <stddef.h>\nint main() { size_t s = 0; for (int i = 0; i < 10; ++i) { s += i; } return (int)s; }\n')));
fs.writeFileSync(`${out}/fixture.json`, JSON.stringify(request('fixture', '01_linear_loop.cpp',
  fs.readFileSync(fixture, 'utf8'))));
NODE
  run() { docker run --rm -v "$ext/bin:/engine:ro" -v "$work/requests:/requests:ro" \
            -v "$repo_root/shared/examples:/examples:ro" "$image" bash -ec "$1"; }
  # 1. Clean image (no LLVM, no compiler): the engine starts, uses its bundled
  #    headers, and analyzes code that needs only Clang's builtin headers.
  run '/engine/bigolens-engine --version | grep -q "clang resource directory: /engine/clang-resource"
       /engine/bigolens-engine --request /examples/capabilities.request.json | grep -q "\"status\":\"ok\""
       /engine/bigolens-engine --request /requests/header-free.json | grep -q "\"pattern\":\"linear-loop\""'
  echo "ok   bundled engine runs in a clean $image container (no LLVM installed)"
  # 2. With the documented runtime dependency (C++ standard library headers from g++).
  if run 'apt-get update -qq >/dev/null 2>&1
          DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends g++ >/dev/null 2>&1
          command -v g++ >/dev/null || exit 42
          /engine/bigolens-engine --request /requests/fixture.json | grep -q "\"pattern\":\"linear-loop\""'; then
    echo "ok   bundled engine analyzes <iostream> fixtures once g++ is installed (apt)"
  else
    status=$?
    [[ $status -eq 42 ]] || { echo "FAIL: fixture analysis failed with g++ installed" >&2; exit 1; }
    # No package network inside the container: provide the host's GCC headers
    # and runtime objects read-only instead (same distribution, still no LLVM).
    echo "note apt-get could not install g++ in the container (no network); using the host's GCC headers"
    gcc_dir="$(dirname "$(g++ -print-libgcc-file-name)")"
    docker run --rm -v "$ext/bin:/engine:ro" -v "$work/requests:/requests:ro" \
      -v /usr/include:/usr/include:ro -v "$gcc_dir:$gcc_dir:ro" "$image" bash -ec \
      'test ! -e /usr/lib/llvm-* 2>/dev/null || exit 3
       /engine/bigolens-engine --request /requests/fixture.json | grep -q "\"pattern\":\"linear-loop\""'
    echo "ok   bundled engine analyzes <iostream> fixtures with host GCC headers (no LLVM in container)"
  fi
fi

if [[ "${SKIP_VSCODE:-0}" != 1 ]]; then
  (cd "$repo_root/extension" && npm run compile >/dev/null &&
    env -u BIGOLENS_ENGINE_PATH BIGOLENS_E2E_EXTENSION_PATH="$ext" BIGOLENS_E2E_REQUIRE_ENGINE=1 \
      node out/test/e2e/runTest.js)
  echo "ok   VS Code analyzed a fixture using the bundled engine"
fi
