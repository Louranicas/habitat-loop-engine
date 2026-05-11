# Habitat Loop Engine scaffold/M0 broadening review

Created UTC: 2026-05-10T11:28:40Z
Reviewer: Weaver / Hermes
Scope: `/home/louranicas/claude-code-workspace/habitat-loop-engine`, the dedicated review vault, and the deployment framework source packet.
Boundary: broaden scaffold and local M0 verifier/runtime surfaces only. No live Habitat writes, cron installation, systemd service, unbounded daemon, package installation, or final deployment claim.

## Source roots reviewed

| Root | Role | Observed shape |
| --- | --- | --- |
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework` | Deployment framework authority packet | 28 files after excluding `.git`, `target`, cache dirs; includes authorization phrases, false-100 traps, handoffs, subagent prompts, receipts, and SHA256 manifest. |
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine` | Dedicated review vault | 48 markdown/manifest files after excluding `.obsidian`; 18 topic directories covering index, deployment framework, orchestrator collaboration, scaffold contract, Claude folder, QI, module genesis, patterns, runbook layer, receipts, gap closure, runtime guidelines, readiness specs, authorization, and workflows. |
| `/home/louranicas/claude-code-workspace/habitat-loop-engine` | Active scaffold/runtime repository | 166 non-generated files after excluding `.git`, `target`, cache dirs; includes root docs, 13 specs, 7 layers, 4 module docs, anti/use pattern registries, schematics, scripts, bin wrappers, Rust crates, tests, schemas, runbooks, receipts, runtime ledgers, and status packets. |

## Directory/file coverage review

| Surface | Expected from deployment framework | Observed | Review verdict |
| --- | --- | --- | --- |
| Root governance docs | README, QUICKSTART, ARCHITECTURE, ULTRAMAP, QUALITY_BAR, HARNESS_CONTRACT, CLAUDE files, changelog | Present | Covered; status language now needs M0-local alignment rather than old scaffold-only wording. |
| Plan authority | Lowercase `plan.toml`, authorization flags, layer/module/script mapping | Present; `m0_runtime = true`, live/cron flags false | Covered; add explicit `m0_runtime` script entry for broadened mapping clarity. |
| AI specs | Exactly S01-S13 | Present: 13 spec files plus index | Covered. |
| AI layer docs | Exactly L01-L07 | Present: 7 layer docs | Covered. |
| Module docs | M001-M004 and index | Present | Covered. |
| Rust crates | substrate-types, substrate-verify, substrate-emit, hle-cli | Present and expanded into bounded M0 local runtime | Covered; verifier authority and one-shot daemon guard must remain mandatory. |
| Verifier scripts | Structural, docs, Claude folder, pattern, module/layer, receipt, negative controls, bounded logs, script safety | Present, plus local loop helper and M0 runtime verifier | Covered; M0 verifier is now part of the broadened framework. |
| Bin wrappers | One wrapper per verifier script | Present | Covered by `verify-bin-wrapper-parity`. |
| Tests | Python unit/integration and Rust workspace tests | Present; 17 Rust tests and 9 Python tests observed in gates | Covered. |
| Manifests/receipts | SHA256SUMS, status, receipts | Present | Covered but manifest should be refreshed after this broadening pass. |
| Dedicated vault parity | Expected review-vault sections | Present | Covered by `verify-vault-parity`. |
| Local runtime receipts | Bounded local JSONL examples only | Present under `.deployment-work/runtime/` | Covered; keep out of live integration claims. |

## Broadening actions from this review

1. Recorded a directory/file coverage receipt as this document so future agents can see what was checked and why.
2. Reconciled the historical scaffold-only workflow packet with the current local-M0 transition state by adding a transition addendum instead of deleting provenance.
3. Broadened `ULTRAMAP.md` from a sparse four-module map into an operator-facing map that names verifier families, runtime gates, receipts/manifests, vault/framework roots, and the local-only boundary.
4. Broadened `plan.toml` script mapping with `m0_runtime = "scripts/verify-m0-runtime.sh"` so the M0 verifier is explicitly anchored in the plan.
5. Updated machine status to reflect the observed local M0 authorization while preserving `live_integrations_authorized=false` and `cron_daemons_authorized=false`.

## Verification performed before edits

Both gates passed before this broadening pass:

- `scripts/quality-gate.sh --scaffold --json`: PASS.
- `scripts/quality-gate.sh --m0 --json`: PASS.

Observed test counts from the gate output:

- Rust tests: 17/17 passing.
  - `hle-cli`: 3/3.
  - `substrate-emit`: 8/8.
  - `substrate-types`: 2/2.
  - `substrate-verify`: 4/4.
- Python tests: 9/9 passing.
  - `tests/unit/test_manifest.py`: 4/4.
  - `tests/integration/test_scaffold.py`: 5/5.

## Post-review required gate

After this file and alignment edits, rerun:

```bash
cd /home/louranicas/claude-code-workspace/habitat-loop-engine
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --scaffold --json
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --m0 --json
```

Then refresh `SHA256SUMS.txt` because this review intentionally changes tracked scaffold documentation.

## Boundary restatement

The active broadened state is local M0 only. The deployment framework is now broadened into an executable local verifier/runtime substrate, but this review does not authorize live Habitat writes, background services, OS cron/systemd, external deployment, or final production deployment claims.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a workflow preservation artifact within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `docs/workflows/scaffold-m0-broadening-review-20260510T112840Z.md`.
- Parent directory: `docs/workflows`.
- Adjacent markdown siblings sampled: scaffold-deployment-kanban-map.md, scaffold-deployment-mermaid.md, scaffold-deployment-workflow.md, scaffold-workflow-kanban-closure.md.
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

