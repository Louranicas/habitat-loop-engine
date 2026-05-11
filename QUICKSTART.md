# Quickstart

## Prerequisites

- Rust toolchain with `cargo`.
- Python 3 for scaffold verification scripts.
- No external services required for scaffold verification.

## Scaffold/local-M0 verification

```bash
scripts/quality-gate.sh --scaffold
scripts/quality-gate.sh --m0
```

## Running M0 locally

The bounded local CLI is one-shot only:

```bash
cargo run -p hle-cli -- run --workflow examples/workflow.example.toml --ledger .deployment-work/status/m0-local-ledger.jsonl
cargo run -p hle-cli -- verify --ledger .deployment-work/status/m0-local-ledger.jsonl
cargo run -p hle-cli -- daemon --once --workflow examples/workflow.example.toml --ledger .deployment-work/status/m0-local-ledger.jsonl
```

Do not create services, cron jobs, live Habitat write integrations, or unbounded daemons.

## Verification pipeline

The scaffold and M0 quality gates orchestrate 22 verify scripts + 4 cargo lanes + 2 python lanes. Run them as one-shot foreground commands:

```bash
scripts/quality-gate.sh --scaffold --json | tee .deployment-work/status/quality-gate-scaffold-latest.json
scripts/quality-gate.sh --m0 --json       | tee .deployment-work/status/quality-gate-m0-latest.json
sha256sum -c SHA256SUMS.txt
```

Each verify script has a 1:1 wrapper at `bin/hle-*` (parity enforced by `scripts/verify-bin-wrapper-parity.sh`). To run an individual predicate without the full chain:

```bash
bin/hle-verify-sync                  # root files, S01-S13 specs, 7 layer docs, M001-M004 markers
bin/hle-doc-links                    # markdown link resolution across scaffold tree
bin/hle-module-map                   # M001-M004 markers in CODE_MODULE_MAP and plan.toml
bin/hle-layer-dag                    # layer DAG present and acyclic enough
bin/hle-receipt-schema               # ^Verdict / ^Manifest_sha256 / ^Framework_sha256 anchors
bin/hle-receipt-graph                # receipt graph + split-hash anchor authority
bin/hle-m0-runtime                   # M0 runtime files, CLI markers, schema markers
bin/hle-skeleton-only                # Rust skeleton boundary while m0_runtime gate transitions
bin/hle-script-safety                # forbid live service starts, cron, network fetches, daemons
bin/hle-vault-parity                 # dedicated Obsidian vault topic layout parity
bin/hle-bin-wrapper-parity           # 1:1 scripts/ ↔ bin/hle-* mapping
bin/hle-framework-hash-freshness     # ^Framework_sha256 resolves against current manifest
```

PASS requires every step `exit_code == 0` AND `status == PASS` AND non-vacuity floors satisfied. Full predicate map: `docs/SCRIPT_SPEC_PREDICATE_MAP.md`. Operator runbook: `runbooks/verification-and-sync-pipeline.md`. Status JSON lands in `.deployment-work/status/quality-gate-*.json`.

## Local DB location

Future local DB path: `.deployment-work/hle-local.sqlite3`.

## Receipt location

Scaffold and future runtime receipts live under `.deployment-work/receipts/`.

## Planned coding readiness

The full codebase target is 50 modules across 9 clusters (C01-C09), enumerated in `plan.toml [[planned_modules]]` with `[full_codebase] status = "planned_topology_incomplete"`. Today's authorized scope is M001-M004 (substrate-types, substrate-verify, substrate-emit, hle-cli). Expansion to M005-M054 is planned but **gated** — implementation requires the explicit `begin M0` phrase per `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`.

Cluster summary:

| Cluster | Crate(s) | Modules |
|---|---|---|
| C01 Evidence Integrity | hle-core, hle-storage, hle-verifier | receipt_hash, claims_store, receipts_store, receipt_sha_verifier, final_claim_evaluator |
| C02 Authority & State | hle-core, hle-executor, hle-verifier | claim_authority, workflow_state, state_machine, status_transitions, claim_authority_verifier |
| C03 Bounded Execution | hle-executor | bounded, local_runner, phase_executor, timeout_policy, retry_policy |
| C04 Anti-Pattern Intelligence | hle-core, hle-storage, hle-verifier | anti_pattern_scanner, anti_pattern_events, test_taxonomy, test_taxonomy_verifier, false_pass_auditor |
| C05 Persistence Ledger | hle-storage | pool, migrations, workflow_runs, workflow_ticks, evidence_store, verifier_results_store, blockers_store |
| C06 Runbook Semantics | hle-runbook | runbook_{schema,parser,phase_map,human_confirm,manual_evidence,scaffold,incident_replay,safety_policy} |
| C07 Dispatch Bridges | hle-bridge | bridge_contract, zellij_dispatch, atuin_qi_bridge, devops_v3_probe, stcortex_anchor_bridge, watcher_notice_writer |
| C08 CLI Surface | hle-cli | cli_args, cli_run, cli_verify, cli_daemon_once, cli_status |
| C09 DevOps/QI Lane | scripts/ | verify_sync_script, quality_gate_script, module_map_script, layer_dag_script (already exist; M-IDs pending renumber) |

