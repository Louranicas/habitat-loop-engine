# Habitat Loop Engine

Status: M0 local runtime ACTIVE. 4 of 50 planned modules built (M001-M004 = substrate-types/substrate-verify/substrate-emit/hle-cli); the remaining 46 are declared in `plan.toml [[planned_modules]]` with `[full_codebase] status = "planned_topology_incomplete"`. Both `--scaffold` and `--m0` quality gates PASS as of 2026-05-10 (`.deployment-work/status/scaffold-status.json`). Live Habitat integrations and cron/daemon operation remain disabled. The codebase needs to be 'one shotted': every runtime path must be bounded, foreground, explicit, and finite.

Mission: provide a Habitat-grade local workflow loop engine with executor/verifier separation, durable receipts, runbook-aware awaiting-human semantics, and substrate-ready evidence trails.

## Current boundary

This repository now contains bounded M0 local loop surfaces: a local CLI, verifier-authorized receipts, bounded helper scripts, and scaffold/M0 quality gates. It must be operated as a one-shot codebase only: no hidden loops, no persistent workers, no background services, and no repeated autonomous execution unless a later authorization explicitly expands scope. It must not perform live Habitat writes, cron/daemon installation, or deployment claims without later authorization.

## Quick commands

```bash
scripts/verify-sync.sh
scripts/verify-doc-links.sh
scripts/verify-claude-folder.sh
scripts/verify-antipattern-registry.sh
scripts/verify-module-map.sh
scripts/verify-layer-dag.sh
scripts/verify-receipt-schema.sh
scripts/verify-negative-controls.sh
scripts/verify-runbook-schema.sh
scripts/verify-receipt-graph.sh
scripts/verify-test-taxonomy.sh
scripts/verify-bounded-logs.sh
scripts/verify-script-safety.sh
scripts/verify-local-loop-helpers.sh
scripts/quality-gate.sh --scaffold
scripts/quality-gate.sh --scaffold --json
```

See [Script / Spec / Predicate Map](docs/SCRIPT_SPEC_PREDICATE_MAP.md) for the scaffold predicate chain, split receipt hash anchors, and CI/Watcher JSON report contract.

## Verification & Sync Pipeline

22 verify scripts + 4 cargo lanes + 2 python lanes orchestrated by `scripts/quality-gate.sh`. Each verify script has a 1:1 wrapper at `bin/hle-*` (parity enforced by `scripts/verify-bin-wrapper-parity.sh`). PASS requires every step `exit_code == 0` AND `status == PASS` AND non-vacuity floors satisfied.

**Family groups** (full predicate map: [`docs/SCRIPT_SPEC_PREDICATE_MAP.md`](docs/SCRIPT_SPEC_PREDICATE_MAP.md); operator runbook: [`runbooks/verification-and-sync-pipeline.md`](runbooks/verification-and-sync-pipeline.md)):

- **Sync & topology** — `verify-sync`, `verify-module-map`, `verify-layer-dag`, `verify-source-topology`
- **Documentation** — `verify-doc-links`, `verify-vault-parity`
- **Claude/operator** — `verify-claude-folder`, `verify-bin-wrapper-parity`
- **Pattern intelligence** — `verify-antipattern-registry`, `verify-usepattern-registry`, `verify-semantic-predicates`
- **Evidence & receipts** — `verify-receipt-schema`, `verify-receipt-graph`, `verify-framework-hash-freshness`
- **Safety & negative controls** — `verify-negative-controls`, `verify-bounded-logs`, `verify-script-safety`, `verify-skeleton-only`, `verify-test-taxonomy`
- **Local M0** — `verify-local-loop-helpers`, `verify-m0-runtime`, `verify-runbook-schema`

