# M054 layer_dag_script — verify-layer-dag.sh

> **File:** `scripts/verify-layer-dag.sh` | **LOC:** ~15 (bash) | **Wrapper:** `bin/hle-layer-dag`
> **Role:** asserts the layer DAG (L01-L07) is present and acyclic enough for scaffold review

---

## Predicate at a Glance

| Check | Authority Surface | Failure Signature |
|---|---|---|
| `schematics/layer-dag.md` exists | Filesystem stat | `missing schematics/layer-dag.md` |
| Each of L01..L07 referenced in the DAG file | Substring grep loop | `missing L0<N> in layer-dag.md` |
| `ai_docs/layers/L0<N>_*.md` exists for each layer | Glob check | `missing layer doc L0<N>` |
| No forbidden edges declared (verifier-mutation, dispatch-write, CLI-bypass-verifier, runbook-second-engine) | Substring presence of "Forbidden edges" section | `layer DAG missing forbidden-edges section` |

PASS output: `verify-layer-dag PASS`.

---

## Invocation

```bash
scripts/verify-layer-dag.sh
# or via wrapper:
bin/hle-layer-dag
```

No arguments. Exit 0 = PASS, 1 = FAIL.

---

## Implementation

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

dag="schematics/layer-dag.md"
[[ -f "$dag" ]] || { echo "missing $dag"; exit 1; }

for n in 01 02 03 04 05 06 07; do
  grep -q "L${n}" "$dag" || { echo "missing L${n} in layer-dag.md"; exit 1; }
  ls "ai_docs/layers/L${n}_"*.md >/dev/null 2>&1 \
    || { echo "missing layer doc L${n}"; exit 1; }
done

grep -q "Forbidden edges" "$dag" \
  || { echo "layer DAG missing forbidden-edges section"; exit 1; }

printf 'verify-layer-dag PASS\n'
```

Pure bash; no python3. Validates presence + cross-reference, not graph topology.

---

## Layer DAG (per ARCHITECTURE.md)

```
L01 Foundation ─→ L02 Persistence ─→ L03 Workflow Executor ─→ L04 Verification
       │                                                            │
       ├──→ L05 Dispatch Bridges                                    │
       ├──→ L06 CLI                                                 │
       └──→ L03 ─→ L07 Runbook Semantics ←──────────────────────────┘
```

---

## Forbidden Edges (asserted by this script's existence + manual-review predicate)

1. Verifier MUST NOT call executor mutation paths (HLE-UP-001).
2. Dispatch bridges MUST NOT write live Habitat services during scaffold (framework §3, §17.11).
3. CLI MUST NOT bypass verifier authority (HLE-UP-001).
4. Runbook semantics MUST NOT become a second workflow engine (framework §11 cluster C06 design rule).

This script does NOT statically verify these edges — that would require parsing module imports across all crates. It verifies the SECTION exists in `schematics/layer-dag.md` so any agent editing the DAG must reckon with the forbidden-edges register.

---

## Counter-Examples

| Setup | Expected exit | Expected message |
|---|---|---|
| Delete `schematics/layer-dag.md` | 1 | `missing schematics/layer-dag.md` |
| Remove `L05` references from DAG | 1 | `missing L05 in layer-dag.md` |
| Delete `ai_docs/layers/L03_WORKFLOW_EXECUTOR.md` | 1 | `missing layer doc L03` |
| Delete "Forbidden edges" header from DAG | 1 | `layer DAG missing forbidden-edges section` |
| All correct | 0 | `verify-layer-dag PASS` |

---

## Limitations & Future Hardening

- **No graph parsing.** A future enhancement could parse the DAG into a graph and verify acyclicity at the node level. Currently relies on schematic structure.
- **No import-level forbidden-edge enforcement.** That requires `cargo deny` or a custom syn-based linter (not yet built; would land in C04 anti_pattern_scanner).
- **No layer DAG drift detector vs `plan.toml [[layers]]`.** `verify-sync.sh` covers count (7), not topology.

---

## Cluster Invariants

- C09 invariant: read-only, no network, bounded output.
- This script is the **layer DAG non-vacuity floor**. Without it, schematic drift is silent.
- The 4 forbidden edges above are the **architectural law** of the codebase. Any agent that proposes a layer change touching them must update `schematics/layer-dag.md` AND `ARCHITECTURE.md` simultaneously, then refresh manifest, then run this script.

---

## Cross-references

- Cluster overview: `00-CLUSTER-OVERVIEW.md`
- Layer DAG schematic: `../../schematics/layer-dag.md`
- Architecture authority: `../../ARCHITECTURE.md`
- Layer docs: `../../ai_docs/layers/L01_*.md` … `L07_*.md`
- Predicate map: `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md`
- Wrapper: `../../bin/hle-layer-dag`

---

*M054 layer_dag_script Spec v1.0 | 2026-05-11*
