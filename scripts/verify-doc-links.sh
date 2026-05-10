#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
import re
root=Path('.')
missing=[]
for md in root.rglob('*.md'):
    if '.deployment-work/scratch' in md.as_posix():
        continue
    for target in re.findall(r'\[([^\]]+)\]\(([^)]+)\)', md.read_text(errors='ignore')):
        href=target[1]
        if '://' in href or href.startswith('#'):
            continue
        path=(md.parent / href.split('#')[0]).resolve()
        if not path.exists():
            missing.append((md.as_posix(), href))
if missing:
    raise SystemExit('broken markdown links: '+str(missing[:10]))
print('verify-doc-links PASS')
PY
