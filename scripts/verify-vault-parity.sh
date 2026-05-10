#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

VAULT_ROOT = Path('/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine')
EXPECTED_SECTIONS = [
    '00 Index',
    '01 Deployment Framework',
    '02 Orchestrator Collaboration',
    '03 Scaffold Contract',
    '04 Claude Folder',
    '05 Docs Specs Schematics',
    '06 DevOps V3 Integration',
    '07 Atuin QI',
    '08 Module Genesis',
    '09 Clustered Modules',
    '10 Anti Patterns and Use Patterns',
    '11 Runbook Layer',
    '12 Receipts',
    '13 Gap Closure',
    '14 Runtime Guidelines',
    '15 Readiness Specs',
    '16 Authorization',
    '17 Workflows',
]
EXPECTED_FILES = ['HOME.md', 'MASTER_INDEX.md']

if not VAULT_ROOT.exists():
    raise SystemExit(f'missing dedicated vault root: {VAULT_ROOT}')

missing_sections = [name for name in EXPECTED_SECTIONS if not (VAULT_ROOT / name).is_dir()]
missing_files = [name for name in EXPECTED_FILES if not (VAULT_ROOT / name).is_file()]
extra_sections = sorted(
    p.name for p in VAULT_ROOT.iterdir()
    if p.is_dir() and not p.name.startswith('.') and p.name not in EXPECTED_SECTIONS
)

if missing_sections or missing_files or extra_sections:
    details = []
    if missing_sections:
        details.append('missing sections: ' + ', '.join(missing_sections))
    if missing_files:
        details.append('missing files: ' + ', '.join(missing_files))
    if extra_sections:
        details.append('unexpected sections: ' + ', '.join(extra_sections))
    raise SystemExit('vault parity violation: ' + '; '.join(details))

print('verify-vault-parity PASS')
PY
