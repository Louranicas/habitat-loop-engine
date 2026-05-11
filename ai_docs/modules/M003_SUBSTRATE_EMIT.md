# M003 SUBSTRATE_EMIT

Layer: L03
Crate: `crates/substrate-emit`
Status: M0 local emitter.

## M0 local acceptance

- bounded JSONL receipt emission exists;
- local command guard rejects network/live-integration tokens and absolute/relative paths whose basename matches a blocked command;
- local command guard tokenizes shell separators so chained blocked commands such as `safe;rm` are rejected before execution;
- module listed in `plan.toml`;
- module listed in `ULTRAMAP.md`;
- no network, cron, or daemon behavior.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> Obsidian HOME -> ULTRAMAP.md -> layer docs -> this module -> source`.

- Previous authority: `L02_PERSISTENCE.md; L03_WORKFLOW_EXECUTOR.md; L07_RUNBOOK_SEMANTICS.md` in `ai_docs/layers/` plus `ULTRAMAP.md`.
- Source authority: `crates/substrate-emit/src/lib.rs`.
- Reciprocal requirement: when this module changes, update its layer doc(s), `ULTRAMAP.md`, source comments/behavior, README/Quickstart if operator-visible, and the Obsidian HOME backlink block before claiming deployment alignment.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a module contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/modules/M003_SUBSTRATE_EMIT.md`.
- Parent directory: `ai_docs/modules`.
- Adjacent markdown siblings sampled: INDEX.md, M001_SUBSTRATE_TYPES.md, M002_SUBSTRATE_VERIFY.md, M004_HLE_CLI.md.
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

