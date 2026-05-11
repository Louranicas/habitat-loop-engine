#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

ROOT = Path('.')
plan = (ROOT / 'plan.toml').read_text()
if 'm0_runtime = true' not in plan:
    raise SystemExit('M0 runtime verification requires plan.toml m0_runtime = true')
for forbidden_enabled in ['live_integrations = true', 'cron_daemons = true']:
    if forbidden_enabled in plan:
        raise SystemExit(f'M0 runtime must remain local-only; forbidden authorization: {forbidden_enabled}')
required = [
    'crates/substrate-types/src/lib.rs',
    'crates/substrate-verify/src/lib.rs',
    'crates/substrate-emit/src/lib.rs',
    'crates/hle-cli/src/main.rs',
    'migrations/0001_scaffold_schema.sql',
    'examples/workflow.example.toml',
]
missing = [path for path in required if not (ROOT / path).exists()]
if missing:
    raise SystemExit('missing M0 runtime files:\n' + '\n'.join(missing))
cli = (ROOT / 'crates/hle-cli/src/main.rs').read_text()
for needle in ['hle run', 'hle verify', "codebase needs to be 'one shotted'", 'daemon command requires --once', 'execute_local_workflow']:
    if needle not in cli:
        raise SystemExit(f'M0 CLI/runtime marker missing: {needle}')
for doc_path in ['README.md', 'CLAUDE.md', 'ULTRAMAP.md', 'docs/SCRIPT_SPEC_PREDICATE_MAP.md', 'ai_specs/S06_CLI_AND_LOCAL_OPERATION_SURFACE.md']:
    if "codebase needs to be 'one shotted'" not in (ROOT / doc_path).read_text():
        raise SystemExit(f'one-shot codebase authority phrase missing: {doc_path}')
verify = (ROOT / 'crates/substrate-verify/src/lib.rs').read_text()
for needle in ['verify_authorization', 'verify_step', 'AWAITING_HUMAN']:
    if needle not in verify:
        raise SystemExit(f'M0 verifier marker missing: {needle}')
emit = (ROOT / 'crates/substrate-emit/src/lib.rs').read_text()
for needle in ['append_jsonl_receipt', 'receipt_to_json_line', 'execute_local_workflow']:
    if needle not in emit:
        raise SystemExit(f'M0 emitter marker missing: {needle}')
schema = (ROOT / 'migrations/0001_scaffold_schema.sql').read_text()
for needle in ['workflow_runs', 'step_receipts', 'REFERENCES workflow_runs']:
    if needle not in schema:
        raise SystemExit(f'M0 SQLite schema marker missing: {needle}')
print('verify-m0-runtime PASS')
PY
