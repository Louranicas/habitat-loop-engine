#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for l in L01 L02 L03 L04 L05 L06 L07; do
  test -f ai_docs/layers/${l}_*.md || { echo "missing layer $l"; exit 1; }
  grep -q "$l" ARCHITECTURE.md || { echo "missing $l in architecture"; exit 1; }
done
printf 'verify-layer-dag PASS
'
