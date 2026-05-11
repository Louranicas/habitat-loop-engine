# UP_ATUIN_QI_CHAIN

Status: scaffold use-pattern contract. Atuin-QI is a provenance and recall aid, not a substitute for verifier receipts.

Predicate ID: `HLE-UP-004`

## Intent

The Loop Engine may use shell-history intelligence to reconstruct operator context, command provenance, and waiver history. That context can guide review, but it cannot independently certify a workflow PASS.

## Scaffold-time interpretation

- Atuin-derived command evidence may support receipt narratives.
- Command history must be reduced to bounded, relevant excerpts before entering a receipt.
- Secret-safe handling is mandatory: no API keys, tokens, or credentials may be copied into scaffold receipts.
- A Command-2 waiver can be recorded only when Luke explicitly grants it or when an approved supersession receipt exists.

## Future M0 integration boundary

A future Atuin adapter may emit typed evidence records such as command, timestamp, cwd, exit class, and redacted output hash. The verifier must still check these records against schemas and negative controls.

## Non-authority rule

A command appearing in shell history is not proof that it succeeded, nor proof that its side effects are acceptable. The receipt graph must include verifier evidence and artifact hashes.

## Review checklist

- Is Atuin evidence redacted and bounded?
- Is the difference between observed command and verified outcome explicit?
- Does any waiver cite an exact human-authorized phrase or supersession receipt?
- Are command excerpts tied to source hashes when they support claims?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a use-pattern contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/use_patterns/UP_ATUIN_QI_CHAIN.md`.
- Parent directory: `ai_docs/use_patterns`.
- Adjacent markdown siblings sampled: INDEX.md, UP_BOUNDED_OUTPUT.md, UP_CLUSTERED_MODULES.md, UP_EXECUTOR_VERIFIER_SPLIT.md, UP_RECEIPT_GRAPH.md, UP_RUNBOOK_AWAITING_HUMAN.md.
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

