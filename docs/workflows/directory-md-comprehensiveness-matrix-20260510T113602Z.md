# HLE Directory and Markdown Comprehensiveness Matrix

Created UTC: 2026-05-10T11:36:02Z
Reviewer: Weaver / Hermes
Boundary: documentation/review broadening only; local M0 remains bounded; live integrations and service deployment remain forbidden.

## Purpose
This matrix double-checks every directory and markdown file in the three HLE authority roots after the broadening pass. It records directory coverage, weak-file findings, and verification expectations so a future agent can audit the scaffold/M0 surface without redoing discovery from memory.

## Authority roots
- active_repo: `/home/louranicas/claude-code-workspace/habitat-loop-engine`
- dedicated_vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`
- deployment_framework: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`

## Directory coverage summary

| Root | Directories scanned | Markdown files | Weak markdown files after broadening | Notes |
|---|---:|---:|---:|---|
| active_repo | 48 | 92 | 1 | Only weak file is intentional negative fixture. |
| dedicated_vault | 19 | 47 | 0 | No weak markdown files remain by census threshold. |
| deployment_framework | 4 | 27 | 0 | No weak markdown files remain by census threshold. |

## Directory-by-directory markdown coverage

| Root | Directory | Markdown count | Weak count | Sample files |
|---|---|---:|---:|---|
| active_repo | `.` | 10 | 0 | `CLAUDE.md`, `CODEOWNERS.md`, `ULTRAMAP.md`, `CHANGELOG.md`, `QUALITY_BAR.md`, `QUICKSTART.md`, `README.md`, `ARCHITECTURE.md` |
| active_repo | `.claude` | 2 | 0 | `LOCAL_RULES.md`, `PROJECT_CONTEXT.md` |
| active_repo | `.claude/agents` | 1 | 0 | `scaffold-reviewer.md` |
| active_repo | `.claude/commands` | 2 | 0 | `verify-scaffold.md`, `scaffold-status.md` |
| active_repo | `.claude/hooks` | 0 | 0 | none |
| active_repo | `.claude/rules` | 1 | 0 | `no-m0-before-authorization.md` |
| active_repo | `.deployment-work` | 0 | 0 | none |
| active_repo | `.deployment-work/logs` | 0 | 0 | none |
| active_repo | `.deployment-work/receipts` | 2 | 0 | `scaffold-authorization-20260509T235244Z.md`, `scaffold-placeholder-completion-20260510T020554Z.md` |
| active_repo | `.deployment-work/runtime` | 0 | 0 | none |
| active_repo | `.deployment-work/scratch` | 0 | 0 | none |
| active_repo | `.deployment-work/status` | 1 | 0 | `hermes-scaffold-boundary-status-20260510T110536Z.md` |
| active_repo | `ai_docs` | 3 | 0 | `CODE_MODULE_MAP.md`, `INDEX.md`, `CLUSTERED_MODULES.md` |
| active_repo | `ai_docs/anti_patterns` | 9 | 0 | `C12_UNBOUNDED_COLLECTIONS.md`, `INDEX.md`, `C13_MISSING_BUILDER.md`, `AP31_NESTED_LOCKS.md`, `C6_LOCK_HELD_SIGNAL_EMIT.md`, `AP28_COMPOSITIONAL_INTEGRITY_DRIFT.md`, `FP_FALSE_PASS_CLASSES.md`, `AP29_BLOCKING_IN_ASYNC.md` |
| active_repo | `ai_docs/layers` | 7 | 0 | `L07_RUNBOOK_SEMANTICS.md`, `L04_VERIFICATION.md`, `L03_WORKFLOW_EXECUTOR.md`, `L02_PERSISTENCE.md`, `L06_CLI.md`, `L01_FOUNDATION.md`, `L05_DISPATCH_BRIDGES.md` |
| active_repo | `ai_docs/modules` | 5 | 0 | `M001_SUBSTRATE_TYPES.md`, `INDEX.md`, `M002_SUBSTRATE_VERIFY.md`, `M003_SUBSTRATE_EMIT.md`, `M004_HLE_CLI.md` |
| active_repo | `ai_docs/use_patterns` | 7 | 0 | `UP_RECEIPT_GRAPH.md`, `UP_BOUNDED_OUTPUT.md`, `UP_RUNBOOK_AWAITING_HUMAN.md`, `INDEX.md`, `UP_EXECUTOR_VERIFIER_SPLIT.md`, `UP_ATUIN_QI_CHAIN.md`, `UP_CLUSTERED_MODULES.md` |
| active_repo | `ai_specs` | 14 | 0 | `S10_RUNBOOK_SEMANTICS_AND_AWAITING_HUMAN_FSM.md`, `INDEX.md`, `S09_DEVOPS_V3_READ_ONLY_INTEGRATION.md`, `S02_WORKFLOW_DEFINITION_MODEL.md`, `S04_VERIFIER_AND_RECEIPT_AUTHORITY.md`, `S06_CLI_AND_LOCAL_OPERATION_SURFACE.md`, `S11_ORCHESTRATOR_ZELLIJ_HANDOFF_DISCIPLINE.md`, `S03_EXECUTOR_STATE_MACHINE.md` |
| active_repo | `bin` | 0 | 0 | none |
| active_repo | `crates` | 0 | 0 | none |
| active_repo | `crates/hle-cli` | 0 | 0 | none |
| active_repo | `crates/hle-cli/src` | 0 | 0 | none |
| active_repo | `crates/substrate-emit` | 0 | 0 | none |
| active_repo | `crates/substrate-emit/src` | 0 | 0 | none |
| active_repo | `crates/substrate-types` | 0 | 0 | none |
| active_repo | `crates/substrate-types/src` | 0 | 0 | none |
| active_repo | `crates/substrate-verify` | 0 | 0 | none |
| active_repo | `crates/substrate-verify/src` | 0 | 0 | none |
| active_repo | `docs` | 1 | 0 | `SCRIPT_SPEC_PREDICATE_MAP.md` |
| active_repo | `docs/operations` | 1 | 0 | `SCAFFOLD_RECEIPT.md` |
| active_repo | `docs/plans` | 1 | 0 | `M0_IMPLEMENTATION_PLAN_PLACEHOLDER.md` |
| active_repo | `docs/quality` | 1 | 0 | `semantic-predicates.md` |
| active_repo | `docs/reviews` | 1 | 0 | `watcher-scaffold-assessment-20260510.md` |
| active_repo | `docs/workflows` | 5 | 0 | `scaffold-deployment-mermaid.md`, `scaffold-deployment-workflow.md`, `scaffold-m0-broadening-review-20260510T112840Z.md`, `scaffold-deployment-kanban-map.md`, `scaffold-workflow-kanban-closure.md` |
| active_repo | `etc` | 1 | 0 | `README.md` |
| active_repo | `examples` | 0 | 0 | none |
| active_repo | `local-ci` | 0 | 0 | none |
| active_repo | `migrations` | 0 | 0 | none |
| active_repo | `runbooks` | 3 | 0 | `INDEX.md`, `scaffold-verification.md`, `m0-authorization-boundary.md` |
| active_repo | `schemas` | 0 | 0 | none |
| active_repo | `schematics` | 12 | 0 | `runbook-awaiting-human-fsm.md`, `executor-verifier-sequence.md`, `INDEX.md`, `sqlite-er.md`, `devops-v3-integration-flow.md`, `zellij-orchestrator-deployment-flow.md`, `receipt-graph.md`, `layer-dag.md` |
| active_repo | `scripts` | 0 | 0 | none |
| active_repo | `tests` | 0 | 0 | none |
| active_repo | `tests/fixtures` | 0 | 0 | none |
| active_repo | `tests/fixtures/negative` | 1 | 1 | `missing-anchored-receipt.md` |
| active_repo | `tests/integration` | 0 | 0 | none |
| active_repo | `tests/unit` | 0 | 0 | none |
| active_repo | `vault` | 1 | 0 | `CONVENTIONS.md` |
| dedicated_vault | `.` | 2 | 0 | `HOME.md`, `MASTER_INDEX.md` |
| dedicated_vault | `00 Index` | 4 | 0 | `Master Index.md`, `Scaffold Operator Quickstart.md`, `Layer Index.md`, `Glossary.md` |
| dedicated_vault | `01 Deployment Framework` | 3 | 0 | `Cross-Vault Cartography.md`, `Deployment Phases.md`, `Deployment Framework Overview.md` |
| dedicated_vault | `02 Orchestrator Collaboration` | 4 | 0 | `Orchestrator Collaboration Packet.md`, `Cycle 2 Triad Receipts.md`, `Command Handoffs.md`, `Cycle 3 Deployment Framework Receipts.md` |
| dedicated_vault | `03 Scaffold Contract` | 1 | 0 | `Scaffold Contract.md` |
| dedicated_vault | `04 Claude Folder` | 1 | 0 | `Claude Folder Contract.md` |
| dedicated_vault | `05 Docs Specs Schematics` | 1 | 0 | `Docs Specs Schematics Contract.md` |
| dedicated_vault | `06 DevOps V3 Integration` | 1 | 0 | `DevOps V3 Allocation.md` |
| dedicated_vault | `07 Atuin QI` | 2 | 0 | `Atuin QI Verification Chain.md`, `QI Predicate Catalog.md` |
| dedicated_vault | `08 Module Genesis` | 1 | 0 | `Module Genesis Specs.md` |
| dedicated_vault | `09 Clustered Modules` | 3 | 0 | `Canonical Cluster Taxonomy.md`, `Cluster Index.md`, `Clustered Modules Strategy.md` |
| dedicated_vault | `10 Anti Patterns and Use Patterns` | 3 | 0 | `Anti Pattern Registry.md`, `Use Pattern Registry.md`, `Anti-Pattern Detector Registry.md` |
| dedicated_vault | `11 Runbook Layer` | 2 | 0 | `L7 Runbook Schema.md`, `L7 Runbook Semantics.md` |
| dedicated_vault | `12 Receipts` | 5 | 0 | `Verification Manifest.md`, `Canonical Hash Authority.md`, `Luke Command-2 Waiver.md`, `Phase A Pre-Scaffold Receipts.md`, `Canonical Receipt Status.md` |
| dedicated_vault | `13 Gap Closure` | 4 | 0 | `False-100 Anti-Patterns.md`, `Watcher Scaffold Assessment — 2026-05-10.md`, `Deployment Framework Gap Analysis — 2026-05-09.md`, `Gap Closure Canon.md` |
| dedicated_vault | `14 Runtime Guidelines` | 2 | 0 | `Fresh Context Handoff — Runtime Guidelines.md`, `Orchestration Runtime Guidelines.md` |
| dedicated_vault | `15 Readiness Specs` | 5 | 0 | `Runtime Loop Contract.md`, `JSONL Substrate Integration Contract.md`, `AI Specs Inventory.md`, `Cargo Workspace and Crate DAG.md`, `Persistence Schema Contract.md` |
| dedicated_vault | `16 Authorization` | 1 | 0 | `Authorization Phrases and Waivers.md` |
| dedicated_vault | `17 Workflows` | 2 | 0 | `Scaffold Workflow Kanban Closure — 2026-05-10.md`, `Scaffold Deployment Workflow Preservation.md` |
| deployment_framework | `.` | 5 | 0 | `ORCHESTRATOR_DEPLOYMENT_PACKET.md`, `HABITAT_LOOP_ENGINE_DEPLOYMENT_FRAMEWORK.md`, `FALSE_100_TRAPS.md`, `SCAFFOLD_100_READINESS_RUNBOOK.md`, `AUTHORIZATION_PHRASES.md` |
| deployment_framework | `handoffs` | 4 | 0 | `command.md`, `command-3.md`, `command-2.md`, `claude-code-100-score-review-prompt.md` |
| deployment_framework | `receipts` | 12 | 0 | `weaver-substrate-recommendation-integration-2026-05-10T090243Z.md`, `phase-a-exemplar-assimilation.md`, `scaffold-preflight-blocked-20260509T234646Z.md`, `command-3-docs-runbooks-schematics.md`, `phase-a-genesis-read-receipt.md`, `luke-command-2-waiver-20260509T234830Z.md`, `weaver-100-score-scaffold-readiness-20260509T234154Z.md`, `weaver-deployment-framework-gap-analysis-2026-05-09T222946Z.md` |
| deployment_framework | `subagents` | 6 | 0 | `claude-folder-architect.md`, `claude-code-100-score-review-command-2-verification.md`, `atuin-qi-toolchain-audit.md`, `scaffold-exemplar-audit.md`, `claude-code-100-score-review-command-primary-architect.md`, `claude-code-100-score-review-command-3-docs.md` |

## Weak-file disposition

All non-fixture markdown files now include purpose, authority/boundary, files/directory relationship, verification hooks, acceptance criteria, failure modes, and next maintenance action either natively or through the Weaver 2026-05-10 comprehensiveness block.

| Root | File | Disposition |
|---|---|---|
| active_repo | `tests/fixtures/negative/missing-anchored-receipt.md` | Intentional negative-control fixture; do not broaden or tests lose value. |

## Acceptance criteria
- Every directory in the three roots is represented in the directory coverage table.
- Every non-fixture markdown file has enough context for a fresh agent to identify purpose, authority, boundary, verification, acceptance, failure modes, related files, and next maintenance action.
- The active repo still treats local M0 as bounded and refuses live integrations or service deployment.
- Manifests and gates are refreshed after this matrix is written.

## Verification commands
```bash
cd /home/louranicas/claude-code-workspace/habitat-loop-engine
sha256sum -c SHA256SUMS.txt
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --scaffold --json
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --m0 --json
cd /home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine
sha256sum -c '12 Receipts/VAULT_SHA256SUMS.txt'
cd /home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework
sha256sum -c SHA256SUMS.txt
```
