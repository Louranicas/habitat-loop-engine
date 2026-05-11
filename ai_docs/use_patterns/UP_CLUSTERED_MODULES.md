# UP_CLUSTERED_MODULES

Status: scaffold use-pattern contract. Module clustering is a specification discipline, not a claim that every future module is implemented.

Predicate ID: `HLE-UP-006`

## Intent

The Loop Engine should prefer a small number of real, cohesive modules over many shallow descriptor files. Clusters group responsibilities by authority boundary, not by aesthetic symmetry.

## Scaffold clusters

1. Authority and planning: plan schema, authorization boundary, M0/M1/M2 cuts.
2. Substrate types: neutral records shared across emission and verification.
3. Emission contracts: future JSONL/receipt view generation without verifier authority.
4. Verification contracts: negative controls, hash checks, receipt graph validation.
5. Human operations: runbooks, Kanban orchestration, awaiting-human states.
6. Habitat integration doctrine: read-only integration maps for DevOps, Zellij, Atuin, and future live surfaces.

## Good pattern

A module is acceptable when it owns a clear invariant, has a documented dependency direction, and can be tested or reviewed independently.

## Bad pattern

Creating forty modules that only restate names without invariants is composition drift. Count-based completeness is not a scaffold PASS.

## Review checklist

- Does the cluster have a real invariant?
- Does it avoid cross-linking executor and verifier authority?
- Is the dependency direction documented in `schematics/layer-dag.md` or `schematics/module-graph.md`?
- Can a future test or verifier predicate observe drift?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a use-pattern contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/use_patterns/UP_CLUSTERED_MODULES.md`.
- Parent directory: `ai_docs/use_patterns`.
- Adjacent markdown siblings sampled: INDEX.md, UP_ATUIN_QI_CHAIN.md, UP_BOUNDED_OUTPUT.md, UP_EXECUTOR_VERIFIER_SPLIT.md, UP_RECEIPT_GRAPH.md, UP_RUNBOOK_AWAITING_HUMAN.md.
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

