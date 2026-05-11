# Module Specs — Index (50 modules across 9 clusters)

> **Status:** ALL 9 CLUSTERS COMPLETE — 59 spec files (9 cluster overviews + 50 module specs) + this index.
> **Authored:** 2026-05-11 by 8 parallel `rust-pro` cluster agents (C01-C08) + Claude (C09).
> **Gold Standard:** MEv2 L1 spec sheet pattern — `/home/louranicas/claude-code-workspace/the_maintenance_engine_v2/ai_specs/m1-foundation-specs/`.
> **Boundary:** Spec authoring is DOCUMENTATION work under the existing `begin scaffold` authorization. No Rust source has been written; that requires `begin M0`.

---

## Reading Protocol

```
QUICK START:    Read 00-CLUSTER-OVERVIEW.md for any cluster you're touching.
WRITING CODE:   Read the per-module spec (M0xx_*.md) before any source change.
CROSS-CLUSTER:  Read the "Cluster invariants" + "Cross-references" sections.
GOLD STANDARD:  /home/louranicas/claude-code-workspace/the_maintenance_engine_v2/ai_specs/m1-foundation-specs/ — the pattern these specs follow.
```

---

## Cluster Map

| Cluster | Layers | Modules | Files | Crates | Synergy Role |
|---|---|---:|---:|---|---|
| [C01 Evidence Integrity](c01-evidence-integrity/00-CLUSTER-OVERVIEW.md) | L01/L02/L04 | M005-M009 | 6 | hle-core, hle-storage, hle-verifier | receipt hash → claim store → receipt store → SHA verifier → final claim evaluator |
| [C02 Authority & State](c02-authority-state/00-CLUSTER-OVERVIEW.md) | L01/L03/L04 | M010-M014 | 6 | hle-core, hle-executor, hle-verifier | type-state authority preventing executor self-certification |
| [C03 Bounded Execution](c03-bounded-execution/00-CLUSTER-OVERVIEW.md) | L03 | M015-M019 | 6 | hle-executor | bounded output/timeout/retry primitives, one-shot local runner |
| [C04 Anti-Pattern Intelligence](c04-anti-pattern-intelligence/00-CLUSTER-OVERVIEW.md) | L01/L02/L04 | M020-M024 | 6 | hle-core, hle-storage, hle-verifier | catalogued anti-patterns become executable scanner events |
| [C05 Persistence Ledger](c05-persistence-ledger/00-CLUSTER-OVERVIEW.md) | L02 | M025-M031 | 8 | hle-storage | append-only ledger of runs/ticks/evidence/results/blockers |
| [C06 Runbook Semantics](c06-runbook-semantics/00-CLUSTER-OVERVIEW.md) | L07 | M032-M039 | 9 | hle-runbook | typed runbook workflow definition with AwaitingHuman semantics |
| [C07 Dispatch Bridges](c07-dispatch-bridges/00-CLUSTER-OVERVIEW.md) | L05 | M040-M045 | 7 | hle-bridge | Zellij/Atuin/DevOps-V3/STcortex/Watcher; read-only until M2+ |
| [C08 CLI Surface](c08-cli-surface/00-CLUSTER-OVERVIEW.md) | L06 | M046-M050 | 6 | hle-cli (extended) | typed adapters over executor/verifier/runbook authority |
| [C09 DevOps/QI Lane](c09-devops-qi-lane/00-CLUSTER-OVERVIEW.md) | L06/scripts | M051-M054 | 5 | scripts/ (existing) | scripts enforce docs-source-gate parity |

---

## Module Index (50 modules)

### C01 — Evidence Integrity
- [M005 receipt_hash](c01-evidence-integrity/M005_RECEIPT_HASH.md) · L01 · canonical receipt hashing; source of all proof identity
- [M006 claims_store](c01-evidence-integrity/M006_CLAIMS_STORE.md) · L01 · claim graph store with provisional/verified/final state anchors
- [M007 receipts_store](c01-evidence-integrity/M007_RECEIPTS_STORE.md) · L02 · append-only receipt persistence
- [M008 receipt_sha_verifier](c01-evidence-integrity/M008_RECEIPT_SHA_VERIFIER.md) · L04 · independent verifier recomputes receipt hashes
- [M009 final_claim_evaluator](c01-evidence-integrity/M009_FINAL_CLAIM_EVALUATOR.md) · L04 · only verifier can promote final claims