**Cargo + Python lanes:** `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `python3 tests/unit/test_manifest.py`, `python3 tests/integration/test_scaffold.py`.

Status JSON lands in `.deployment-work/status/quality-gate-*.json` and is the only authority for PASS claims; prose alone is insufficient.

## Architecture

- L01 Foundation
- L02 Persistence
- L03 Workflow Executor
- L04 Verification
- L05 Dispatch Bridges
- L06 CLI
- L07 Runbook Semantics

## Planned Topology (50-module clustered codebase)

The full codebase target is 50 modules across 9 clusters (C01-C09), enumerated in `plan.toml [[planned_modules]]`. Today, M001-M004 are built; M005-M054 are declared planned with `[full_codebase] status = "planned_topology_incomplete"`. The strict topology gate `scripts/verify-source-topology.sh` enforces this expansion when M0 implementation begins.

| Cluster | Layers | Crate(s) | Synergy role | Status |
|---|---|---|---|---|
| C01 Evidence Integrity | L01/L02/L04 | hle-core, hle-storage, hle-verifier | receipt hash → claim store → receipt store → SHA verifier → final claim evaluator | planned |
| C02 Authority & State | L01/L03/L04 | hle-core, hle-executor, hle-verifier | type-state authority preventing executor self-certification | planned |
| C03 Bounded Execution | L03 | hle-executor | bounded output/timeout/retry primitives, one-shot local runner | planned |
| C04 Anti-Pattern Intelligence | L01/L02/L04 | hle-core, hle-storage, hle-verifier | catalogued anti-patterns become executable scanner events | planned |
| C05 Persistence Ledger | L02 | hle-storage | append-only ledger of runs/ticks/evidence/results/blockers | planned |
| C06 Runbook Semantics | L07 | hle-runbook | typed runbook workflow definition with AwaitingHuman semantics | planned |
| C07 Dispatch Bridges | L05 | hle-bridge | Zellij/Atuin/DevOps-V3/STcortex/Watcher; read-only until M2+ | planned |
| C08 CLI Surface | L06 | hle-cli (extended) | typed adapters over executor/verifier/runbook authority | planned |
| C09 DevOps/QI Lane | L06/scripts | scripts/ | scripts enforce docs-source parity (canonical scripts already exist) | partial |

M-ID renumbering note: existing `[[modules]]` keeps M001-M004 (substrate-types/verify/emit/hle-cli); the planned table will renumber from M001-M050 to M005-M054 to resolve the latent ID collision when expansion lands. Implementation is gated by the `begin M0` phrase per `loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`.

## Non-goals before live-integration authorization

- no live Habitat writes;
- no cron/daemon creation;
- no production executor loop or unbounded background worker;
- no final deployment claim;
- no S13/live-substrate execution PASS without explicit future round-trip verifier evidence.

## Link map

- [Master Index](MASTER_INDEX.md) — single navigational entry point (start here)
- [Quickstart](QUICKSTART.md)
- [Architecture](ARCHITECTURE.md)
- [UltraMap](ULTRAMAP.md)
- [Plan](plan.toml)
- [Quality Bar](QUALITY_BAR.md)
- [Harness Contract](HARNESS_CONTRACT.md)
- [Claude project authority](CLAUDE.md)
- [Claude local operator overlay](CLAUDE.local.md)
- [**Module Specs Index**](ai_specs/modules/INDEX.md) — 50 module specs across 9 clusters (MEv2 L1 gold standard, authored 2026-05-11)
- [Verification & Sync Pipeline runbook](runbooks/verification-and-sync-pipeline.md)
- [Predicate / Spec / Script map](docs/SCRIPT_SPEC_PREDICATE_MAP.md)


## End-to-end stack deployment cross-reference chain

Treat this repository as a single bidirectional deployment graph while the full end-to-end stack of the codebase is deployed. Every change that touches an operator surface, layer contract, module contract, or source crate must cross-reference the adjacent surfaces in both directions before it is complete.

Canonical chain:

1. `CLAUDE.local.md` — local operator overlay and start-here authority.
2. `README.md` — human entry point and mission/boundary summary.
3. `QUICKSTART.md` — executable one-shot operator command path.
4. Dedicated Obsidian vault — review and knowledge graph mirror: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`.
5. `ULTRAMAP.md` — layer/module/source alignment authority.
6. `ai_docs/layers/L*.md` — L01-L07 layer contracts.
7. `ai_docs/modules/M*.md` — M001-M004 module contracts.
8. `crates/*/src/*.rs` — source implementation authority.

Required cross-reference rule: when any node in this chain changes, inspect the previous and next node, update reciprocal links or notes if behavior/authority changed, then refresh manifests and rerun the relevant scaffold/M0 gates. Do not claim the stack is deployed from a single surface; deployment claims require agreement across the whole chain plus receipts.

Primary forward links:

- README: `README.md`
- Quickstart: `QUICKSTART.md`
- Obsidian HOME: `obsidian://open?vault=habitat-loop-engine&file=HOME`
- Obsidian HOME path: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- UltraMap: `ULTRAMAP.md`
- Layer docs: `ai_docs/layers/L01_FOUNDATION.md`, `ai_docs/layers/L02_PERSISTENCE.md`, `ai_docs/layers/L03_WORKFLOW_EXECUTOR.md`, `ai_docs/layers/L04_VERIFICATION.md`, `ai_docs/layers/L05_DISPATCH_BRIDGES.md`, `ai_docs/layers/L06_CLI.md`, `ai_docs/layers/L07_RUNBOOK_SEMANTICS.md`
- Module docs: `ai_docs/modules/M001_SUBSTRATE_TYPES.md`, `ai_docs/modules/M002_SUBSTRATE_VERIFY.md`, `ai_docs/modules/M003_SUBSTRATE_EMIT.md`, `ai_docs/modules/M004_HLE_CLI.md`
- Source crates: `crates/substrate-types/src/lib.rs`, `crates/substrate-verify/src/lib.rs`, `crates/substrate-emit/src/lib.rs`, `crates/hle-cli/src/main.rs`

