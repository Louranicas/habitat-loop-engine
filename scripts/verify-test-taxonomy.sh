#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for d in tests/unit tests/integration tests/fixtures/negative; do
  test -d "$d" || { echo "missing $d"; exit 1; }
done
test -f tests/unit/test_manifest.py || { echo 'missing unit manifest test'; exit 1; }
test -f tests/integration/test_scaffold.py || { echo 'missing integration scaffold test'; exit 1; }
printf 'verify-test-taxonomy PASS
'
