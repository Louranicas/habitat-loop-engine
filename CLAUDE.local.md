# Habitat Loop Engine — Local Operator Overlay

This file complements `CLAUDE.md` for agents working inside `/home/louranicas/claude-code-workspace/habitat-loop-engine`.

`CLAUDE.md` is the project authority. This local overlay is the fast-start checklist, current boundary reminder, and local verification map. If this file conflicts with `CLAUDE.md`, `plan.toml`, or `ULTRAMAP.md`, treat those authority files as canonical and update this overlay after reconciling the drift.

## Current Authorization Boundary

- Bounded local-M0 implementation is authorized.
- The codebase needs to be 'one shotted': local runtime commands must execute once, terminate, and leave verifier-visible receipt evidence.
- Scaffold/documentation maintenance is authorized when it preserves alignment across authority surfaces.
- Live Habitat write integrations are not authorized from this file alone.
- Cron jobs, systemd units, unbounded daemons, and background service deployment are not authorized from this file alone.
- STcortex references here are recall/context anchors unless an explicit later instruction authorizes live write-path integration for this repo.

## Start Here

1. Read `CLAUDE.md` first.
2. Read `plan.toml` for phase/module intent.
3. Read `ULTRAMAP.md` for layer/module/source/doc alignment.
4. Read `.deployment-work/status/scaffold-status.json` for current scaffold receipt state.
5. Read `docs/SCRIPT_SPEC_PREDICATE_MAP.md` before changing verifier or quality-gate behavior.

## End-to-end stack deployment cross-reference chain

Treat this repository as a single bidirectional deployment graph while the full end-to-end stack of the codebase is deployed. Every change that touches an operator surface, layer contract, module contract, or source crate must cross-reference the adjacent surfaces in both directions before it is complete.

Canonical chain:

1. `CLAUDE.local.md` — local operator overlay and start-here authority.
2. `README.md` — human entry point and mission/boundary summary.
3. `QUICKSTART.md` — executable one-shot operator command path.
4. Dedicated Obsidian vault — review and knowledge graph mirror: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`.
5. `ULTRAMAP.md` — layer/module/source alignment authority.
6. `ai_docs/layers/L*.md` — L01-L07 layer contracts.
7. `ai_docs/modules/M*.md` — M001-M004 module contracts.
8. `crates/*/src/*.rs` — source implementation authority.

Required cross-reference rule: when any node in this chain changes, inspect the previous and next node, update reciprocal links or notes if behavior/authority changed, then refresh manifests and rerun the relevant scaffold/M0 gates. Do not claim the stack is deployed from a single surface; deployment claims require agreement across the whole chain plus receipts.

Primary forward links:

- README: `README.md`
- Quickstart: `QUICKSTART.md`
- Obsidian HOME: `obsidian://open?vault=habitat-loop-engine&file=HOME`
- Obsidian HOME path: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- UltraMap: `ULTRAMAP.md`
- Layer docs: `ai_docs/layers/L01_FOUNDATION.md`, `ai_docs/layers/L02_PERSISTENCE.md`, `ai_docs/layers/L03_WORKFLOW_EXECUTOR.md`, `ai_docs/layers/L04_VERIFICATION.md`, `ai_docs/layers/L05_DISPATCH_BRIDGES.md`, `ai_docs/layers/L06_CLI.md`, `ai_docs/layers/L07_RUNBOOK_SEMANTICS.md`
- Module docs: `ai_docs/modules/M001_SUBSTRATE_TYPES.md`, `ai_docs/modules/M002_SUBSTRATE_VERIFY.md`, `ai_docs/modules/M003_SUBSTRATE_EMIT.md`, `ai_docs/modules/M004_HLE_CLI.md`
- Source crates: `crates/substrate-types/src/lib.rs`, `crates/substrate-verify/src/lib.rs`, `crates/substrate-emit/src/lib.rs`, `crates/hle-cli/src/main.rs`

## Local Source Map

- M001 / L01 substrate types: `crates/substrate-types/src/lib.rs`.
- M002 / L04-L07 verifier substrate: `crates/substrate-verify/src/lib.rs`.
- M003 / L02-L03-L07 event/emission substrate: `crates/substrate-emit/src/lib.rs`.
- M004 / L06 CLI substrate: `crates/hle-cli/src/main.rs`.
- CLI entrypoint package: `crates/hle-cli`.
- Python tests and harness checks: `tests/`.
- Scripts and gates: `scripts/`.

## Planned topology specs (M005-M054, authored 2026-05-11)

The 50-module planned topology is now fully spec'd at `ai_specs/modules/`. These are doc-only — no Rust source has been written. Implementation is gated by `begin M0`.

- Spec index: `ai_specs/modules/INDEX.md`
- Master Index: `MASTER_INDEX.md`
- Cluster overviews + 50 per-module specs across `c01-evidence-integrity/` through `c09-devops-qi-lane/`
- Gold-standard reference: `/home/louranicas/claude-code-workspace/the_maintenance_engine_v2/ai_specs/m1-foundation-specs/` (MEv2 L1 — 80.6/100 quality score, 4083 tests, 0 clippy warnings)
- Each spec follows MEv2 pattern: header (file/LOC/tests/role), Types-at-a-Glance, Rust signatures with #[derive], method/trait tables, Design Notes, cluster invariants
- Total: 59 spec files, ~14,000 lines of markdown

## Verification Commands

Run the smallest relevant gate while working, then the broader gate before claiming completion.

```bash
cd /home/louranicas/claude-code-workspace/habitat-loop-engine
scripts/verify-sync.sh
scripts/quality-gate.sh --scaffold --json
scripts/quality-gate.sh --m0 --json
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
sha256sum -c SHA256SUMS.txt
```

For documentation-only edits, refresh the manifest before `sha256sum -c SHA256SUMS.txt` if the changed file is tracked by the manifest.

## Completion Standard

A change is not complete until:

- Source, docs, `plan.toml`, and `ULTRAMAP.md` agree.
- The relevant quality gate passes.
- Manifest verification passes or the manifest is intentionally refreshed and then verified.
- PASS/FAIL claims cite actual command output or receipt files.
- No live integration, daemon, cron, or service deployment has been introduced without explicit authorization.

## Obsidian and Review Vault Anchors

Dedicated review vault:

- `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/MASTER_INDEX.md`
- `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/00 Index/Scaffold Operator Quickstart.md`
- `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/00 Index/Master Index.md`

Deployment framework authority:

- `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`

## STcortex Recall Anchors

Use these as context/recall handles when STcortex is available:

- namespace: `hle`
- readiness memory: `hle:coding-readiness-sync`
- boundary anchor: `hle:local-m0-boundary`
- verification anchor: `hle:verification-gates`
- directory review anchor: `hle:directory-md-comprehensiveness-matrix`

Do not infer permission for live STcortex writes from these anchors. They document prior readiness/context sync only.

## Failure Modes to Avoid

- Saying "ready" or "complete" without gate output.
- Treating local-M0 authorization as live Habitat deployment authorization.
- Adding background loops, watchers, daemons, cron jobs, or service units.
- Editing verifier semantics without reading `docs/SCRIPT_SPEC_PREDICATE_MAP.md`.
- Updating documentation without refreshing/validating tracked manifests.
- Letting `CLAUDE.local.md` drift from `CLAUDE.md`, `plan.toml`, or `ULTRAMAP.md`.

## Maintenance Note

This file previously contained a stale line saying M0/coding remained blocked. The active project authority now says bounded local-M0 implementation is authorized while live integrations and unbounded runtime behavior remain forbidden.
