#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
import json,re
for name in ['receipt.schema.json','status.schema.json','plan.schema.json']:
    json.loads((Path('schemas')/name).read_text())
for receipt in Path('.deployment-work/receipts').glob('*.md'):
    text=receipt.read_text()
    for field in ['^Verdict:', '^Manifest_sha256:', '^Framework_sha256:', '^Counter_evidence_locator:']:
        if field not in text:
            raise SystemExit(f'{receipt} missing {field}')
    for anchor in ['Manifest_sha256', 'Framework_sha256']:
        found=re.search(rf'\^{anchor}:\s*([0-9a-f]{{64}})', text)
        if not found:
            raise SystemExit(f'{receipt} {anchor} missing/invalid')
    legacy=re.search(r'\^Source_sha256:\s*([0-9a-f]{64})', text)
    if legacy and '^Framework_sha256:' not in text:
        raise SystemExit(f'{receipt} legacy Source_sha256 present without Framework_sha256 split anchor')
print('verify-receipt-schema PASS')
PY
