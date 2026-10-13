#!/usr/bin/env bash
set -euo pipefail

INPUT_PATH="${1:-/tmp/fts-figma-export.json}"

if [[ ! -f "$INPUT_PATH" ]]; then
  echo "missing file: $INPUT_PATH" >&2
  exit 1
fi

jq -r '
  def walknodes(n): [n] + ((n.children // []) | map(walknodes(.)) | add // []);
  .schema as $schema |
  .nodes[0] as $root |
  (walknodes($root)) as $all |
  "schema=\($schema)",
  "nodes=\($all|length)",
  "types:",
  ($all|group_by(.type)|map("  \(.[0].type): \(length)")|.[]),
  "fill_types:",
  ($all|map(.fills // [])|add|map(.type)|group_by(.)|map("  \(.[0]): \(length)")|.[]),
  "effects:",
  ($all|map(.effects // [])|add|map(.type)|group_by(.)|map("  \(.[0]): \(length)")|.[]),
  "svg_exports=\($all|map(select(.exports?.svgBase64 != null))|length)",
  "max_svg_chars=\($all|map(.exports?.svgBase64|strings|length)|max)",
  "total_svg_chars=\($all|map(.exports?.svgBase64|strings|length)|add)",
  "image_fill_refs=\($all|map(.fills // [])|add|map(select(.type=="IMAGE" and .imageHash != null))|length)"
' "$INPUT_PATH"
