# zellij-orchestrator-deployment-flow

```text
Weaver / Hermes planner
  |
  v
verify pane labels and target context
  |
  v
send role-specific Command / Command-2 / Command-3 packet
  |
  v
bounded debate or review interval
  |
  v
collect durable artifacts and hashes
  |
  v
synthesize receipt and Kanban comments
  |
  v
return visible panes only when active dispatch follow-up requires it
```

## Scaffold boundary

Zellij orchestration may be used to gather reviews and debate outputs, but it does not grant runtime authority. A Command-pane recommendation must be captured as evidence and then verified against scaffold gates.

## Targeting rule

Pane labels and layout must be verified before dispatch. Do not assume focus, tab order, or pane numbering from memory.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a text-first architecture schematic within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `schematics/zellij-orchestrator-deployment-flow.md`.
- Parent directory: `schematics`.
- Adjacent markdown siblings sampled: INDEX.md, anti-pattern-decision-tree.md, atuin-qi-chain.md, devops-v3-integration-flow.md, executor-verifier-sequence.md, layer-dag.md, module-graph.md, receipt-graph.md.
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

