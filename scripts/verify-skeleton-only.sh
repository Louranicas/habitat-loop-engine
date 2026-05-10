#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

ROOT = Path('.')
CAP = 40
violations: list[str] = []

plan = (ROOT / 'plan.toml').read_text()
if 'm0_runtime = false' not in plan:
    raise SystemExit('skeleton-only LOC cap requires plan.toml m0_runtime = false')

for path in sorted((ROOT / 'crates').rglob('*.rs')):
    lines = path.read_text().splitlines()
    logical = [
        line for line in lines
        if line.strip() and not line.strip().startswith('//') and not line.strip().startswith('///')
    ]
    if len(logical) > CAP:
        violations.append(f'{path}: {len(logical)} logical LOC exceeds skeleton cap {CAP}')

if violations:
    raise SystemExit('Rust skeleton LOC cap violation:\n' + '\n'.join(violations))

print('verify-skeleton-only PASS')
PY
