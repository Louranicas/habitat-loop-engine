#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
from pathlib import Path
import re
patterns = [
    re.compile(r'\brm\s+-rf\b'),
    re.compile(r'\bcrontab\b'),
    re.compile(r'\bsystemctl\b'),
    re.compile(r'\bnohup\b'),
    re.compile(r'\bdisown\b'),
    re.compile(r'\bsetsid\b'),
    re.compile(r'\bcurl\s+'),
    re.compile(r'\bwget\s+'),
    re.compile(r'\bssh\s+'),
    re.compile(r'\bnc\s+'),
    re.compile(r'\bsocat\s+'),
    re.compile(r'python\s+-m\s+http\.server'),
    re.compile(r'\bcargo\s+install\b'),
    re.compile(r'\bapt\s+'),
    re.compile(r'\bsudo\s+'),
]
violations = []
for base in [Path('scripts'), Path('bin'), Path('.claude')]:
    for path in base.rglob('*'):
        if not path.is_file() or path.name == 'verify-script-safety.sh':
            continue
        text = path.read_text(errors='ignore')
        for idx, line in enumerate(text.splitlines(), 1):
            if any(pattern.search(line) for pattern in patterns):
                violations.append(f'{path}:{idx}:{line}')
if violations:
    raise SystemExit('forbidden scaffold script side-effect pattern found:\n' + '\n'.join(violations))
print('verify-script-safety PASS')
PY