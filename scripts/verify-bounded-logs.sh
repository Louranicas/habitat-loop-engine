#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
from pathlib import Path
patterns = ['tail -f', 'while true', 'yes ', 'cat /dev/zero', '/dev/random']
violations = []
for base in [Path('scripts'), Path('.claude'), Path('bin')]:
    for path in base.rglob('*'):
        if not path.is_file() or path.name == 'verify-bounded-logs.sh':
            continue
        text = path.read_text(errors='ignore')
        for idx, line in enumerate(text.splitlines(), 1):
            if any(pattern in line for pattern in patterns):
                violations.append(f'{path}:{idx}:{line}')
if violations:
    raise SystemExit('unbounded output/log pattern found:\n' + '\n'.join(violations))
print('verify-bounded-logs PASS')
PY