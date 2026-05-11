# M051 verify_sync_script — verify-sync.sh

> **File:** `scripts/verify-sync.sh` | **LOC:** ~25 (bash + embedded python3) | **Wrapper:** `bin/hle-verify-sync`
> **Role:** root-file inventory and authority-map alignment — the first predicate any agent must satisfy

---

## Predicate at a Glance

| Check | Authority Surface | Failure Signature |
|---|---|---|
| Required root files exist | 13 files: README.md, QUICKSTART.md, ARCHITECTURE.md, ULTRAMAP.md, plan.toml, QUALITY_BAR.md, CODEOWNERS.md, CHANGELOG.md, CLAUDE.md, CLAUDE.local.md, Cargo.toml, HARNESS_CONTRACT.md, vault/CONVENTIONS.md | `missing root files: <list>` |
| `plan.toml` casing | Lowercase only — `PLAN.toml` rejected | `uppercase PLAN.toml is forbidden` |
| 13 ai_specs S*.md files exist | Top-level non-recursive glob | `expected exactly 13 ai_specs` |
| 7 ai_docs/layers/L*.md files exist | Top-level non-recursive glob | `expected exactly 7 layer docs` |
| M001-M004 markers in plan.toml AND ULTRAMAP.md | Hardcoded loop over four module IDs | `module marker missing from plan or ultramap: M00<N>` |

PASS output: `verify-sync PASS` (single line to stdout).

---

## Invocation

```bash
scripts/verify-sync.sh
# or via wrapper:
bin/hle-verify-sync
```

No arguments. Exit code 0 = PASS, 1 = FAIL.

---

## Implementation Strategy

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
root = Path('.')
required = [
    'README.md', 'QUICKSTART.md', 'ARCHITECTURE.md', 'ULTRAMAP.md',
    'plan.toml', 'QUALITY_BAR.md', 'CODEOWNERS.md', 'CHANGELOG.md',
    'CLAUDE.md', 'CLAUDE.local.md', 'Cargo.toml', 'HARNESS_CONTRACT.md',
    'vault/CONVENTIONS.md',
]
missing = [p for p in required if not (root / p).exists()]
if missing:
    raise SystemExit('missing root files: ' + ', '.join(missing))
if (root / 'PLAN.toml').exists():
    raise SystemExit('uppercase PLAN.toml is forbidden')
if len(list((root / 'ai_specs').glob('S*.md'))) != 13:
    raise SystemExit('expected exactly 13 ai_specs')
if len(list((root / 'ai_docs/layers').glob('L*.md'))) != 7:
    raise SystemExit('expected exactly 7 layer docs')
for marker in ['M001', 'M002', 'M003', 'M004']:
    if (marker not in (root / 'plan.toml').read_text()
        or marker not in (root / 'ULTRAMAP.md').read_text()):
        raise SystemExit(f'module marker missing from plan or ultramap: {marker}')
print('verify-sync PASS')
PY
```

Embedded python3 because:
- pathlib globs are exact and acyclic
- list-of-required-files is data, not control flow
- Single SystemExit message on first violation gives operators a clean signal

---

## Counter-Examples (negative controls)

| Setup | Expected exit | Expected message |
|---|---|---|
| Delete `README.md` | 1 | `missing root files: README.md` |
| Create `PLAN.toml` | 1 | `uppercase PLAN.toml is forbidden` |
| Add 14th ai_specs/S99_X.md | 1 | `expected exactly 13 ai_specs` |
| Remove M002 from plan.toml | 1 | `module marker missing from plan or ultramap: M002` |
| All correct | 0 | `verify-sync PASS` |

---

## Limitations & Future Hardening

- **Hardcoded M001-M004 list.** When the M005-M054 expansion lands, this loop must extend (or read from `plan.toml [[modules]]` dynamically). Today it greps for substring presence only — a `[[planned_modules]]` row with `id = "M001"` (different namespace) would still satisfy the check.
- **Glob is non-recursive.** Adding `ai_specs/modules/M005_*.md` doesn't break the count check (good), but also doesn't get verified by this script (it's verified by `verify-source-topology.sh`).
- **No content validation.** Files just need to exist; their content is verified by other scripts (`verify-doc-links`, `verify-receipt-schema`, etc.).

---

## Cluster Invariants

- C09 invariant: read-only, no network, no service starts, bounded output.
- This script's PASS line is the **first non-vacuity floor** (`root_docs_min = 8` per framework §17.9). Without it, the entire gate is meaningless.
- The script's hardcoded list of 13 required files matches the "Phase D scaffold contract" from the deployment framework §4.1 / §17.3.

---

## Cross-references

- Cluster overview: `00-CLUSTER-OVERVIEW.md`
- Predicate map: `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md` § "Sync and topology"
- Pipeline runbook: `../../runbooks/verification-and-sync-pipeline.md` § "Sync & topology"
- Wrapper: `../../bin/hle-verify-sync`

---

*M051 verify_sync_script Spec v1.0 | 2026-05-11*
