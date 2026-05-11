#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

json=false
mode=""
for arg in "$@"; do
  case "$arg" in
    --scaffold) mode="scaffold" ;;
    --m0) mode="m0" ;;
    --full) mode="full" ;;
    --json) json=true ;;
    *) echo 'usage: scripts/quality-gate.sh (--scaffold|--m0|--full) [--json]' >&2; exit 2 ;;
  esac
done

if [ -z "$mode" ]; then
  echo 'usage: scripts/quality-gate.sh (--scaffold|--m0|--full) [--json]' >&2
  exit 2
fi

steps=(
  'scripts/verify-sync.sh'
  'scripts/verify-doc-links.sh'
  'scripts/verify-claude-folder.sh'
  'scripts/verify-antipattern-registry.sh'
  'scripts/verify-semantic-predicates.sh'
  'scripts/verify-module-map.sh'
  'scripts/verify-layer-dag.sh'
  'scripts/verify-receipt-schema.sh'
  'scripts/verify-negative-controls.sh'
  'scripts/verify-runbook-schema.sh'
  'scripts/verify-receipt-graph.sh'
  'scripts/verify-test-taxonomy.sh'
  'scripts/verify-bounded-logs.sh'
  'scripts/verify-usepattern-registry.sh'
  'scripts/verify-skeleton-only.sh'
  'scripts/verify-framework-hash-freshness.sh'
  'scripts/verify-vault-parity.sh'
  'scripts/verify-bin-wrapper-parity.sh'
  'scripts/verify-script-safety.sh'
  'scripts/verify-local-loop-helpers.sh'
  'scripts/verify-source-topology.sh'
)

if [ "$mode" = "m0" ] || [ "$mode" = "full" ]; then
  m0_gate_dir="${TMPDIR:-/tmp}/hle-m0-quality-gate-$$"
  mkdir -p "$m0_gate_dir"
  trap 'find "$m0_gate_dir" -type f -delete 2>/dev/null; rmdir "$m0_gate_dir" 2>/dev/null || true' EXIT
  steps+=(
    'scripts/verify-m0-runtime.sh'
    'cargo run -q -p hle-cli -- --version'
    "cargo run -q -p hle-cli -- run --workflow examples/workflow.example.toml --ledger $m0_gate_dir/m0-example-ledger.jsonl"
    "cargo run -q -p hle-cli -- verify --ledger $m0_gate_dir/m0-example-ledger.jsonl"
    "cargo run -q -p hle-cli -- daemon --once --workflow examples/workflow.example.toml --ledger $m0_gate_dir/m0-daemon-ledger.jsonl"
  )
fi

if [ "$mode" = "full" ]; then
  steps+=(
    'scripts/verify-source-topology.sh --strict'
  )
fi

steps+=(
  'cargo fmt --check'
  'cargo check --workspace --all-targets'
  'cargo test --workspace --all-targets'
  'cargo clippy --workspace --all-targets -- -D warnings'
  'python3 tests/unit/test_manifest.py'
  'python3 tests/integration/test_scaffold.py'
)

if [ "$json" = true ]; then
  python3 - "$mode" "${steps[@]}" <<'PY'
import json
import subprocess
import sys
import time

mode = sys.argv[1]
steps = sys.argv[2:]
report = {
    'tool': 'scripts/quality-gate.sh',
    'mode': mode,
    'schema': 'hle.quality_gate.v2',
    'verdict': 'PASS',
    'steps': [],
}
exit_code = 0
for command in steps:
    started = time.time()
    result = subprocess.run(command, shell=True, text=True, capture_output=True, check=False)
    duration_ms = int((time.time() - started) * 1000)
    if result.stdout:
        sys.stderr.write(result.stdout)
    if result.stderr:
        sys.stderr.write(result.stderr)
    step = {
        'command': command,
        'exit_code': result.returncode,
        'status': 'PASS' if result.returncode == 0 else 'FAIL',
        'duration_ms': duration_ms,
    }
    report['steps'].append(step)
    if result.returncode != 0:
        report['verdict'] = 'FAIL'
        exit_code = result.returncode
        break
json.dump(report, sys.stdout, indent=2)
sys.stdout.write('\n')
sys.exit(exit_code)
PY
else
  for command in "${steps[@]}"; do
    bash -o pipefail -c "$command"
  done
  printf 'quality-gate --%s PASS\n' "$mode"
fi
