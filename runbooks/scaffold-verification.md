# Scaffold Verification Runbook

Run the scaffold and local-M0 gates as one-shot foreground commands. The codebase needs to be 'one shotted': verification must prove that runtime paths are bounded, explicit, finite, and receipt-visible.

## Standard verification flow

Run the full pipeline as a single canonical sequence. Both gates wrap the same family-grouped verifier chain — the M0 gate adds runtime predicates that the scaffold gate skips:

```bash
# 1. Manifest integrity (catches doc drift before predicates run)
sha256sum -c SHA256SUMS.txt

# 2. Scaffold gate (21 verify scripts + 4 cargo + 2 python = 27 steps; scaffold-mode predicates)
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo \
  PATH=/home/louranicas/.cargo/bin:$PATH \
  scripts/quality-gate.sh --scaffold --json | tee .deployment-work/status/quality-gate-scaffold-latest.json

# 3. M0 gate (adds verify-m0-runtime = 22 verify scripts + 4 cargo + 2 python = 28 steps)
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo \
  PATH=/home/louranicas/.cargo/bin:$PATH \
  scripts/quality-gate.sh --m0 --json       | tee .deployment-work/status/quality-gate-m0-latest.json

# 4. Re-verify manifest (catches gate-mutated state files like scaffold-status.json)
sha256sum -c SHA256SUMS.txt
```

Family-grouped predicate breakdown lives in [`verification-and-sync-pipeline.md`](verification-and-sync-pipeline.md). Predicate-to-spec mapping lives in `../docs/SCRIPT_SPEC_PREDICATE_MAP.md`. Wrapper parity (`bin/hle-*` ↔ `scripts/verify-*.sh`) is enforced by `scripts/verify-bin-wrapper-parity.sh`.

For mirror/project edits, also verify:

```bash
( cd /home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine && sha256sum -c '12 Receipts/VAULT_SHA256SUMS.txt' )
( cd /home/louranicas/claude-code-workspace/loop-workflow-engine-project && sha256sum -c PROJECT_SHA256SUMS.txt )
```

Expected runtime proof inside the M0 gate:

- `scripts/verify-m0-runtime.sh` passes.
- `hle daemon --once ...` passes.
- `hle daemon` without `--once` is rejected by tests.
- Rust tests, Python tests, cargo fmt/check/test, and clippy `-D warnings` all pass.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a operator runbook within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `runbooks/scaffold-verification.md`.
- Parent directory: `runbooks`.
- Adjacent markdown siblings sampled: INDEX.md, m0-authorization-boundary.md.
- This file should be read with `plan.toml`, `ULTRAMAP.md`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, and `.deployment-work/status/scaffold-status.json` when deciding whether a change is scaffold-only, local-M0, or outside authorization.

### Verification hooks
- Baseline scaffold gate: `scripts/quality-gate.sh --scaffold --json`.
- Local-M0 gate: `scripts/quality-gate.sh --m0 --json`.
- One-shot phrase gate: `scripts/verify-m0-runtime.sh` checks the CLI and canonical docs for `codebase needs to be 'one shotted'`.
- Manifest authority: `sha256sum -c SHA256SUMS.txt` after every documentation or status edit.
- For vault/framework-only edits, refresh the appropriate vault/framework manifest before declaring closure.

### Acceptance criteria
- The document names its role, boundary, and verification surface clearly.
- Claims about PASS/FAIL are backed by verifier output or receipts, not prose alone.
- Any runtime behavior described here remains local-only, one-shot, foreground, finite, and receipt-emitting unless a later authorization receipt explicitly expands scope.
- Future agents can identify which files to inspect next without guessing hidden context.

### Failure modes
- Treat vague "complete", "ready", or "deployed" wording as insufficient unless it points to gates, manifests, and receipts.
- Do not infer live integration permission from local-M0 wording.
- Do not create background services, recurring jobs, implicit loops, or daemonized workers from this document alone.
- If this file drifts from `plan.toml` or `ULTRAMAP.md`, update the authority files first and rerun gates.

### Next maintenance action
On the next broadening pass, re-run the markdown census, inspect files with fewer than 180 words or missing boundary/verification terms, update this section with any new authority roots, then refresh manifests and rerun both quality gates.

