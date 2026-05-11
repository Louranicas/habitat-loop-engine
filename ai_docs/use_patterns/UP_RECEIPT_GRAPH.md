# UP_RECEIPT_GRAPH

Status: scaffold use-pattern contract. Durable receipt graph implementation is deferred until `begin M0`, but graph shape and review semantics are scaffold-authoritative.

Predicate ID: `HLE-UP-002`

## Intent

Every important workflow claim must become a typed, hash-addressed receipt node. Receipts do not merely narrate progress; they define what evidence exists, what authority it has, and which earlier claims it supersedes or blocks.

## Minimal graph fields

- `receipt_id`: stable identifier for the receipt node.
- `claim_id`: identifier of the claim being asserted or verified.
- `claim_class`: scaffold, verifier, blocker, waiver, negative-control, or future runtime class.
- `source_artifact_path`: path to the artifact under review.
- `source_sha256`: hash of the artifact at claim time.
- `parent_sha256`: hash edge to the parent receipt or manifest when applicable.
- `verdict`: PASS, BLOCKED, WAIVED, SUPERSEDED, or INFORMATIONAL.
- `counter_evidence_locator`: where a reviewer should look for contradicting evidence.

## Scaffold-time evidence

- `.deployment-work/receipts/scaffold-authorization-*.md` anchors the current scaffold authority.
- `SHA256SUMS.txt` anchors the repository file set.
- `schematics/receipt-graph.md` documents the intended graph topology.
- `scripts/verify-receipt-graph.sh` checks presence and required anchors without claiming runtime completeness.

## Future M0 rule

M0 may append receipt nodes, but it must not mutate historical receipt contents in place. Corrections are represented by superseding nodes with explicit parent edges.

## Review checklist

- Does each PASS have an evidence path and hash?
- Is a waiver distinguishable from a verifier PASS?
- Can a future reader reconstruct why a claim was believed at the time?
- Are stale or superseded claims still auditable?

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a use-pattern contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/use_patterns/UP_RECEIPT_GRAPH.md`.
- Parent directory: `ai_docs/use_patterns`.
- Adjacent markdown siblings sampled: INDEX.md, UP_ATUIN_QI_CHAIN.md, UP_BOUNDED_OUTPUT.md, UP_CLUSTERED_MODULES.md, UP_EXECUTOR_VERIFIER_SPLIT.md, UP_RUNBOOK_AWAITING_HUMAN.md.
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

