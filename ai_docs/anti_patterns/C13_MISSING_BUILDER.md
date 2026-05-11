# C13_MISSING_BUILDER

Status: scaffold detector predicate. Runtime detector implementation is explicitly deferred until M0 authorization.

Predicate ID: `HLE-SP-001`

## Detector predicate

A future detector for `C13_MISSING_BUILDER` must identify evidence of configuration-heavy type construction without a builder or equivalent validation boundary in source, configuration, verifier receipts, or scaffold review artifacts. The detector predicate is reviewable at scaffold time even though executable runtime detection is not implemented here.

## Negative control

A compliant example must not fire the detector when the same concern is handled by an explicit boundary, bounded contract, documented verifier gate, or typed/isolated implementation path. Negative controls are required before any future runtime detector can claim PASS.

## Remediation expectation

A remediation receipt must name the affected file, describe the semantic correction, and point to verifier evidence. Count-only evidence such as file presence or registry size is insufficient for `C13_MISSING_BUILDER`.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a anti-pattern detector contract within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ai_docs/anti_patterns/C13_MISSING_BUILDER.md`.
- Parent directory: `ai_docs/anti_patterns`.
- Adjacent markdown siblings sampled: AP28_COMPOSITIONAL_INTEGRITY_DRIFT.md, AP29_BLOCKING_IN_ASYNC.md, AP31_NESTED_LOCKS.md, C12_UNBOUNDED_COLLECTIONS.md, C6_LOCK_HELD_SIGNAL_EMIT.md, C7_LOCK_GUARD_REFERENCE_RETURN.md, FP_FALSE_PASS_CLASSES.md, INDEX.md.
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