### C02 — Authority and State
- [M010 claim_authority](c02-authority-state/M010_CLAIM_AUTHORITY.md) · L01 · type-state authority model
- [M011 workflow_state](c02-authority-state/M011_WORKFLOW_STATE.md) · L01 · workflow state enum and invariants
- [M012 state_machine](c02-authority-state/M012_STATE_MACHINE.md) · L03 · transition executor with verifier-visible events
- [M013 status_transitions](c02-authority-state/M013_STATUS_TRANSITIONS.md) · L03 · transition table and rollback affordances
- [M014 claim_authority_verifier](c02-authority-state/M014_CLAIM_AUTHORITY_VERIFIER.md) · L04 · adversarial check against executor self-certification

### C03 — Bounded Execution
- [M015 bounded](c03-bounded-execution/M015_BOUNDED.md) · L03 · bounded output/time/memory primitives
- [M016 local_runner](c03-bounded-execution/M016_LOCAL_RUNNER.md) · L03 · one-shot local command runner
- [M017 phase_executor](c03-bounded-execution/M017_PHASE_EXECUTOR.md) · L03 · phase-aware step execution
- [M018 timeout_policy](c03-bounded-execution/M018_TIMEOUT_POLICY.md) · L03 · TERM-to-KILL bounded timeout policy
- [M019 retry_policy](c03-bounded-execution/M019_RETRY_POLICY.md) · L03 · explicit bounded retry semantics

### C04 — Anti-Pattern Intelligence
- [M020 anti_pattern_scanner](c04-anti-pattern-intelligence/M020_ANTI_PATTERN_SCANNER.md) · L04 · catalog → executable checks
- [M021 anti_pattern_events](c04-anti-pattern-intelligence/M021_ANTI_PATTERN_EVENTS.md) · L02 · scanner event store
- [M022 test_taxonomy](c04-anti-pattern-intelligence/M022_TEST_TAXONOMY.md) · L01 · behavioral test taxonomy
- [M023 test_taxonomy_verifier](c04-anti-pattern-intelligence/M023_TEST_TAXONOMY_VERIFIER.md) · L04 · rejects vacuous tests
- [M024 false_pass_auditor](c04-anti-pattern-intelligence/M024_FALSE_PASS_AUDITOR.md) · L04 · HLE-SP-001 detector for unanchored PASS claims

### C05 — Persistence Ledger
- [M025 pool](c05-persistence-ledger/M025_POOL.md) · L02 · local database pool + connection discipline
- [M026 migrations](c05-persistence-ledger/M026_MIGRATIONS.md) · L02 · schema-first migration runner
- [M027 workflow_runs](c05-persistence-ledger/M027_WORKFLOW_RUNS.md) · L02 · workflow run table
- [M028 workflow_ticks](c05-persistence-ledger/M028_WORKFLOW_TICKS.md) · L02 · tick ledger for freshness/causality
- [M029 evidence_store](c05-persistence-ledger/M029_EVIDENCE_STORE.md) · L02 · bounded evidence blob/index store
- [M030 verifier_results_store](c05-persistence-ledger/M030_VERIFIER_RESULTS_STORE.md) · L02 · append-only verifier result ledger
- [M031 blockers_store](c05-persistence-ledger/M031_BLOCKERS_STORE.md) · L02 · blocked/awaiting-human persistence

### C06 — Runbook Semantics
- [M032 runbook_schema](c06-runbook-semantics/M032_RUNBOOK_SCHEMA.md) · L07 · typed runbook workflow definition
- [M033 runbook_parser](c06-runbook-semantics/M033_RUNBOOK_PARSER.md) · L07 · parse + validate runbooks
- [M034 runbook_phase_map](c06-runbook-semantics/M034_RUNBOOK_PHASE_MAP.md) · L07 · runbook phases → executor phases
- [M035 runbook_human_confirm](c06-runbook-semantics/M035_RUNBOOK_HUMAN_CONFIRM.md) · L07 · AwaitingHuman semantics
- [M036 runbook_manual_evidence](c06-runbook-semantics/M036_RUNBOOK_MANUAL_EVIDENCE.md) · L07 · manual evidence attachment
- [M037 runbook_scaffold](c06-runbook-semantics/M037_RUNBOOK_SCAFFOLD.md) · L07 · generate incident runbook scaffolds
- [M038 runbook_incident_replay](c06-runbook-semantics/M038_RUNBOOK_INCIDENT_REPLAY.md) · L07 · deterministic replay fixtures
- [M039 runbook_safety_policy](c06-runbook-semantics/M039_RUNBOOK_SAFETY_POLICY.md) · L07 · runbook safety gate

