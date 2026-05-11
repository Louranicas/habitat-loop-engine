# UltraMap

## Layers

| Layer | Doc | Cluster |
|---|---|---|
| L01 | ai_docs/layers/L01_FOUNDATION.md | HLE-C01 |
| L02 | ai_docs/layers/L02_PERSISTENCE.md | HLE-C02 |
| L03 | ai_docs/layers/L03_WORKFLOW_EXECUTOR.md | HLE-C03 |
| L04 | ai_docs/layers/L04_VERIFICATION.md | HLE-C04 |
| L05 | ai_docs/layers/L05_DISPATCH_BRIDGES.md | HLE-C05 |
| L06 | ai_docs/layers/L06_CLI.md | HLE-C06 |
| L07 | ai_docs/layers/L07_RUNBOOK_SEMANTICS.md | HLE-C07 |

## Modules

### Existing (M001-M004) — substrate-* crates + hle-cli main

| Module | Layer | Crate/Path | Tests | Specs |
|---|---|---|---|---|
| M001 | L01 | crates/substrate-types | tests/unit/test_manifest.py | S01,S02,S13 |
| M002 | L04 | crates/substrate-verify | tests/unit/test_manifest.py | S04,S08,S13 |
| M003 | L03 | crates/substrate-emit | tests/unit/test_manifest.py | S03,S05,S13 |
| M004 | L06 | crates/hle-cli | tests/integration/test_scaffold.py | S06 |

### Planned topology (M005-M054, authored 2026-05-11) — 9 clusters across 6 new crates + extended hle-cli + scripts/

50 module specs at `ai_specs/modules/c01..c09/M0xx_*.md` (MEv2 L1 gold standard pattern). Compile-safe Rust stubs landed under `start coding` 2026-05-11 with workspace-wide cargo check + clippy + test all PASS (1263 tests across 10 crates).

| Cluster | Layers | Crate(s) | Modules | Spec dir |
|---|---|---|---|---|
| C01 Evidence Integrity | L01/L02/L04 | hle-core, hle-storage, hle-verifier | M005 receipt_hash, M006 claims_store, M007 receipts_store, M008 receipt_sha_verifier, M009 final_claim_evaluator | `ai_specs/modules/c01-evidence-integrity/` |
| C02 Authority & State | L01/L03/L04 | hle-core, hle-executor, hle-verifier | M010 claim_authority, M011 workflow_state, M012 state_machine, M013 status_transitions, M014 claim_authority_verifier | `ai_specs/modules/c02-authority-state/` |
| C03 Bounded Execution | L03 | hle-executor | M015 bounded, M016 local_runner, M017 phase_executor, M018 timeout_policy, M019 retry_policy | `ai_specs/modules/c03-bounded-execution/` |
| C04 Anti-Pattern Intelligence | L01/L02/L04 | hle-core, hle-storage, hle-verifier | M020 anti_pattern_scanner, M021 anti_pattern_events, M022 test_taxonomy, M023 test_taxonomy_verifier, M024 false_pass_auditor | `ai_specs/modules/c04-anti-pattern-intelligence/` |
| C05 Persistence Ledger | L02 | hle-storage | M025 pool, M026 migrations, M027 workflow_runs, M028 workflow_ticks, M029 evidence_store, M030 verifier_results_store, M031 blockers_store | `ai_specs/modules/c05-persistence-ledger/` |
| C06 Runbook Semantics | L07 | hle-runbook | M032-M039 runbook_{schema,parser,phase_map,human_confirm,manual_evidence,scaffold,incident_replay,safety_policy} | `ai_specs/modules/c06-runbook-semantics/` |
| C07 Dispatch Bridges | L05 | hle-bridge | M040 bridge_contract, M041 zellij_dispatch, M042 atuin_qi_bridge, M043 devops_v3_probe, M044 stcortex_anchor_bridge, M045 watcher_notice_writer | `ai_specs/modules/c07-dispatch-bridges/` |
| C08 CLI Surface | L06 | hle-cli (extended) | M046 cli_args, M047 cli_run, M048 cli_verify, M049 cli_daemon_once, M050 cli_status | `ai_specs/modules/c08-cli-surface/` |
| C09 DevOps/QI Lane | L06/scripts | scripts/ | M051 verify_sync_script, M052 quality_gate_script, M053 module_map_script, M054 layer_dag_script | `ai_specs/modules/c09-devops-qi-lane/` |

