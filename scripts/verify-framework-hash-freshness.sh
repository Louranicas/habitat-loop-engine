#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from hashlib import sha256
from pathlib import Path
import re

FRAMEWORK_ROOT = Path('/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework')
manifest = FRAMEWORK_ROOT / 'SHA256SUMS.txt'
receipts = sorted(Path('.deployment-work/receipts').glob('scaffold-authorization-*.md'))

if not FRAMEWORK_ROOT.exists():
    raise SystemExit(f'missing framework root: {FRAMEWORK_ROOT}')
if not manifest.exists():
    raise SystemExit(f'missing framework hash manifest: {manifest}')
if not receipts:
    raise SystemExit('missing scaffold authorization receipt')

receipt = receipts[-1]
text = receipt.read_text()
found = re.search(r'^\^Framework_sha256:\s*([0-9a-f]{64})\s*$', text, re.MULTILINE)
if not found:
    legacy = re.search(r'^\^Source_sha256:\s*([0-9a-f]{64})\s*$', text, re.MULTILINE)
    if not legacy:
        raise SystemExit(f'{receipt} missing valid ^Framework_sha256')
    framework_hash = legacy.group(1)
else:
    framework_hash = found.group(1)

manifest_entries: dict[str, str] = {}
for line in manifest.read_text().splitlines():
    parts = line.split(maxsplit=1)
    if len(parts) == 2 and re.fullmatch(r'[0-9a-f]{64}', parts[0]):
        manifest_entries[parts[1].strip()] = parts[0]

matches = [rel for rel, digest in manifest_entries.items() if digest == framework_hash]
if not matches:
    raise SystemExit(f'{receipt} framework hash is not present in current framework SHA256SUMS.txt: {framework_hash}')

stale: list[str] = []
for rel in matches:
    path = FRAMEWORK_ROOT / rel.removeprefix('./')
    if not path.exists():
        stale.append(f'{rel}: manifest target missing')
        continue
    actual = sha256(path.read_bytes()).hexdigest()
    if actual != framework_hash:
        stale.append(f'{rel}: receipt={framework_hash} actual={actual}')

if stale:
    raise SystemExit('framework source hash stale:\n' + '\n'.join(stale))

print('verify-framework-hash-freshness PASS')
PY
