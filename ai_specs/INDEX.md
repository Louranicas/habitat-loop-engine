# AI Specs Index

S01-S13 are canonical.

- [S01 SYSTEM_OVERVIEW_AND_AUTHORITY_BOUNDARY](./S01_SYSTEM_OVERVIEW_AND_AUTHORITY_BOUNDARY.md)
- [S02 WORKFLOW_DEFINITION_MODEL](./S02_WORKFLOW_DEFINITION_MODEL.md)
- [S03 EXECUTOR_STATE_MACHINE](./S03_EXECUTOR_STATE_MACHINE.md)
- [S04 VERIFIER_AND_RECEIPT_AUTHORITY](./S04_VERIFIER_AND_RECEIPT_AUTHORITY.md)
- [S05 PERSISTENCE_LEDGER_AND_SQLITE_SCHEMA](./S05_PERSISTENCE_LEDGER_AND_SQLITE_SCHEMA.md)
- [S06 CLI_AND_LOCAL_OPERATION_SURFACE](./S06_CLI_AND_LOCAL_OPERATION_SURFACE.md)
- [S07 ANTI_USE_PATTERN_INTELLIGENCE](./S07_ANTI_USE_PATTERN_INTELLIGENCE.md)
- [S08 ATUIN_QI_VERIFICATION_CHAIN](./S08_ATUIN_QI_VERIFICATION_CHAIN.md)
- [S09 DEVOPS_V3_READ_ONLY_INTEGRATION](./S09_DEVOPS_V3_READ_ONLY_INTEGRATION.md)
- [S10 RUNBOOK_SEMANTICS_AND_AWAITING_HUMAN_FSM](./S10_RUNBOOK_SEMANTICS_AND_AWAITING_HUMAN_FSM.md)
- [S11 ORCHESTRATOR_ZELLIJ_HANDOFF_DISCIPLINE](./S11_ORCHESTRATOR_ZELLIJ_HANDOFF_DISCIPLINE.md)
- [S12 RUNTIME_LOOP_AND_ROLLBACK_SEMANTICS](./S12_RUNTIME_LOOP_AND_ROLLBACK_SEMANTICS.md)
- [S13 JSONL_SUBSTRATE_INTEGRATION_CONTRACT](./S13_JSONL_SUBSTRATE_INTEGRATION_CONTRACT.md)

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a AI specification contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_specs/INDEX.md`.
- Parent directory: `ai_specs`.
- Adjacent markdown siblings sampled: S01_SYSTEM_OVERVIEW_AND_AUTHORITY_BOUNDARY.md, S02_WORKFLOW_DEFINITION_MODEL.md, S03_EXECUTOR_STATE_MACHINE.md, S04_VERIFIER_AND_RECEIPT_AUTHORITY.md, S05_PERSISTENCE_LEDGER_AND_SQLITE_SCHEMA.md, S06_CLI_AND_LOCAL_OPERATION_SURFACE.md, S07_ANTI_USE_PATTERN_INTELLIGENCE.md, S08_ATUIN_QI_VERIFICATION_CHAIN.md.
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

