# Full Codebase Deployment Cluster Synergy Plan

> For Hermes: use Claude Code fleet orchestration to implement this plan cluster-by-cluster. Do not hand-edit a four-module minimum and call it stack completion.

Goal: realign Habitat Loop Engine from the current four-module local-M0 subset to the deployment framework target of 7 layers and ~50 behavior-bearing module surfaces.

Architecture: keep local-M0 one-shot safety and no live-write/no-daemon boundaries, but build the full source topology as bounded local crates. Use verifier authority and source-topology gates as blockers before any deployment claim.

## Framework target

- 7 layers: L01 Foundation, L02 Persistence, L03 Workflow Executor, L04 Verification, L05 Dispatch Bridges, L06 CLI, L07 Runbook Semantics.
- 50 planned module surfaces: 46 Rust/source modules plus 4 operational/QI script surfaces.
- Current repo is incomplete: 4 legacy M0 crate surfaces only.

## Synergy-first implementation waves

### C01_EVIDENCE_INTEGRITY — Evidence Integrity

- Layers: L01/L02/L04
- Planned surfaces: 5
- Synergy: receipt hash -> claim store -> receipt store -> verifier recompute -> final claim evaluator.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C02_AUTHORITY_STATE — Authority and State

- Layers: L01/L03/L04
- Planned surfaces: 5
- Synergy: type-state authority and transition table prevent executor self-certification.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C03_BOUNDED_EXECUTION — Bounded Execution

- Layers: L03
- Planned surfaces: 5
- Synergy: local runner + phase executor + timeout/retry policies make every runtime path finite and verifier-visible.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C04_ANTI_PATTERN_INTELLIGENCE — Anti-Pattern Intelligence

- Layers: L01/L02/L04
- Planned surfaces: 5
- Synergy: catalogued anti-patterns become scanner events, test taxonomy checks, and false-pass audits.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C05_PERSISTENCE_LEDGER — Persistence Ledger

- Layers: L02
- Planned surfaces: 7
- Synergy: schema-first ledger ties runs, ticks, evidence, verifier results, and blockers into append-only proof.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C06_RUNBOOK_SEMANTICS — Runbook Semantics

- Layers: L07
- Planned surfaces: 8
- Synergy: incident-response runbooks reuse workflow authority instead of becoming a parallel engine.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C07_DISPATCH_BRIDGES — Dispatch Bridges

- Layers: L05
- Planned surfaces: 6
- Synergy: Zellij/Atuin/DevOps/STcortex/Watcher bridges share contract parity and read-only/live-write gates.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C08_CLI_SURFACE — CLI Surface

- Layers: L06
- Planned surfaces: 5
- Synergy: operator commands remain thin typed adapters over executor/verifier/runbook authority.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

### C09_DEVOPS_QI_OPERATIONAL_LANE — DevOps/QI Operational Lane

- Layers: L06/scripts
- Planned surfaces: 4
- Synergy: scripts enforce docs-source-gate parity and prevent quiet topology collapse.
- Implementation rule: create source + tests first, then update docs; no final PASS from executor-owned code.
- Claude Code assignment: one bounded worktree/session for this cluster, followed by a separate read-only reviewer.

## Gates

1. `scripts/verify-source-topology.sh` must pass in planning mode after this correction.
2. `scripts/verify-source-topology.sh --strict` must pass before full-codebase completion is claimed.
3. `scripts/quality-gate.sh --full --json` is the deployment-framework gate; `--scaffold` and `--m0` are subset gates only.
4. Full closure requires independent Claude Code/Hermes review and exact test counts.
