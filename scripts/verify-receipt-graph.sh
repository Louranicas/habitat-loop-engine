#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
receipts=list(Path('.deployment-work/receipts').glob('*.md'))
if not receipts:
    raise SystemExit('no receipts')
for receipt in receipts:
    text=receipt.read_text()
    missing=[f for f in ['^Verdict:', '^Manifest_sha256:', '^Framework_sha256:', '^Counter_evidence_locator:'] if f not in text]
    if missing:
        raise SystemExit(f'{receipt} missing {missing}')
print('verify-receipt-graph PASS')
PY
