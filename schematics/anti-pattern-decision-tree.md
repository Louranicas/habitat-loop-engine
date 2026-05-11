# anti-pattern-decision-tree

```text
New scaffold or future runtime surface
  |
  v
Does it introduce async work, locks, unbounded state, or claim authority?
  |-- no --> require ordinary doc/schema/test review
  |
  yes
  |
  v
Which predicate class applies?
  |-- AP28 compositional drift --> verify cross-surface alignment
  |-- AP29 blocking in async --> require async-native or spawn_blocking boundary
  |-- AP31 nested locks --> require lock-order proof or redesign
  |-- C6/C7/C12/C13 constraints --> require specific remediation predicate
  |
  v
Is there a negative control and verifier receipt?
  |-- no --> BLOCKED
  |-- yes --> reviewer checks evidence and hash anchors
  |
  v
PASS only if the semantic predicate, not just file count, is satisfied
```

## Review note

The anti-pattern registry is not a checklist trophy. It is a set of detector predicates that future M0 code must keep satisfiable under independent review.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a text-first architecture schematic within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `schematics/anti-pattern-decision-tree.md`.
- Parent directory: `schematics`.
- Adjacent markdown siblings sampled: INDEX.md, atuin-qi-chain.md, devops-v3-integration-flow.md, executor-verifier-sequence.md, layer-dag.md, module-graph.md, receipt-graph.md, runbook-awaiting-human-fsm.md.
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

