#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
import json
bad=json.loads(Path('tests/fixtures/negative/vacuous-status.json').read_text())
if set(['m0_authorized','live_integrations_authorized']).issubset(bad):
    raise SystemExit('negative status unexpectedly has required fields')
bad_receipt=Path('tests/fixtures/negative/missing-anchored-receipt.md').read_text()
if '^Verdict:' in bad_receipt and '^Manifest_sha256:' in bad_receipt and '^Framework_sha256:' in bad_receipt:
    raise SystemExit('negative receipt unexpectedly anchored')
print('verify-negative-controls PASS')
PY
