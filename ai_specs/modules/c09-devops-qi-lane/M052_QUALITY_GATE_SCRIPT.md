# M052 quality_gate_script — quality-gate.sh

> **File:** `scripts/quality-gate.sh` | **LOC:** ~250 (bash + embedded python3) | **Wrapper:** `bin/hle-quality-gate`
> **Role:** orchestrator of the 27-step canonical verification sequence; emits `hle.quality_gate.v2` JSON

---

## Modes

| Flag | Steps | Purpose |
|---|---|---|
| `--scaffold` | 27 (21 verify + 4 cargo + 2 python) | Canonical baseline; runs everything except `verify-m0-runtime` |
| `--m0` | 28 (adds `verify-m0-runtime`) | Asserts M0 runtime files exist when `m0_runtime = true` in plan.toml |
| `--json` | (modifier) | Emits `hle.quality_gate.v2` JSON to stdout; logs to stderr |

---

## JSON Output Schema (`hle.quality_gate.v2`)

```json
{
  "tool": "scripts/quality-gate.sh",
  "mode": "scaffold" | "m0",
  "schema": "hle.quality_gate.v2",
  "verdict": "PASS" | "FAIL" | "AWAITING_HUMAN",
  "steps": [
    {
      "command": "scripts/verify-sync.sh",
      "exit_code": 0,
      "status": "PASS" | "FAIL" | "SKIP",
      "duration_ms": 21,
      "skip_reason": "<optional, only when status=SKIP>"
    }
  ]
}
```

PASS requires every `step.status == "PASS"` AND every `step.exit_code == 0` AND non-vacuity floors satisfied (see framework §17.9).

---

## Invocation

```bash
scripts/quality-gate.sh --scaffold --json | tee .deployment-work/status/quality-gate-scaffold-latest.json
scripts/quality-gate.sh --m0       --json | tee .deployment-work/status/quality-gate-m0-latest.json
# or via wrapper:
bin/hle-quality-gate --scaffold --json
```

Without `--json`, emits human-readable PASS lines (informational only — never the authority).

---

## Canonical Sequence (scaffold mode)

```
01. verify-sync                 (M051)
02. verify-doc-links
03. verify-claude-folder
04. verify-antipattern-registry
05. verify-semantic-predicates
06. verify-module-map           (M053)
07. verify-layer-dag            (M054)
08. verify-receipt-schema
09. verify-negative-controls
10. verify-runbook-schema
11. verify-receipt-graph
12. verify-test-taxonomy
13. verify-bounded-logs
14. verify-usepattern-registry
15. verify-skeleton-only        (SKIPS when m0_runtime = true)
16. verify-framework-hash-freshness
17. verify-vault-parity
18. verify-bin-wrapper-parity
19. verify-script-safety
20. verify-local-loop-helpers
21. verify-source-topology
22. cargo fmt --check
23. cargo check --workspace --all-targets
24. cargo test --workspace --all-targets
25. cargo clippy --workspace --all-targets -- -D warnings
26. python3 tests/unit/test_manifest.py
27. python3 tests/integration/test_scaffold.py
```

In `--m0` mode, `verify-m0-runtime` is inserted (typically at position 21, before topology).

---

## Failure Behavior

- Any step with non-zero exit → that step's `status = "FAIL"` AND overall `verdict = "FAIL"`.
- Subsequent steps **still run** (accumulating drift visibility per framework §17.9 — do NOT use `set -e` short-circuit).
- The JSON contains all steps' results, not just the first failing one.
- Watcher/CI consumers parse the JSON `verdict` field; they do NOT grep stdout PASS lines (those are advisory only).

---

## Non-Vacuity Floors (framework §17.9)

The script emits FAIL even when individual steps PASS if any of these floors are unmet:

| Floor | Threshold |
|---|---|
| `root_docs_min` | 8 |
| `claude_files_min` | 13 |
| `layers_exact` | 7 |
| `specs_exact_min` | 12 |
| `schematics_exact_min` | 13 |
| `runbook_fixtures_exact_min` | 8 |
| `atuin_scripts_min` | 18 |
| `anti_pattern_ids_min` | 21 |
| `use_pattern_ids_min` | 15 |
| `module_sheets_min` | 1 |
| `cluster_docs_min` | 7 |
| `negative_control_layers_exact` | 7 |

(Some floors are advisory in scaffold mode; strict in M0 mode.)

---

## Implementation Sketch

```bash
#!/usr/bin/env bash
set -uo pipefail   # NOT set -e — drift accumulation is signal
cd "$(dirname "$0")/.."

mode="scaffold"
emit_json=false
for arg in "$@"; do
  case "$arg" in
    --scaffold) mode="scaffold" ;;
    --m0)       mode="m0" ;;
    --json)     emit_json=true ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

steps=()  # bash array of {cmd, exit, status, duration}
for cmd in "${VERIFY_SCRIPTS[@]}" "${CARGO_LANES[@]}" "${PYTHON_LANES[@]}"; do
  start=$(date +%s%3N)
  $cmd
  exit=$?
  end=$(date +%s%3N)
  duration=$((end - start))
  status=$([[ $exit -eq 0 ]] && echo PASS || echo FAIL)
  steps+=("{\"command\":\"$cmd\",\"exit_code\":$exit,\"status\":\"$status\",\"duration_ms\":$duration}")
done

if $emit_json; then
  emit_hle_quality_gate_v2_json "${steps[@]}"
fi

# verdict = FAIL if any step != PASS
```

---

## Counter-Examples (negative controls)

| Setup | Expected verdict | Expected JSON |
|---|---|---|
| Break `verify-doc-links` (add bad link) | FAIL | step 02 status=FAIL, exit_code=1 |
| Break `cargo clippy` (add warning) | FAIL | step 25 status=FAIL |
| Break a python test | FAIL | step 26 or 27 status=FAIL |
| Pass an unknown flag | (script exits 2 immediately) | no JSON emitted |

---

## Cluster Invariants

- This script is the **single entry point** for the canonical pipeline. Calling individual `scripts/verify-*.sh` is allowed for debugging but **not authoritative** for PASS claims.
- The JSON output is the **only authoritative PASS evidence**. Prose alone is insufficient (framework FP_FALSE_PASS_CLASSES detector).
- `quality-gate.sh` itself is **read-only** for source; the only file it writes is its own JSON output to `.deployment-work/status/`.

---

## Cross-references

- Cluster overview: `00-CLUSTER-OVERVIEW.md`
- Predicate map: `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md`
- Pipeline runbook: `../../runbooks/verification-and-sync-pipeline.md`
- Receipt schema (for ^Manifest_sha256 / ^Framework_sha256 anchors): `../../schemas/receipt.schema.json`
- C04 false_pass_auditor (M024) consumes the JSON output of this script.

---

*M052 quality_gate_script Spec v1.0 | 2026-05-11*
