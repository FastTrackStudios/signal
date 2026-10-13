#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_FILE="${ROOT_DIR}/crates/frame/render/tests/parity/reports/svg-hash-baselines.json"
SCENARIOS_DIR="${ROOT_DIR}/crates/frame/render/tests/parity/scenarios"

if [[ ! -f "${BASELINE_FILE}" ]]; then
  echo "missing baseline file: ${BASELINE_FILE}" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

failed=0
while IFS=$'\t' read -r scenario expected_sha; do
  actual_svg="${SCENARIOS_DIR}/${scenario}/actual.svg"
  expected_svg="${SCENARIOS_DIR}/${scenario}/expected.svg"
  if [[ ! -f "${actual_svg}" || ! -f "${expected_svg}" ]]; then
    echo "[${scenario}] missing actual.svg or expected.svg"
    failed=1
    continue
  fi

  actual_sha="$(shasum -a 256 "${actual_svg}" | awk '{print $1}')"
  expected_svg_sha="$(shasum -a 256 "${expected_svg}" | awk '{print $1}')"

  if [[ "${actual_sha}" != "${expected_sha}" ]]; then
    echo "[${scenario}] actual hash mismatch: got=${actual_sha} expected=${expected_sha}"
    failed=1
  fi
  if [[ "${expected_svg_sha}" != "${expected_sha}" ]]; then
    echo "[${scenario}] expected.svg hash changed: got=${expected_svg_sha} expected=${expected_sha}"
    failed=1
  fi

  if [[ "${actual_sha}" == "${expected_sha}" && "${expected_svg_sha}" == "${expected_sha}" ]]; then
    echo "[${scenario}] ok (${actual_sha})"
  fi
done < <(jq -r '.baselines[] | [.scenario, .sha256] | @tsv' "${BASELINE_FILE}")

if [[ ${failed} -ne 0 ]]; then
  exit 1
fi

echo "all svg hash baselines passed"
