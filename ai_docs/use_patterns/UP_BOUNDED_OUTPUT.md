# UP_BOUNDED_OUTPUT

Status: scaffold use-pattern contract. Runtime log enforcement is deferred until `begin M0`; scaffold scripts and docs already enforce bounded-output expectations.

Predicate ID: `HLE-UP-003`

## Intent

Workflow loops must never create unbounded logs, runaway buffers, or infinite background output streams. Human review should receive concise evidence packets with durable paths for deeper inspection.

## Scaffold-time rules

- Verification scripts print bounded summaries and fail with actionable messages.
- Temporary files under `.deployment-work/logs/` are excluded from canonical manifest when explicitly marked as transient.
- Long-running monitors must have a hard timeout and must be read-only unless separately authorized.
- Kanban monitor loops must report state changes, not stream repetitive full board dumps forever.

## Future runtime requirements

1. Every workflow run declares maximum log bytes per phase.
2. Every spawned command declares a timeout and output retention policy.
3. Verifier inputs are hash-addressed artifacts, not terminal scrollback.
4. Human-readable summaries point to bounded files and exact hashes.
5. Exceeding an output cap produces BLOCKED, not truncated PASS.

## Negative control

A fixture that emits an unbounded stream or writes an uncapped log must be rejected by future runtime verification. Scaffold-only negative controls may model this with a static fixture and expected failure signature.

## Review checklist

- Is there a hard cap, timeout, or retention policy?
- Does a summary cite the durable artifact rather than relying on scrollback?
- Does truncation prevent PASS?
- Are logs separated from canonical manifests unless intentionally preserved?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a use-pattern contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/use_patterns/UP_BOUNDED_OUTPUT.md`.
- Parent directory: `ai_docs/use_patterns`.
- Adjacent markdown siblings sampled: INDEX.md, UP_ATUIN_QI_CHAIN.md, UP_CLUSTERED_MODULES.md, UP_EXECUTOR_VERIFIER_SPLIT.md, UP_RECEIPT_GRAPH.md, UP_RUNBOOK_AWAITING_HUMAN.md.
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

