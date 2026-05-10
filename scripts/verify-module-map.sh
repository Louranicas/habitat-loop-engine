#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for m in M001 M002 M003 M004; do
  grep -q "$m" ai_docs/CODE_MODULE_MAP.md || { echo "missing $m in CODE_MODULE_MAP"; exit 1; }
  grep -q "$m" plan.toml || { echo "missing $m in plan"; exit 1; }
done
printf 'verify-module-map PASS
'