M-ID renumbering: the planned table currently uses M001-M050 and collides with the existing `[[modules]]` M001-M004. The renumber to M005-M054 is queued and will land alongside the expansion.

## Troubleshooting

- If `verify-sync` fails, inspect `plan.toml` and `ULTRAMAP.md` first.
- If doc links fail, run `scripts/verify-doc-links.sh` and fix exact missing paths.
- If cargo fails, inspect the local Rust toolchain first, then run the scaffold/M0 quality gate again after code fixes.


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

## Coding-ready local M0 path

Use this path for the next authorized coding slice. It is local-only; it does not permit live Habitat writes, cron/systemd/service installation, or unbounded daemons.

```bash
cd /home/louranicas/claude-code-workspace/habitat-loop-engine
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --scaffold --json
RUSTUP_HOME=/home/louranicas/.rustup CARGO_HOME=/home/louranicas/.cargo PATH=/home/louranicas/.cargo/bin:$PATH scripts/quality-gate.sh --m0 --json
python3 - <<'PY'
from pathlib import Path
import hashlib
root=Path('.').resolve(); out=root/'SHA256SUMS.txt'
exclude_dirs={'.git','target','__pycache__','.obsidian'}; exclude_names={'SHA256SUMS.txt'}
entries=[]
for p in sorted(root.rglob('*')):
    rel=p.relative_to(root)
    if not p.is_file() or set(rel.parts).intersection(exclude_dirs):
        continue
    s=rel.as_posix()
    if s.startswith('.deployment-work/scratch/') or s.startswith('.claude/cache/'):
        continue
    if rel.name in exclude_names or rel.suffix=='.pyc' or rel.suffix in {'.db','.sqlite3'} or rel.name=='.env':
        continue
    entries.append(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  ./{s}")
out.write_text('\n'.join(entries)+'\n')
print(f'refreshed SHA256SUMS.txt with {len(entries)} entries')
PY
sha256sum -c SHA256SUMS.txt
```

Source/doc sync map:

- `MASTER_INDEX.md` is the single navigational entry point — every authority surface back-links here.
- `ULTRAMAP.md` is the layer/module/source map.
- `docs/SCRIPT_SPEC_PREDICATE_MAP.md` is the verifier/spec predicate authority.
- `plan.toml` is the machine-checkable module/script/status authority.
- `README.md` is the human entry point.
- `QUICKSTART.md` is the operator command path.
- `ai_specs/modules/INDEX.md` is the 50-module spec index (M005-M054, MEv2 L1 gold standard).
- `runbooks/verification-and-sync-pipeline.md` documents the 22+4+2 step canonical chain.

Bidirectional knowledge links:

- Obsidian: `obsidian://open?vault=habitat-loop-engine&file=HOME`
- Obsidian HOME path: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/HOME.md`
- Obsidian root Master Index: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/MASTER_INDEX.md`
- Obsidian nested Master Index: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/00 Index/Master Index.md`
- Obsidian Scaffold Operator Quickstart reciprocal: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine/00 Index/Scaffold Operator Quickstart.md`
- Active README reciprocal target: `/home/louranicas/claude-code-workspace/habitat-loop-engine/README.md`
- Active CLAUDE local overlay: `/home/louranicas/claude-code-workspace/habitat-loop-engine/CLAUDE.local.md`
- STcortex namespace: `hle`
- STcortex anchor to recall this readiness state: `hle:coding-readiness-sync`

If this quickstart changes, keep the Obsidian reciprocal blocks above in sync and refresh the active repo `SHA256SUMS.txt`, the vault `12 Receipts/VAULT_SHA256SUMS.txt`, and the project-level manifest when applicable.

## Comprehensiveness Review — Weaver 2026-05-10

### Purpose
This file is treated as a root or topic documentation surface within the Habitat Loop Engine deployment framework. Its purpose is to make the local-M0/scaffold surface legible to fresh agents without relying on unstated session memory.

### Authority and boundary
- Canonical active repository: `/home/louranicas/claude-code-workspace/habitat-loop-engine`.
- Deployment framework authority: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework`.
- Dedicated review vault: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/habitat-loop-engine/habitat-loop-engine`.
- Local M0 is authorized; the codebase needs to be 'one shotted': every runtime path must execute as a bounded, foreground, one-shot operation, and live Habitat write integrations, cron jobs, systemd services, background services, and unbounded daemons remain outside this file's authority.

### Files and directory relationship
- Current file: `QUICKSTART.md`.
- Parent directory: `.`.
- Adjacent markdown siblings sampled: ARCHITECTURE.md, CHANGELOG.md, CLAUDE.local.md, CLAUDE.md, CODEOWNERS.md, HARNESS_CONTRACT.md, QUALITY_BAR.md, README.md.
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