Authority documents:
- Per-module specs: `ai_specs/modules/c0x-<slug>/M0xx_<NAME>.md`
- Per-module sheets: `ai_docs/modules/M0xx_<NAME>.md` (LITE format)
- Cluster overviews: `ai_specs/modules/c0x-<slug>/00-CLUSTER-OVERVIEW.md`
- Index: `ai_specs/modules/INDEX.md`

Workspace lints under which all stubs live: `forbid(unsafe_code); deny unwrap_used/expect_used/panic/todo/dbg_macro` (production code); test code allowed `clippy::all + warnings + restriction-class allows` for ergonomic assertions.

## Scripts and verifier families

All scaffold/M0 verifier scripts are listed in `scripts/` and must be reflected in `plan.toml` when they become authority-bearing gates. The active verifier families are:

| Family | Script anchor | Purpose |
|---|---|---|
| Sync and topology | `scripts/verify-sync.sh`, `scripts/verify-module-map.sh`, `scripts/verify-layer-dag.sh` | Keep `plan.toml`, this UltraMap, layers, modules, and structural counts aligned. |
| Documentation | `scripts/verify-doc-links.sh`, `scripts/verify-vault-parity.sh` | Keep root docs, dedicated vault sections, and cross-file links coherent. |
| Claude/operator surfaces | `scripts/verify-claude-folder.sh`, `scripts/verify-bin-wrapper-parity.sh` | Keep `.claude/` rules/commands and `bin/hle-*` wrappers in parity with scripts. |
| Pattern intelligence | `scripts/verify-antipattern-registry.sh`, `scripts/verify-usepattern-registry.sh`, `scripts/verify-semantic-predicates.sh` | Preserve anti-pattern/use-pattern breadth and script/spec predicate traceability. |
| Evidence and receipts | `scripts/verify-receipt-schema.sh`, `scripts/verify-receipt-graph.sh`, `scripts/verify-framework-hash-freshness.sh` | Keep authorization, provenance, hash, and receipt graph authority durable. |
| Safety and negative controls | `scripts/verify-negative-controls.sh`, `scripts/verify-bounded-logs.sh`, `scripts/verify-script-safety.sh`, `scripts/verify-skeleton-only.sh` | Block false PASS, unbounded output, live side effects, and unauthorized runtime drift. |
| Local M0 | `scripts/verify-local-loop-helpers.sh`, `scripts/verify-m0-runtime.sh` | Verify bounded local helpers, `hle run`, `hle verify`, `hle daemon --once`, verifier authority, JSONL receipts, and local SQLite schema markers. |

## Runtime and boundary map

| Capability | Status | Guardrail |
|---|---|---|
| Scaffold documentation/spec substrate | Active | `scripts/quality-gate.sh --scaffold` remains a required baseline. |
| Local M0 one-shot runtime | Active | The codebase needs to be 'one shotted': `scripts/quality-gate.sh --m0` must pass; runtime commands must be foreground/finite; daemon mode requires `--once`. |
| Local JSONL receipt ledgers | Active | Kept under `.deployment-work/runtime/`; verifier emits PASS/FAIL/AWAITING_HUMAN evidence. |
| Live Habitat write integrations | Forbidden | `plan.toml` keeps `live_integrations = false`; script safety gate scans for forbidden side effects. |
| Cron/systemd/service deployment | Forbidden | `plan.toml` keeps `cron_daemons = false`; use only bounded foreground helpers. |
| Final deployment claim | Not authorized | Requires fresh gates, manifests, receipts, and independent review. |

## External authority roots

