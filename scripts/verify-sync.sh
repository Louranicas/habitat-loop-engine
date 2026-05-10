#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
root=Path('.')
required=[
 'README.md','QUICKSTART.md','ARCHITECTURE.md','ULTRAMAP.md','plan.toml','QUALITY_BAR.md','CODEOWNERS.md','CHANGELOG.md','CLAUDE.md','CLAUDE.local.md','Cargo.toml','HARNESS_CONTRACT.md','vault/CONVENTIONS.md'
]
missing=[p for p in required if not (root/p).exists()]
if missing:
    raise SystemExit('missing root files: '+', '.join(missing))
if (root/'PLAN.toml').exists():
    raise SystemExit('uppercase PLAN.toml is forbidden')
if len(list((root/'ai_specs').glob('S*.md'))) != 13:
    raise SystemExit('expected exactly 13 ai_specs')
if len(list((root/'ai_docs/layers').glob('L*.md'))) != 7:
    raise SystemExit('expected exactly 7 layer docs')
for marker in ['M001','M002','M003','M004']:
    if marker not in (root/'plan.toml').read_text() or marker not in (root/'ULTRAMAP.md').read_text():
        raise SystemExit(f'module marker missing from plan or ultramap: {marker}')
print('verify-sync PASS')
PY
