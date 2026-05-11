#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
strict=false
if [ "${1:-}" = "--strict" ]; then
  strict=true
elif [ "${1:-}" != "" ]; then
  echo 'usage: scripts/verify-source-topology.sh [--strict]' >&2
  exit 2
fi
python3 - "$strict" <<'PY'
from pathlib import Path
import re
import sys
root = Path('.')
strict = sys.argv[1] == 'true'
plan = (root / 'plan.toml').read_text()
errors = []
for needle in ['[full_codebase]', 'module_surfaces_exact = 50', 'rust_modules_exact = 46', 'ops_surfaces_exact = 4', 'fleet_mode = "claude-code-arena-style"']:
    if needle not in plan:
        errors.append(f'missing full-codebase authority marker: {needle}')
layer_docs = sorted((root / 'ai_docs/layers').glob('L*.md'))
if len(layer_docs) != 7:
    errors.append(f'expected 7 layer docs, found {len(layer_docs)}')
planned = re.findall(r'\[\[planned_modules\]\]\s+id = "([^"]+)"\s+name = "([^"]+)"\s+layer = "([^"]+)"\s+cluster = "([^"]+)"\s+source_path = "([^"]+)"', plan)
if len(planned) != 50:
    errors.append(f'expected 50 planned modules, found {len(planned)}')
clusters = re.findall(r'\[\[full_codebase_clusters\]\]', plan)
if len(clusters) != 9:
    errors.append(f'expected 9 full-codebase synergy clusters, found {len(clusters)}')
cluster_doc = (root / 'ai_docs/CLUSTERED_MODULES.md').read_text()
code_map = (root / 'ai_docs/CODE_MODULE_MAP.md').read_text()
for module_id, name, layer, cluster, source_path in planned:
    for doc_name, doc_text in [('CLUSTERED_MODULES.md', cluster_doc), ('CODE_MODULE_MAP.md', code_map)]:
        if module_id not in doc_text or name not in doc_text or cluster not in doc_text:
            errors.append(f'{doc_name} missing planned module linkage for {module_id} {name} {cluster}')
    if strict and not (root / source_path).exists():
        errors.append(f'strict source missing for {module_id}: {source_path}')
if strict:
    missing_tests = []
    for module_id, name, layer, cluster, source_path in planned[:46]:
        source = root / source_path
        if source.exists():
            content = source.read_text(errors='ignore')
            if '#[cfg(test)]' not in content and 'mod tests' not in content:
                missing_tests.append(f'{module_id}:{source_path}')
    if missing_tests:
        errors.append('strict test modules missing: ' + ', '.join(missing_tests[:20]))
if errors:
    print('verify-source-topology FAIL')
    for error in errors:
        print(f'- {error}')
    sys.exit(1)
mode = 'strict' if strict else 'planning'
print(f'verify-source-topology PASS ({mode})')
print('planned_module_surfaces=50 rust_modules=46 ops_surfaces=4 clusters=9 layers=7')
if not strict:
    print('strict_status=FULL_CODEBASE_TOPOLOGY_INCOMPLETE until all planned source paths and tests exist')
PY
