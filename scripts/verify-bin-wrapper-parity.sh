#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

ROOT = Path('.')
SCRIPTS = ROOT / 'scripts'
BIN = ROOT / 'bin'


def expected_bin_for(script: Path) -> str:
    stem = script.stem
    if stem == 'quality-gate':
        return 'hle-quality-gate'
    if stem == 'verify-sync':
        return 'hle-verify-sync'
    if stem.startswith('verify-'):
        return 'hle-' + stem.removeprefix('verify-')
    raise ValueError(f'unexpected script name: {script}')


def expected_script_for(wrapper: Path) -> str:
    name = wrapper.name.removeprefix('hle-')
    if name == 'quality-gate':
        return 'quality-gate.sh'
    if name == 'verify-sync':
        return 'verify-sync.sh'
    return f'verify-{name}.sh'

scripts = sorted([*SCRIPTS.glob('verify-*.sh'), SCRIPTS / 'quality-gate.sh'])
expected_bins = {expected_bin_for(script): script for script in scripts}
actual_bins = {path.name: path for path in BIN.glob('hle-*') if path.is_file()}

errors: list[str] = []
for bin_name, script in expected_bins.items():
    wrapper = actual_bins.get(bin_name)
    if wrapper is None:
        errors.append(f'missing wrapper {BIN / bin_name} for {script}')
        continue
    expected_line = f'exec "$(dirname "$0")/../{script}" "$@"'
    lines = wrapper.read_text().splitlines()
    if lines != ['#!/usr/bin/env bash', expected_line]:
        errors.append(f'{wrapper} does not exactly delegate to {script}')

for bin_name, wrapper in actual_bins.items():
    script = SCRIPTS / expected_script_for(wrapper)
    if script not in scripts:
        errors.append(f'orphan wrapper {wrapper}: expected script {script} does not exist')

if errors:
    raise SystemExit('bin-wrapper parity violation:\n' + '\n'.join(errors))

print('verify-bin-wrapper-parity PASS')
PY
