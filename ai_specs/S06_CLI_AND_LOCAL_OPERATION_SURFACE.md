# S06 CLI AND LOCAL OPERATION SURFACE

Status: M0 local implementation surface.

## Authority

The CLI may run only explicit local commands, write only operator-selected local ledger paths, and report verifier-derived verdicts. The codebase needs to be 'one shotted': each CLI runtime invocation must run once, terminate, and leave local receipt evidence.

## Acceptance

- Present in `ai_specs/INDEX.md`.
- `hle run` requires `--workflow` and `--ledger`.
- `hle verify` reads a local ledger and reports verifier authority.
- `hle daemon` requires `--once`; unbounded daemon behavior is rejected.
- Referenced by scaffold verification.
- Not marked execution PASS before implementation evidence exists.
- No secrets are printed and no external Habitat writes are performed without a verifier receipt.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a AI specification contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_specs/S06_CLI_AND_LOCAL_OPERATION_SURFACE.md`.
- Parent directory: `ai_specs`.
- Adjacent markdown siblings sampled: INDEX.md, S01_SYSTEM_OVERVIEW_AND_AUTHORITY_BOUNDARY.md, S02_WORKFLOW_DEFINITION_MODEL.md, S03_EXECUTOR_STATE_MACHINE.md, S04_VERIFIER_AND_RECEIPT_AUTHORITY.md, S05_PERSISTENCE_LEDGER_AND_SQLITE_SCHEMA.md, S07_ANTI_USE_PATTERN_INTELLIGENCE.md, S08_ATUIN_QI_VERIFICATION_CHAIN.md.
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

