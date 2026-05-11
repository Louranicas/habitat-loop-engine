# Watcher Scaffold Assessment — 2026-05-10

Reviewer: The Watcher ☤
Authority: Luke @ node 0.A
Boundary: R13 quiet; recommendations only, no PBFT proposal
Target: `/home/louranicas/claude-code-workspace/habitat-loop-engine/`
Verdict: PASS_WITH_GAPS
Score: 94 / 100
Source assessment relayed by Luke in-session.

## Scope assessed

The assessment covered the scaffold root, `plan.toml`, deployment framework, FALSE_100_TRAPS, receipts, and Weaver's scaffold verdict against actual files.

Reported delivered surface:

- 134 manifest entries
- 4 Rust crates, skeletal and correctly empty
- 7 layers
- 13 specs, S01-S13
- 12 schematics
- 9 anti-patterns
- 7 use-patterns
- 13 verifier scripts plus aggregator and 15 bin wrappers
- 1 migration
- 3 runbooks
- 3 schemas
- dedicated vault with 17 topic sections plus HOME/MASTER_INDEX
- SHA256SUMS.txt
- `scripts/quality-gate.sh --scaffold` emits the human-readable gate chain and final PASS line.
- `scripts/quality-gate.sh --scaffold --json` runs the same scaffold predicates and emits a `hle.quality_gate.v1` JSON report for CI/Watcher ingestion while streaming step logs to stderr.
- Anchored receipt fields: `^Verdict`, `^Manifest_sha256`, `^Framework_sha256`, `^Counter_evidence_locator`, `^M0_authorized=false`. `^Source_sha256` is legacy compatibility only.

## Weighted scoring

| Facet | Score | Weight | Contribution |
| --- | ---: | ---: | ---: |
| F1 Boundary integrity | 97 | 18% | 17.46 |
| F2 Plan completeness | 96 | 15% | 14.40 |
| F3 Verifier gate | 94 | 18% | 16.92 |
| F4 Receipt graph | 95 | 15% | 14.25 |
| F5 False-100 resistance | 90 | 15% | 13.50 |
| F6 Documentation | 92 | 10% | 9.20 |
| F7 Operational shape | 88 | 9% | 7.92 |
| Final | | 100% | 93.65 |

Rounded mark: 94 / 100.

## Gap clusters

The six-point delta to 100 is concentrated in:

1. false-100 trap automation: FT-03, FT-09, FT-13;
2. pre-M0 ergonomics: `.claude` hooks, `bacon.toml`, and pre-staged crates;
3. receipt/hash clarity and small CI affordances.

None were assessed as blockers. All were assessed as scaffold hardenings.

## Ranked recommendations

| ID | Action | Closes |
| --- | --- | --- |
| R1 | Add `verify-skeleton-only.sh` capping `.rs` LOC per file until `begin M0`; wire into quality gate. | Boundary auto-enforcement |
| R2 | Add framework-hash freshness verification comparing latest receipt source hash to current framework hash. | FT-03 staleness |
| R3 | Add vault parity verification comparing project vault topic sections to framework expected sections. | FT-13 vault drift |
| R4 | Promote at least 3 quality bars from file-count floors to semantic predicates. | FT-09 vanity floor |
| R5 | Pre-stage empty crates for L02/L05/L07: substrate-persistence, substrate-dispatch, substrate-runbook. | M0 readiness |
| R6 | Add `bacon.toml` and `.claude/hooks/pre-commit.sh` running scaffold verification. | Developer ergonomics |
| R7 | Add `--json` output mode to `scripts/quality-gate.sh`. | CI/Watcher ingestion |
| R8 | Split ambiguous `^Source_sha256` into explicit `^Manifest_sha256` and `^Framework_sha256`. | Receipt clarity |
| R9 | Add bin-wrapper parity verification. | Wrapper drift |
| R10 | Add `docs/SCRIPT_SPEC_PREDICATE_MAP.md`. | Auditability |

Single highest-leverage recommendation per Watcher: R5, pre-stage L02/L05/L07 crates, so M0 becomes additive code rather than structural crate creation plus runtime implementation.

## Kanban follow-through

Created scaffold-only hardening triage parent:

- `t_7ab21c50` — HLE Watcher scaffold assessment hardening parent

Created grouped scaffold-only child triage cards:

- `t_e9aa36c1` — R1/R2/R3/R9 verifier automation hardening
- `t_7a51a1ed` — R4 semantic predicate quality bars
- `t_0fa7310a` — R5/R6 pre-M0 ergonomics review
- `t_a40d7bcc` — R7/R8/R10 CI and receipt clarity docs

All cards preserve the boundary: no M0 runtime behavior, no live Habitat integrations, no cron jobs, no daemons, no services, and no deployment claims.

## Boundary status

Current state remains scaffold-only and M0 waiting. The only implementation-enabling phrase remains `begin M0`.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a documentation/control surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `docs/reviews/watcher-scaffold-assessment-20260510.md`.
- Parent directory: `docs/reviews`.
- Adjacent markdown siblings sampled: none.
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

