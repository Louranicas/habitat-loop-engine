# M0 Authorization Boundary

Local M0 is now authorized, but only as a one-shot local runtime surface. The codebase needs to be 'one shotted': every operator-facing runtime path must run once, stay in the foreground, terminate, and leave verifier-visible local receipt evidence.

## One-shot operating boundary

Allowed local-M0 commands:

```bash
scripts/quality-gate.sh --m0 --json
cargo run -q -p hle-cli -- run --workflow examples/workflow.example.toml --ledger .deployment-work/runtime/m0-example-ledger.jsonl
cargo run -q -p hle-cli -- verify --ledger .deployment-work/runtime/m0-example-ledger.jsonl
cargo run -q -p hle-cli -- daemon --once --workflow examples/workflow.example.toml --ledger .deployment-work/runtime/m0-daemon-ledger.jsonl
```

Forbidden without later explicit authorization:

- Running `hle daemon` without `--once`.
- Installing cron jobs, systemd units, service managers, watchers, or background loops.
- Adding live Habitat or stcortex write integrations from this runbook alone.
- Treating local-M0 PASS as production deployment clearance.

## Planned topology authorization scope

The one-shot boundary above applies equally to the planned 46 modules (M005-M054 in `plan.toml [[planned_modules]]`) once the topology expansion lands. Cluster invariants that MUST be preserved across the expansion:

- **C03 Bounded Execution** (`bounded`, `local_runner`, `phase_executor`, `timeout_policy`, `retry_policy`) — bounded foreground execution; no background workers, no persistent loops, no fire-and-forget tasks.
- **C07 Dispatch Bridges** (`zellij_dispatch`, `atuin_qi_bridge`, `devops_v3_probe`, `stcortex_anchor_bridge`, `watcher_notice_writer`) — read-only bridge surface until M2+ explicit authorization. See C07 status in `plan.toml [full_codebase_clusters]`.
- **C06 Runbook Semantics** (`runbook_human_confirm`, `runbook_safety_policy`, `runbook_manual_evidence`) — AwaitingHuman semantics preserved; no executor self-confirmation.
- **C02 Authority & State** (`claim_authority`, `claim_authority_verifier`) — type-state authority enforced; executor cannot mint verifier-domain receipts.
- **C01 Evidence Integrity** (`receipt_hash`, `receipt_sha_verifier`, `final_claim_evaluator`) — receipt SHA recompute is independent of executor; verifier owns final claim authority.

Authorization phrases per `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`:

- Compile-safe stubs across the planned topology = scaffold expansion under existing `begin scaffold` (Phase D §3 of the deployment framework).
- Real implementation logic (SQLite wired, receipt SHA recompute live, bridges sending requests, runbooks executing) = explicit `begin M0` required.
- Live Habitat write-side integrations and cron/daemon installation remain out of scope under any phrase issued today.

If runtime scope expands, update `plan.toml`, `ULTRAMAP.md`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, this runbook, and the dedicated Obsidian mirror before any code or service change.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a operator runbook within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `runbooks/m0-authorization-boundary.md`.
- Parent directory: `runbooks`.
- Adjacent markdown siblings sampled: INDEX.md, scaffold-verification.md.
- This file should be read with `plan.toml`, `ULTRAMAP.md`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, and `.deployment-work/status/scaffold-status.json` when deciding whether a change is scaffold-only, local-M0, or outside authorization.

### Verification hooks
- Baseline scaffold gate: `scripts/quality-gate.sh --scaffold --json`.
- Local-M0 gate: `scripts/quality-gate.sh --m0 --json`.
- One-shot phrase gate: `scripts/verify-m0-runtime.sh` must find `codebase needs to be 'one shotted'` in the CLI source and canonical authority docs.
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