| Root | Role |
|---|---|
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework` | Source deployment framework, receipts, handoffs, authorization packet, false-100 traps. |
| `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine` | Dedicated review vault and scaffold/M0 reasoning mirror. |
| `docs/workflows/scaffold-m0-broadening-review-20260510T112840Z.md` | Current Weaver directory/file review and broadening receipt. |
| `the_maintenance_engine_v2/ai_specs/m1-foundation-specs/` | MEv2 L1 gold-standard spec sheet exemplar — referenced by `ai_specs/modules/INDEX.md`. |
| `stcortex hle namespace` (`127.0.0.1:3000`) | Pioneer memory substrate; anchors `hle:coding-readiness-sync`, `hle:local-m0-boundary`, `hle:verification-gates`, `hle:module-specs-authored-2026-05-11`. |

## Module specs (50 modules across 9 clusters)

| Authority | Path | Contents |
|---|---|---|
| Module specs index | `ai_specs/modules/INDEX.md` | All 50 module specs M005-M054 across C01-C09 (authored 2026-05-11 by 8 parallel `rust-pro` agents + Claude for C09) |
| Cluster overviews | `ai_specs/modules/c<NN>-<slug>/00-CLUSTER-OVERVIEW.md` | Per-cluster purpose, file map, dependency graph, error code range, design principles |
| Per-module specs | `ai_specs/modules/c<NN>-<slug>/M0xx_<NAME>.md` | Per-module: header (file/LOC/tests/role), Types-at-a-Glance, Rust signatures, method/trait tables, Design Notes, cluster invariants |
| Master index | `MASTER_INDEX.md` | Single navigational entry point linking every authority surface |

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

## DevOps V3 integration plan

Read-only integration only before explicit live-integration authorization. Future bridge must write receipts and pass verifier authority gates.


## Coding readiness sync — Weaver 2026-05-10

Status: ready for the next authorized coding slice inside bounded local M0. This is not a live-integration or deployment clearance.

### Layer ↔ module ↔ source alignment

| Layer | Module | Source authority | Runtime status | Verification authority |
|---|---|---|---|---|
| L01 Foundation | M001_SUBSTRATE_TYPES | `crates/substrate-types/src/lib.rs` | Active local types | `scripts/verify-sync.sh`, `cargo test --workspace --all-targets` |
| L02 Persistence | M003_SUBSTRATE_EMIT + migration schema | `crates/substrate-emit/src/lib.rs`, `migrations/0001_scaffold_schema.sql` | Local JSONL + local schema markers | `scripts/verify-m0-runtime.sh`, `scripts/verify-receipt-schema.sh` |
| L03 Workflow Executor | M003_SUBSTRATE_EMIT | `execute_local_workflow` in `crates/substrate-emit/src/lib.rs` | Active bounded local executor | M0 gate + Rust unit tests |
| L04 Verification | M002_SUBSTRATE_VERIFY | `crates/substrate-verify/src/lib.rs` | Active verifier authority | `verify_authorization`, `verify_step`, AWAITING_HUMAN tests |
| L05 Dispatch Bridges | future bridge docs only | `ai_specs/S09_DEVOPS_V3_READ_ONLY_INTEGRATION.md` | Read-only planning only | live write integrations remain forbidden |
| L06 CLI | M004_HLE_CLI | `crates/hle-cli/src/main.rs` | `hle run`, `hle verify`, `hle daemon --once` | CLI integration tests + M0 gate |
| L07 Runbook Semantics | M002 + M003 | verifier/emitter crates + `runbooks/` | AWAITING_HUMAN is preserved | `scripts/verify-runbook-schema.sh`, M0 runtime tests |

### Bidirectional knowledge links

- Dedicated Obsidian vault HOME: `obsidian://open?vault=habitat-loop-engine&file=HOME`
- Dedicated Obsidian filesystem root: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- Obsidian reciprocal notes should point back to this repository root: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- STcortex namespace: `hle`.
- STcortex anchors: `hle:scaffold-m0-broadening-review`, `hle:directory-md-comprehensiveness-matrix`, `hle:local-m0-boundary`, `hle:verification-gates`, `hle:coding-readiness-sync`.

### Coding gate

Before new code lands, run:

```bash
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --scaffold --json
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --m0 --json
```

After those gates, refresh `SHA256SUMS.txt` because M0 runtime ledgers are intentionally rewritten by the gate.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a root or topic documentation surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `ULTRAMAP.md`.
- Parent directory: `.`.
- Adjacent markdown siblings sampled: ARCHITECTURE.md, CHANGELOG.md, CLAUDE.local.md, CLAUDE.md, CODEOWNERS.md, HARNESS_CONTRACT.md, QUALITY_BAR.md, QUICKSTART.md.
- This file should be read with `plan.toml`, `docs/SCRIPT_SPEC_PREDICATE_MAP.md`, and `.deployment-work/status/scaffold-status.json` when deciding whether a change is scaffold-only, local-M0, or outside authorization.

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
- If this file drifts from `plan.toml`, source modules, or verifier maps, update the authority files first and rerun gates.

### Next maintenance action
On the next broadening pass, re-run the markdown census, inspect files with fewer than 180 words or missing boundary/verification terms, update this section with any new authority roots, then refresh manifests and rerun both quality gates.

