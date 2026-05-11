# L04 Verification

Status: scaffold-only.

## Responsibilities

- Define boundaries for L04.
- Link to module docs and specs.
- Preserve executor/verifier separation.

## Forbidden during scaffold

- Runtime behavior beyond skeleton compile checks.
- Live service writes.

## Bidirectional deployment chain links

This layer is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> this layer -> module docs -> source`.

- Previous authority: `ULTRAMAP.md` layer table.
- Peer layers: `ai_docs/layers/`.
- Downstream module authority: `M002_SUBSTRATE_VERIFY` in `ai_docs/modules/`.
- Downstream source authority: `crates/substrate-verify/src/lib.rs`.
- Reciprocal requirement: when this layer changes, update `ULTRAMAP.md`, the listed module docs, source comments/behavior where applicable, and the Obsidian HOME backlink block before claiming deployment alignment.

## Cluster ownership (planned topology M005-M054)

Crates: substrate-verify, hle-verifier.

Clusters touching this layer (per `ai_docs/CLUSTERED_MODULES.md`):

- C01 (M008 receipt_sha_verifier, M009 final_claim_evaluator), C02 (M014 claim_authority_verifier), C04 (M020 anti_pattern_scanner, M023 test_taxonomy_verifier, M024 false_pass_auditor)

Per-cluster overview docs: `ai_specs/modules/c01..c09/00-CLUSTER-OVERVIEW.md`.
Per-module specs: `ai_specs/modules/c0x-<slug>/M0xx_<NAME>.md`.
Per-module sheets: `ai_docs/modules/M0xx_<NAME>.md`.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a layer contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/layers/L04_VERIFICATION.md`.
- Parent directory: `ai_docs/layers`.
- Adjacent markdown siblings sampled: L01_FOUNDATION.md, L02_PERSISTENCE.md, L03_WORKFLOW_EXECUTOR.md, L05_DISPATCH_BRIDGES.md, L06_CLI.md, L07_RUNBOOK_SEMANTICS.md.
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

