# Semantic Predicate Quality Bars

This receipt promotes scaffold bars that were previously count/checklist-only into reviewable semantic predicates. It is scaffold-only: it defines documentation and verifier-surface requirements without implementing M0 runtime behavior or live Habitat integrations.

## Current scaffold bars enumerated first

The pre-existing scaffold quality bars were:

1. Root inventory parity: `plan.toml` and `ULTRAMAP.md` agree.
2. Layer inventory floor: seven layer docs exist.
3. Spec inventory floor: S01-S13 specs exist.
4. Anti/use-pattern registry floor: anti-pattern and use-pattern registries exist.
5. Claude scaffold floor: `.claude` contains context, local rules, commands, rules, and agents.
6. Cargo skeleton floor: workspace compiles as skeleton-only.
7. Rust safety floor: Rust sources contain no `unwrap`, `expect`, `panic!`, `todo!`, `dbg!`, or `unsafe`.
8. Gate-chain floor: scaffold quality gate runs sync, doc links, `.claude`, anti-pattern registry, module map, layer DAG, receipt schema, negative controls, runbook schema, receipt graph, test taxonomy, bounded logs, script safety, cargo fmt/check/test/clippy, and Python scaffold tests.

## Promoted semantic predicates

### HLE-SP-001: anti-pattern docs require detector semantics

- Source files: `ai_docs/anti_patterns/*.md` except `INDEX.md`.
- Rationale: the old registry floor only proved that anti-pattern files existed. A semantic bar must prove each anti-pattern file declares what the future detector observes, how a negative control avoids false positives, and what remediation evidence is expected.
- Evaluator/check location: `scripts/verify-semantic-predicates.sh` checks every anti-pattern doc for `Predicate ID: `HLE-SP-001``, `## Detector predicate`, `## Negative control`, and `## Remediation expectation`.
- PASS example: `ai_docs/anti_patterns/AP29_BLOCKING_IN_ASYNC.md` declares a detector predicate for blocking calls in async contexts, a negative control for `spawn_blocking`/async-native paths, and remediation expectations.
- FAIL example: a new `ai_docs/anti_patterns/APXX.md` containing only `Status: scaffold detector placeholder` fails because it is count-only and lacks detector semantics.

### HLE-SP-002: S01-S13 specs require acceptance gates

- Source files: `ai_specs/S*.md` and `ai_specs/INDEX.md`.
- Rationale: the old spec floor only proved there were exactly 13 spec shells. A semantic bar must prove each spec carries an acceptance gate that can reject premature execution PASS claims.
- Evaluator/check location: `scripts/verify-semantic-predicates.sh` checks there are exactly 13 `ai_specs/S*.md` files and that each contains `## Acceptance`, `Present in `ai_specs/INDEX.md``, `Referenced by scaffold verification`, and `Not marked execution PASS before implementation evidence exists`.
- PASS example: `ai_specs/S04_VERIFIER_AND_RECEIPT_AUTHORITY.md` contains an Acceptance section with index presence, scaffold verification reference, and no premature execution PASS.
- FAIL example: a spec shell with a title and status but no Acceptance section fails even if the S-file count remains 13.

### HLE-SP-003: verifier scripts map to predicate IDs

- Source files: `scripts/quality-gate.sh`, `scripts/verify-semantic-predicates.sh`, `scripts/verify-antipattern-registry.sh`, `scripts/verify-sync.sh`, and `scripts/verify-script-safety.sh`.
- Rationale: the old gate-chain floor only listed scripts. A semantic bar must make the relationship between scaffold predicates and evaluator scripts explicit enough for reviewers to audit what each script proves.
- Evaluator/check location: `scripts/verify-semantic-predicates.sh` checks that this document names each required predicate ID and maps each predicate to at least one evaluator/check path. `scripts/quality-gate.sh --scaffold` runs the semantic predicate verifier.
- PASS example: this document maps `HLE-SP-001` to the anti-pattern docs and `scripts/verify-semantic-predicates.sh`, then the quality gate invokes that verifier.
- FAIL example: adding a predicate ID to `QUALITY_BAR.md` without naming its source files and evaluator path fails review because the predicate cannot be audited.

## Predicate-to-check map

| Predicate ID | Source files | Evaluator/check path | Existing checklist floor promoted |
|---|---|---|---|
| HLE-SP-001 | `ai_docs/anti_patterns/*.md` | `scripts/verify-semantic-predicates.sh`; supported by `scripts/verify-antipattern-registry.sh` | Anti-pattern file count |
| HLE-SP-002 | `ai_specs/S*.md`, `ai_specs/INDEX.md` | `scripts/verify-semantic-predicates.sh`; supported by `scripts/verify-sync.sh` | S01-S13 file count |
| HLE-SP-003 | `scripts/*.sh`, `scripts/quality-gate.sh` | `scripts/verify-semantic-predicates.sh`; invoked by `scripts/quality-gate.sh --scaffold` | Gate-chain checklist |

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a documentation/control surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `docs/quality/semantic-predicates.md`.
- Parent directory: `docs/quality`.
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

