#!/usr/bin/env bash
# Regenerates shared/examples/*.response.json by running the real engine on the
# example requests. Responses are never written by hand.
#
# Usage: scripts/development/regenerate-examples.sh [path/to/bigolens-engine]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
engine="${1:-${BIGOLENS_ENGINE_PATH:-$repo_root/build/dev/engine/bigolens-engine}}"
examples="$repo_root/shared/examples"

if [[ ! -x "$engine" ]]; then
  echo "error: engine not found or not executable: $engine" >&2
  echo "build it first: cmake --preset dev && cmake --build --preset dev" >&2
  exit 1
fi

for request in "$examples"/*.request.json; do
  name="$(basename "$request" .request.json)"
  [[ "$name" == invalid ]] && continue
  "$engine" --request "$request" | python3 -m json.tool --indent 2 > "$examples/$name.response.json"
  echo "wrote shared/examples/$name.response.json"
done
"$engine" --request "$examples/malformed.request.txt" | python3 -m json.tool --indent 2 > "$examples/malformed.response.json"
echo "wrote shared/examples/malformed.response.json"
