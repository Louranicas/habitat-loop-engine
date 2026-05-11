# M053 module_map_script — verify-module-map.sh

> **File:** `scripts/verify-module-map.sh` | **LOC:** ~10 (pure bash) | **Wrapper:** `bin/hle-module-map`
> **Role:** asserts M001-M004 module markers exist in both `ai_docs/CODE_MODULE_MAP.md` and `plan.toml`

---

## Predicate at a Glance

| Check | Authority Surface | Failure Signature |
|---|---|---|
| `M001` substring in CODE_MODULE_MAP.md | Loop iteration | `missing M001 in CODE_MODULE_MAP` |
| `M001` substring in plan.toml | Loop iteration | `missing M001 in plan` |
| Repeat for M002, M003, M004 | Loop iteration | `missing M00<N> in plan/CODE_MODULE_MAP` |

PASS output: `verify-module-map PASS` (single line).

---

## Invocation

```bash
scripts/verify-module-map.sh
# or via wrapper:
bin/hle-module-map
```

No arguments. Exit code 0 = PASS, 1 = FAIL.

---

## Implementation

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for m in M001 M002 M003 M004; do
  grep -q "$m" ai_docs/CODE_MODULE_MAP.md || { echo "missing $m in CODE_MODULE_MAP"; exit 1; }
  grep -q "$m" plan.toml                  || { echo "missing $m in plan"; exit 1; }
done
printf 'verify-module-map PASS\n'
```

Pure bash, no python3. Substring grep only — does not validate `[[modules]]` table structure or column count.

---

## Counter-Examples

| Setup | Expected exit | Expected message |
|---|---|---|
| Delete M002 row from CODE_MODULE_MAP.md | 1 | `missing M002 in CODE_MODULE_MAP` |
| Delete M003 row from plan.toml | 1 | `missing M003 in plan` |
| Both files intact | 0 | `verify-module-map PASS` |

---

## Limitations & Future Hardening

- **Hardcoded to M001-M004.** Will continue passing when the M005-M054 expansion lands (M005-M054 are checked by `verify-source-topology.sh` instead).
- **Substring match only.** A line `id = "M001"` in `[[planned_modules]]` (different namespace) satisfies the check even if the `[[modules]]` row is missing. This is acceptable because `verify-sync.sh` (M051) already checks both tables.
- **No column-level validation.** `verify-source-topology.sh` covers (id, name, cluster) triple matching for the planned table.

---

## When to Extend

If/when the existing 4 substrate-* crates are absorbed into the planned topology (per `CLUSTERED_MODULES.md`'s "Legacy M0 crate surfaces" framing), this script's hardcoded loop will need either:
- Removal (if M001-M004 collide with planned IDs)
- Renumbering (e.g., M-LEG-001..004 for legacy substrates)
- Replacement by `verify-source-topology.sh --strict` as the sole module-map authority

The decision is **gated by `begin M0`** — until then, M001-M004 stay as-is and this script keeps passing.

---

## Cluster Invariants

- C09 invariant: read-only, no network, bounded output.
- This script is the **module marker non-vacuity floor** — without it, `[[modules]]` table drift is silent.

---

## Cross-references

- Cluster overview: `00-CLUSTER-OVERVIEW.md`
- Sibling: M054 verify-layer-dag.sh (layer marker check)
- Planned-topology counterpart: `../../scripts/verify-source-topology.sh` (covers M005-M054)
- Predicate map: `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md` § "Sync and topology"
- Wrapper: `../../bin/hle-module-map`

---

*M053 module_map_script Spec v1.0 | 2026-05-11*
