# Habitat Loop Engine — Master Index

> **Status:** M0 local runtime ACTIVE · 4 of 50 modules built (M001-M004) · 50 spec sheets authored (M005-M054 planned topology) · scaffold + M0 quality gates PASS.
> **Last updated:** 2026-05-11.
> **Authority chain:** `CLAUDE.local.md` → `README.md` → `QUICKSTART.md` → `ULTRAMAP.md` → `ai_docs/layers/` → `ai_docs/modules/` → `ai_specs/modules/` → `crates/*/src/`.

This index is the single navigational entry point for the Habitat Loop Engine codebase. Every link below resolves; every linked surface back-links to this Master Index.

---

## Quick Operator Path

| Need | File | Notes |
|---|---|---|
| Start here | [`CLAUDE.local.md`](CLAUDE.local.md) | Local operator overlay, current authorization boundary |
| Mission + status | [`README.md`](README.md) | One-line mission, Verification & Sync Pipeline, Planned Topology table |
| Run commands | [`QUICKSTART.md`](QUICKSTART.md) | Verification pipeline, Planned coding readiness |
| Layer/module/source map | [`ULTRAMAP.md`](ULTRAMAP.md) | Source-of-truth alignment table |
| Architecture rules | [`ARCHITECTURE.md`](ARCHITECTURE.md) | 7-layer DAG + forbidden edges |
| Quality bar | [`QUALITY_BAR.md`](QUALITY_BAR.md) | MEv2 L1 gold standard reference |
| Harness contract | [`HARNESS_CONTRACT.md`](HARNESS_CONTRACT.md) | Receipt anchored fields, S13 substrate |
| Plan source-of-truth | [`plan.toml`](plan.toml) | Machine-readable: layers, modules, planned_modules, clusters |
| Latest scaffold receipt | [`.deployment-work/receipts/`](.deployment-work/receipts/) | Authorization receipts |
| Latest gate verdict | [`.deployment-work/status/scaffold-status.json`](.deployment-work/status/scaffold-status.json) | PASS/FAIL of last canonical sequence |

---

## Spec & Doc Surfaces

### ai_specs/ — formal specifications

- [`ai_specs/INDEX.md`](ai_specs/INDEX.md) — S01-S13 topic-level specs index
- [`ai_specs/modules/INDEX.md`](ai_specs/modules/INDEX.md) — **50 module specs across 9 clusters (MEv2 L1 gold standard)**
  - [C01 Evidence Integrity](ai_specs/modules/c01-evidence-integrity/00-CLUSTER-OVERVIEW.md) · M005-M009
  - [C02 Authority & State](ai_specs/modules/c02-authority-state/00-CLUSTER-OVERVIEW.md) · M010-M014
  - [C03 Bounded Execution](ai_specs/modules/c03-bounded-execution/00-CLUSTER-OVERVIEW.md) · M015-M019
  - [C04 Anti-Pattern Intelligence](ai_specs/modules/c04-anti-pattern-intelligence/00-CLUSTER-OVERVIEW.md) · M020-M024
  - [C05 Persistence Ledger](ai_specs/modules/c05-persistence-ledger/00-CLUSTER-OVERVIEW.md) · M025-M031
  - [C06 Runbook Semantics](ai_specs/modules/c06-runbook-semantics/00-CLUSTER-OVERVIEW.md) · M032-M039
  - [C07 Dispatch Bridges](ai_specs/modules/c07-dispatch-bridges/00-CLUSTER-OVERVIEW.md) · M040-M045
  - [C08 CLI Surface](ai_specs/modules/c08-cli-surface/00-CLUSTER-OVERVIEW.md) · M046-M050
  - [C09 DevOps/QI Lane](ai_specs/modules/c09-devops-qi-lane/00-CLUSTER-OVERVIEW.md) · M051-M054

### ai_docs/ — narrative documentation

- [`ai_docs/layers/`](ai_docs/layers/) — L01_FOUNDATION..L07_RUNBOOK_SEMANTICS
- [`ai_docs/modules/`](ai_docs/modules/) — M001-M004 module sheets (existing); M005-M054 land at `start coding`
- [`ai_docs/CLUSTERED_MODULES.md`](ai_docs/CLUSTERED_MODULES.md) — 9-cluster synergy map
- [`ai_docs/CODE_MODULE_MAP.md`](ai_docs/CODE_MODULE_MAP.md) — full 50-module status map
- [`ai_docs/anti_patterns/`](ai_docs/anti_patterns/) — AP28/AP29/AP31/C6/C7/C12/C13/FP_FALSE_PASS_CLASSES
- [`ai_docs/use_patterns/`](ai_docs/use_patterns/) — UP_EXECUTOR_VERIFIER_SPLIT / UP_RECEIPT_GRAPH / UP_BOUNDED_OUTPUT / UP_ATUIN_QI_CHAIN / UP_RUNBOOK_AWAITING_HUMAN / UP_CLUSTERED_MODULES

