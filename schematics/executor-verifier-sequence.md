# executor-verifier-sequence

```text
participant Human
participant Executor
participant ArtifactStore
participant Verifier
participant ReceiptGraph

Human -> Executor: future authorized workflow step
Executor -> ArtifactStore: write bounded artifact
Executor -> ReceiptGraph: write CLAIM receipt with artifact sha256
Verifier -> ArtifactStore: read artifact by path and sha256
Verifier -> ReceiptGraph: read claim receipt
Verifier -> Verifier: run independent checks and negative controls
Verifier -> ReceiptGraph: write VERIFIER receipt with PASS or BLOCKED
Human -> ReceiptGraph: review verdict and counter-evidence locator
```

## Scaffold boundary

This sequence is a contract diagram. The current scaffold does not implement a runtime executor. It documents that future M0 work must keep executor and verifier authority separate.

## Failure semantics

- Missing artifact hash -> BLOCKED.
- Executor and verifier identity collapse -> BLOCKED.
- Negative control unexpectedly passes -> BLOCKED.
- Human phrase gate absent for runtime work -> M0 waiting, not PASS.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a text-first architecture schematic within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `schematics/executor-verifier-sequence.md`.
- Parent directory: `schematics`.
- Adjacent markdown siblings sampled: INDEX.md, anti-pattern-decision-tree.md, atuin-qi-chain.md, devops-v3-integration-flow.md, layer-dag.md, module-graph.md, receipt-graph.md, runbook-awaiting-human-fsm.md.
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