## Coding readiness and knowledge links

The scaffold is ready for the next authorized coding slice inside bounded local M0. It is not cleared for live Habitat write integrations, cron/systemd/service installation, or final deployment claims.

Layer/module/source sync authority:

| Module | Layer | Source | Main docs |
|---|---|---|---|
| M001_SUBSTRATE_TYPES | L01 | `crates/substrate-types/src/lib.rs` | `ai_docs/modules/M001_SUBSTRATE_TYPES.md`, `ai_docs/layers/L01_FOUNDATION.md` |
| M002_SUBSTRATE_VERIFY | L04/L07 | `crates/substrate-verify/src/lib.rs` | `ai_docs/modules/M002_SUBSTRATE_VERIFY.md`, `ai_docs/layers/L04_VERIFICATION.md` |
| M003_SUBSTRATE_EMIT | L02/L03/L07 | `crates/substrate-emit/src/lib.rs` | `ai_docs/modules/M003_SUBSTRATE_EMIT.md`, `ai_docs/layers/L03_WORKFLOW_EXECUTOR.md` |
| M004_HLE_CLI | L06 | `crates/hle-cli/src/main.rs` | `ai_docs/modules/M004_HLE_CLI.md`, `ai_docs/layers/L06_CLI.md` |

Bidirectional knowledge links:

- Obsidian HOME: `obsidian://open?vault=habitat-loop-engine&file=HOME`
- Obsidian HOME path: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- Obsidian Master Index: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/MASTER_INDEX.md`
- Obsidian Scaffold Operator Quickstart: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/00 Index/Scaffold Operator Quickstart.md`
- STcortex namespace: `hle`
- STcortex anchors: `hle:coding-readiness-sync`, `hle:local-m0-boundary`, `hle:verification-gates`, `hle:directory-md-comprehensiveness-matrix`

Reciprocal Obsidian backlinks are maintained in the dedicated vault HOME, root Master Index, nested Master Index, and Scaffold Operator Quickstart so vault readers can navigate back to this repository, README, QUICKSTART, ULTRAMAP, CLAUDE.md, and CLAUDE.local.md.

When changing README/QUICKSTART link text, also update the reciprocal blocks in the dedicated vault and refresh both repository and vault manifests.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a root or topic documentation surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `README.md`.
- Parent directory: `.`.
- Adjacent markdown siblings sampled: ARCHITECTURE.md, CHANGELOG.md, CLAUDE.local.md, CLAUDE.md, CODEOWNERS.md, HARNESS_CONTRACT.md, QUALITY_BAR.md, QUICKSTART.md.
- This file should be read with `plan.toml`, `ULTRAMAP.md`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, and `.deployment-work/status/scaffold-status.json` when deciding whether a change is scaffold-only, local-M0, or outside authorization.

### Verification hooks
- Baseline scaffold gate: `scripts/quality-gate.sh --scaffold --json`.
- Local-M0 gate: `scripts/quality-gate.sh --m0 --json`.
- Manifest authority: `sha256sum -c SHA256SUMS.txt` after every documentation or status edit.
- For vault/framework-only edits, refresh the appropriate vault/framework manifest before declaring closure.

### Acceptance criteria
- The document names its role, boundary, and verification surface clearly.
- Claims about PASS/FAIL are backed by verifier output or receipts, not prose alone.
- Any runtime behavior described here remains local-only unless a later authorization receipt explicitly expands scope.
- Future agents can identify which files to inspect next without guessing hidden context.

### Failure modes
- Treat vague "complete", "ready", or "deployed" wording as insufficient unless it points to gates, manifests, and receipts.
- Do not infer live integration permission from local-M0 wording.
- Do not create background services or recurring jobs from this document alone.
- If this file drifts from `plan.toml` or `ULTRAMAP.md`, update the authority files first and rerun gates.

### Next maintenance action
On the next broadening pass, re-run the markdown census, inspect files with fewer than 180 words or missing boundary/verification terms, update this section with any new authority roots, then refresh manifests and rerun both quality gates.