### C07 — Dispatch Bridges
- [M040 bridge_contract](c07-dispatch-bridges/M040_BRIDGE_CONTRACT.md) · L05 · bridge schema/port/path contract
- [M041 zellij_dispatch](c07-dispatch-bridges/M041_ZELLIJ_DISPATCH.md) · L05 · zellij pane dispatch adapter
- [M042 atuin_qi_bridge](c07-dispatch-bridges/M042_ATUIN_QI_BRIDGE.md) · L05 · Atuin QI script chain integration
- [M043 devops_v3_probe](c07-dispatch-bridges/M043_DEVOPS_V3_PROBE.md) · L05 · read-only DevOps V3 probe
- [M044 stcortex_anchor_bridge](c07-dispatch-bridges/M044_STCORTEX_ANCHOR_BRIDGE.md) · L05 · stcortex anchor; read-only until authorized
- [M045 watcher_notice_writer](c07-dispatch-bridges/M045_WATCHER_NOTICE_WRITER.md) · L05 · Watcher/Hermes notification receipts

### C08 — CLI Surface
- [M046 cli_args](c08-cli-surface/M046_CLI_ARGS.md) · L06 · typed CLI argument parser
- [M047 cli_run](c08-cli-surface/M047_CLI_RUN.md) · L06 · `hle run` command
- [M048 cli_verify](c08-cli-surface/M048_CLI_VERIFY.md) · L06 · `hle verify` command
- [M049 cli_daemon_once](c08-cli-surface/M049_CLI_DAEMON_ONCE.md) · L06 · `hle daemon --once` command
- [M050 cli_status](c08-cli-surface/M050_CLI_STATUS.md) · L06 · status + topology report

### C09 — DevOps/QI Operational Lane
- [M051 verify_sync_script](c09-devops-qi-lane/M051_VERIFY_SYNC_SCRIPT.md) · scripts/ · root inventory + authority-map alignment
- [M052 quality_gate_script](c09-devops-qi-lane/M052_QUALITY_GATE_SCRIPT.md) · scripts/ · 27/28-step orchestrator; emits `hle.quality_gate.v2` JSON
- [M053 module_map_script](c09-devops-qi-lane/M053_MODULE_MAP_SCRIPT.md) · scripts/ · M001-M004 marker presence
- [M054 layer_dag_script](c09-devops-qi-lane/M054_LAYER_DAG_SCRIPT.md) · scripts/ · L01-L07 DAG topology

---

## M-ID Numbering Note

Existing `[[modules]]` (currently-built crates) keeps M001-M004 anchored to `substrate-types/substrate-verify/substrate-emit/hle-cli`. Planned topology uses **M005-M054** in these spec sheets per Luke's 2026-05-10 decision. The atomic 3-file renumber of `plan.toml [[planned_modules]]` + `ai_docs/CLUSTERED_MODULES.md` + `ai_docs/CODE_MODULE_MAP.md` from canonical M001-M050 → M005-M054 is queued under `start coding` (Task #2).

---

## Cross-references

- **Module sheets (lite format):** `../../ai_docs/modules/M001-M004*.md` (existing) — when expansion lands, M005-M054 sheets land alongside.
- **Cluster + code-map indexes:** `../../ai_docs/CLUSTERED_MODULES.md`, `../../ai_docs/CODE_MODULE_MAP.md`
- **Layer docs:** `../../ai_docs/layers/L01_*.md` … `L07_*.md`
- **Specs S01-S13 (topical):** `../S01_*.md` … `../S13_*.md` (note: S03/S04/S05/S07/S10 are content-less shells; the module specs in this directory ARE the load-bearing specs going forward)
- **Predicate map:** `../../docs/SCRIPT_SPEC_PREDICATE_MAP.md`
- **Pipeline runbook:** `../../runbooks/verification-and-sync-pipeline.md`
- **Authorization phrases:** `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`
- **Obsidian vault:** `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- **stcortex namespace:** `hle` (anchors: `hle:module-specs-authored`, `hle:cluster-c01..c09-spec`)

---

*Module Specs Index v1.0 | 2026-05-11*
