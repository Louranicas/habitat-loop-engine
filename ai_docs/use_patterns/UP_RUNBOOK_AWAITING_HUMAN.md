# UP_RUNBOOK_AWAITING_HUMAN

Status: scaffold use-pattern contract. Awaiting-human runtime behavior is deferred until `begin M0`; the state machine and review contract are scaffold-authoritative.

Predicate ID: `HLE-UP-005`

## Intent

A Habitat workflow loop must be able to stop safely when it reaches a human decision point. It should preserve context, blockers, candidate actions, and verifier evidence without pretending the workflow completed.

## Awaiting-human states

- `ready_for_review`: all scaffold evidence is gathered and a human decision is needed.
- `blocked_on_input`: required parameters, authorization, or scope are missing.
- `blocked_on_verifier`: verifier evidence failed or is incomplete.
- `waiver_requested`: an explicit human waiver is needed before proceeding.
- `m0_waiting`: scaffold is complete but runtime implementation remains unauthorized.

## Scaffold-time evidence

- `runbooks/m0-authorization-boundary.md` names the phrase gate for runtime work.
- `runbooks/scaffold-verification.md` explains how to rerun the quality gate.
- `schematics/runbook-awaiting-human-fsm.md` renders the state transitions.
- `.deployment-work/status/scaffold-status.json` records the current state without claiming deployment.

## Future M0 rule

A future executor may pause at these states and write receipts, but it must not auto-waive, auto-approve, or fabricate human authorization.

## Review checklist

- Is the missing human decision named explicitly?
- Does the receipt preserve enough context to resume?
- Is the workflow blocked rather than marked PASS when authorization is absent?
- Are waiver and verifier failure distinct states?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a use-pattern contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/use_patterns/UP_RUNBOOK_AWAITING_HUMAN.md`.
- Parent directory: `ai_docs/use_patterns`.
- Adjacent markdown siblings sampled: INDEX.md, UP_ATUIN_QI_CHAIN.md, UP_BOUNDED_OUTPUT.md, UP_CLUSTERED_MODULES.md, UP_EXECUTOR_VERIFIER_SPLIT.md, UP_RECEIPT_GRAPH.md.
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