### Operator runbooks

- [`runbooks/INDEX.md`](runbooks/INDEX.md)
- [`runbooks/m0-authorization-boundary.md`](runbooks/m0-authorization-boundary.md) — allowed/forbidden runtime; planned topology authorization scope
- [`runbooks/scaffold-verification.md`](runbooks/scaffold-verification.md) — canonical verification flow
- [`runbooks/verification-and-sync-pipeline.md`](runbooks/verification-and-sync-pipeline.md) — full predicate-by-predicate map (22 verify scripts + 4 cargo + 2 python)

### Schematics

- [`schematics/INDEX.md`](schematics/INDEX.md) — system-overview, layer-dag, module-graph, executor-verifier-sequence, receipt-graph, sqlite-er, anti-pattern-decision-tree, atuin-qi-chain, devops-v3-integration-flow, runbook-awaiting-human-fsm, zellij-orchestrator-deployment-flow

### Schemas (machine contracts)

- [`schemas/receipt.schema.json`](schemas/receipt.schema.json) — `^Verdict / ^Manifest_sha256 / ^Framework_sha256 / ^Counter_evidence_locator`
- [`schemas/status.schema.json`](schemas/status.schema.json) — scaffold-status fields
- [`schemas/plan.schema.json`](schemas/plan.schema.json) — plan.toml shape

---

## Source Crates (current 4)

| Module | Layer | Crate | Source |
|---|---|---|---|
| M001 | L01 | substrate-types | [`crates/substrate-types/src/lib.rs`](crates/substrate-types/src/lib.rs) |
| M002 | L04 | substrate-verify | [`crates/substrate-verify/src/lib.rs`](crates/substrate-verify/src/lib.rs) |
| M003 | L02/L03/L07 | substrate-emit | [`crates/substrate-emit/src/lib.rs`](crates/substrate-emit/src/lib.rs) |
| M004 | L06 | hle-cli | [`crates/hle-cli/src/main.rs`](crates/hle-cli/src/main.rs) |

Planned crates (M005-M054, gated by `begin M0`): `hle-core`, `hle-storage`, `hle-executor`, `hle-verifier`, `hle-runbook`, `hle-bridge`, plus extension of `hle-cli`.

---

## Verification & Sync Pipeline

22 verify scripts + 4 cargo lanes + 2 python lanes orchestrated by [`scripts/quality-gate.sh`](scripts/quality-gate.sh). Each verify script has a 1:1 wrapper at [`bin/hle-*`](bin/).

- **Predicate map:** [`docs/SCRIPT_SPEC_PREDICATE_MAP.md`](docs/SCRIPT_SPEC_PREDICATE_MAP.md)
- **Pipeline runbook:** [`runbooks/verification-and-sync-pipeline.md`](runbooks/verification-and-sync-pipeline.md)
- **Quality gate orchestrator spec:** [`ai_specs/modules/c09-devops-qi-lane/M052_QUALITY_GATE_SCRIPT.md`](ai_specs/modules/c09-devops-qi-lane/M052_QUALITY_GATE_SCRIPT.md)

---

## External Authority Surfaces

| Root | Role |
|---|---|
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/` | Source deployment framework, receipts, handoffs, authorization packet, false-100 traps |
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/` | Dedicated Obsidian review vault (HOME.md + 17 sub-indexes) |
| `/home/louranicas/claude-code-workspace/the_maintenance_engine_v2/` | MEv2 — gold standard exemplar for spec sheet pattern (referenced by [`QUALITY_BAR.md`](QUALITY_BAR.md)) |
| stcortex `hle` namespace at `127.0.0.1:3000` | Pioneer memory substrate; anchors: `hle:coding-readiness-sync`, `hle:local-m0-boundary`, `hle:verification-gates`, `hle:module-specs-authored-2026-05-11` |

---

## Authorization Status

- ✅ `begin scaffold` (2026-05-09T23:52:44Z) — substrate scaffold authorized
- ✅ Local M0 active (2026-05-10) — both `--scaffold` and `--m0` quality gates PASS
- ⏳ `begin M0` — NOT issued; required for M005-M054 implementation logic
- ❌ Live Habitat write integrations — forbidden under any current phrase
- ❌ Cron / systemd / unbounded daemons — forbidden under any current phrase

Phrase registry: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`

---

## Bidirectional Anchor Discipline

Every linked surface above carries a back-reference to this Master Index. When any surface is edited, the back-reference must be preserved or refreshed in the same commit (per `CLAUDE.local.md` "End-to-end stack deployment cross-reference chain").

---

*Master Index v1.0 | 2026-05-11 | round-trip from any vault note: `> Back to: [[CLAUDE.md]] · [[CLAUDE.local.md]] · [[MASTER_INDEX]]`*
