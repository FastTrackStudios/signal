#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-crates/frame/render/tests/fixtures}"
OUT="${2:-crates/frame/render/tests/fixtures/corpus-manifest.json}"

if [[ ! -d "$ROOT" ]]; then
  echo "missing fixture dir: $ROOT" >&2
  exit 1
fi

jq -n --arg root "$ROOT" '
  {
    generatedAt: (now | todateiso8601),
    root: $root,
    fixtures: []
  }
' > "$OUT"

while IFS= read -r -d '' f; do
  if [[ "$f" == "$OUT" ]]; then
    continue
  fi
  rel="${f#${ROOT}/}"
  bytes=$(wc -c < "$f" | tr -d ' ')
  sha=$(shasum -a 256 "$f" | awk '{print $1}')
  schema=$(jq -r '.schema // empty' "$f" 2>/dev/null || true)
  nodes=$(jq -r 'def walknodes(n): [n] + ((n.children // []) | map(walknodes(.)) | add // []); (.nodes[0]? | walknodes(.) | length) // 0' "$f" 2>/dev/null || echo 0)
  jq \
    --arg rel "$rel" \
    --arg sha "$sha" \
    --arg schema "$schema" \
    --argjson bytes "$bytes" \
    --argjson nodes "$nodes" \
    '.fixtures += [{path:$rel, sha256:$sha, bytes:$bytes, schema:$schema, nodeCount:$nodes}]' \
    "$OUT" > "$OUT.tmp"
  mv "$OUT.tmp" "$OUT"
done < <(find "$ROOT" -type f -name '*.json' -print0 | sort -z)

jq '.' "$OUT"
