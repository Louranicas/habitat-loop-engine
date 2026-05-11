#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

ROOT = Path('.')
CAP = 40
violations: list[str] = []

plan = (ROOT / 'plan.toml').read_text()
if 'm0_runtime = true' in plan:
    print('verify-skeleton-only SKIP: M0 runtime authorized')
    raise SystemExit(0)
if 'm0_runtime = false' not in plan:
    raise SystemExit('skeleton-only LOC cap requires explicit m0_runtime flag')

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
