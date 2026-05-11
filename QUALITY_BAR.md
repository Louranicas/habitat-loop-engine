# Quality Bar

MEv2 L1 is the gold standard: trait-first boundaries, explicit state, typed errors, bounded output, clustered modules, and behavior-bearing tests.

## Scaffold bar

- `plan.toml` and `ULTRAMAP.md` must agree.
- Seven layer docs must exist.
- S01-S13 specs must exist.
- Anti/use-pattern registries must exist.
- `.claude` must contain context, local rules, commands, rules, and agents.
- Cargo workspace must compile as skeleton-only.
- No `unwrap`, `expect`, `panic!`, `todo!`, `dbg!`, or `unsafe` in Rust sources.

## Semantic predicate bars

The detailed receipt and PASS/FAIL examples live in `docs/quality/semantic-predicates.md`.

- `HLE-SP-001`: anti-pattern docs require detector semantics, negative controls, and remediation expectations instead of file-count-only registry presence.
- `HLE-SP-002`: S01-S13 specs require acceptance gates that reject premature execution PASS claims instead of file-count-only inventory presence.
- `HLE-SP-003`: verifier scripts must map checklist bars to explicit predicate IDs and evaluator paths instead of relying on a script-name checklist.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a root or topic documentation surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `QUALITY_BAR.md`.
- Parent directory: `.`.
- Adjacent markdown siblings sampled: ARCHITECTURE.md, CHANGELOG.md, CLAUDE.local.md, CLAUDE.md, CODEOWNERS.md, HARNESS_CONTRACT.md, QUICKSTART.md, README.md.
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

