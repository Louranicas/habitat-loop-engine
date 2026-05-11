#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
from pathlib import Path

helper = Path('scripts/kanban-hle-workflow-monitor.py')
text = helper.read_text()
errors: list[str] = []

required_fragments = [
    'Bounded read-only Kanban monitor',
    'intentionally not a daemon',
    'does not dispatch, promote, claim,',
    'TASK_IDS = sys.argv[1:]',
    'if not TASK_IDS:',
    'sys.exit(2)',
    'END = time.time() + 60 * 90',
    '["hermes", "kanban", "show", task_id]',
    'time.sleep(60)',
]
for fragment in required_fragments:
    if fragment not in text:
        errors.append(f'missing monitor contract fragment: {fragment}')

for forbidden in [
    'kanban claim',
    'kanban complete',
    'kanban block',
    'kanban create',
    'kanban remove',
    'kanban update',
    'cron' + 'tab',
    'system' + 'ctl',
    'no' + 'hup',
    'dis' + 'own',
    'set' + 'sid',
]:
    if forbidden in text:
        errors.append(f'forbidden local-loop helper side effect pattern: {forbidden}')

if not text.startswith('#!/usr/bin/env python3'):
    errors.append('monitor must keep an explicit python3 shebang')

if errors:
    raise SystemExit('local-loop helper contract violation:\n' + '\n'.join(errors))

print('verify-local-loop-helpers PASS')
PY
