# C09 DevOps / QI Operational Lane — Cluster Overview

> **Cluster:** C09_DEVOPS_QI_OPERATIONAL_LANE | **Modules:** M051-M054 (4 modules) | **Layer:** L06 / scripts
> **LOC (script):** ~600 across 4 shell scripts | **Status:** IMPLEMENTED (operational surface)
> **Synergy:** scripts enforce docs-source parity and prevent quiet topology collapse

---

## Purpose

C09 is unique among the 9 clusters: its modules are **not Rust source files**. They are operational shell scripts that already exist and run in production as part of `scripts/quality-gate.sh`. This cluster documents them as first-class modules so the predicate-to-script mapping in `docs/SCRIPT_SPEC_PREDICATE_MAP.md` is reflected in the planned topology census.

The four C09 modules are the **gate-load-bearing scripts** — the ones the framework's §17.13 "scaffold-readiness" predicate cannot pass without:

- `verify-sync` — root-file inventory and authority-map alignment
- `quality-gate` — orchestrator of the entire 27-step canonical pipeline
- `verify-module-map` — module marker presence in the code-map
- `verify-layer-dag` — layer graph integrity

Each script has a 1:1 wrapper at `bin/hle-*` (parity enforced by `scripts/verify-bin-wrapper-parity.sh`).

---

## File Map

```
scripts/
├── verify-sync.sh        # M051 — required root files, S01-S13 specs, 7 layer docs, M001-M004 markers
├── quality-gate.sh       # M052 — orchestrator; emits hle.quality_gate.v2 JSON
├── verify-module-map.sh  # M053 — M001-M004 markers in CODE_MODULE_MAP and plan.toml
└── verify-layer-dag.sh   # M054 — layer graph topology and acyclicity

bin/   (1:1 wrappers, all execute the corresponding scripts/ files)
├── hle-verify-sync
├── hle-quality-gate
├── hle-module-map
└── hle-layer-dag
```

---

## Dependency Graph (script-level)

```
quality-gate.sh (M052)  ──invokes──→  verify-sync.sh (M051)
                        ──invokes──→  verify-module-map.sh (M053)
                        ──invokes──→  verify-layer-dag.sh (M054)
                        ──invokes──→  18 OTHER scripts/verify-*.sh
                        ──invokes──→  cargo {fmt,check,test,clippy}
                        ──invokes──→  python3 tests/{unit,integration}/*.py
```

`verify-sync`, `verify-module-map`, `verify-layer-dag` run as independent steps inside the orchestrated chain. They do not depend on each other; ordering is for predictable failure reporting.

---

## Cross-Cluster Dependencies

| C09 Module | Reads From | Provides Evidence For |
|---|---|---|
| M051 verify-sync | plan.toml, ULTRAMAP.md, ai_specs/, ai_docs/layers/ | All other scripts (presupposes valid root inventory) |
| M052 quality-gate | All 22 verify-* scripts + cargo + python | C04 false_pass_auditor (M024) reads gate JSON to detect missing anchored fields |
| M053 verify-module-map | ai_docs/CODE_MODULE_MAP.md, plan.toml | Authoritative map for C01/C02/C03/C04/C05/C06/C07/C08 module presence |
| M054 verify-layer-dag | schematics/layer-dag.md, ai_docs/layers/ | Layer integrity proof for the entire 7-layer topology |

---

## Design Principles

1. **Bash + python3 only — no compiled artifacts.** Scripts must work without a build step.
2. **Bounded output.** Each step emits a JSON status row with `exit_code`, `status`, `duration_ms`. No unbounded stdout dumps.
3. **No `set -e` short-circuit when accumulating drift is more useful** (per framework §17.9 script safety). Use explicit branching.
4. **JSON-first reporting.** Human-readable PASS lines are advisory; the verdict is `verdict: "PASS"` in the JSON.
5. **Wrapper parity.** Every script in `scripts/verify-*.sh` MUST have a corresponding `bin/hle-*` wrapper. `verify-bin-wrapper-parity.sh` enforces this; orphan wrappers also fail.
6. **Read-only.** Scripts never mutate source, configs, or receipts. Only `quality-gate.sh` writes to `.deployment-work/status/` (for its own JSON output).
7. **No network, no service starts.** `verify-script-safety.sh` enforces this; any `curl`, `wget`, `systemctl`, `cron` invocation in a script fails the safety gate.

---

## Error Strategy

Scripts use shell exit codes only — no error code range like the Rust clusters. Standard convention:

| Exit Code | Meaning |
|---:|---|
| 0 | PASS |
| 1 | FAIL (predicate violated; see stderr / JSON for detail) |
| 2 | Usage error (bad flags / missing args) |
| ≥ 3 | Reserved for future strict-mode failures |

`quality-gate.sh` aggregates: any non-zero step exit → overall `verdict: "FAIL"`.

---

## Quality Gate Expectations

| Predicate | Authority |
|---|---|
| Every script in `scripts/verify-*.sh` is referenced by `quality-gate.sh` | `verify-bin-wrapper-parity.sh` (sibling) + `docs/SCRIPT_SPEC_PREDICATE_MAP.md` |
| Every script has a `bin/hle-*` wrapper | `verify-bin-wrapper-parity.sh` |
| No script invokes `set -e` without `# hle: allow-set-e` justification | `verify-script-safety.sh` |
| No script performs network fetches, service starts, package installs, or cron operations | `verify-script-safety.sh` |
| All script output bounded ≤ 65,536 bytes by default | `verify-bounded-logs.sh` |
| `main "$@"` invocation present in every script | `verify-script-safety.sh` |

---

## When This Cluster's Scripts Change

Per the framework's bidirectional cross-reference rule:

1. Update the per-module spec sheet (M051-M054) in this directory.
2. Update `docs/SCRIPT_SPEC_PREDICATE_MAP.md` with the predicate change.
3. Update `runbooks/verification-and-sync-pipeline.md` family-group table.
4. Refresh `SHA256SUMS.txt` and rerun the canonical sequence.
5. Confirm `bin/hle-*` wrapper still matches; add or remove as needed.

---

## Cluster Invariants

- C09 modules are **operational scripts, not Rust source**. They are listed in the planned 50-module topology because the framework treats QI scripts as first-class authority surfaces.
- **No M0 implementation logic** runs in C09 scripts — they are pure observers/verifiers.
- C09 scripts are the **only modules that mutate `.deployment-work/status/`** (specifically, `quality-gate.sh` writes its JSON output there).
- `quality-gate.sh` is the **single entry point** for the 27-step canonical sequence; calling individual `scripts/verify-*.sh` is allowed for debugging but not authoritative for PASS claims.

---

## Cross-references

- Predicate map: `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md`
- Operator runbook: `../../runbooks/verification-and-sync-pipeline.md`
- Framework Atuin/QI section: deployment-framework `§8` and `§17.9`
- Wrapper parity: `../../scripts/verify-bin-wrapper-parity.sh`

---

*C09 DevOps/QI Cluster Overview v1.0 | 2026-05-11*
