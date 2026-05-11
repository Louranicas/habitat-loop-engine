# Script / Spec / Predicate Map

Status: scaffold + bounded local-M0 audit map. The codebase needs to be 'one shotted': every runtime verifier path must prove bounded, foreground, finite execution. This file documents what each local verifier proves for CI/Watcher ingestion; it does not authorize live Habitat integrations, cron jobs, unbounded daemons, or deployment claims.

## Canonical gate

`scripts/quality-gate.sh --scaffold` is the canonical scaffold gate. `scripts/quality-gate.sh --m0` is the bounded local-M0 gate. The `--json` forms emit a `hle.quality_gate.v2` JSON report to stdout while streaming step logs to stderr.

## Predicate map

| Script or wrapper | Predicate checked | Primary spec / doc surface | Failure meaning |
| --- | --- | --- | --- |
| `scripts/verify-sync.sh` / `bin/hle-verify-sync` | Required root files exist; `plan.toml` casing is correct; S01-S13 specs, seven layer docs, and M001-M004 markers are aligned. | `plan.toml`, `ULTRAMAP.md`, `ai_specs/`, `ai_docs/layers/` | Scaffold inventory drift or missing authority map. |
| `scripts/verify-doc-links.sh` / `bin/hle-doc-links` | Markdown links resolve inside the scaffold tree. | `README.md`, docs, schematics, runbooks, `ai_docs/`, `ai_specs/` | Broken audit navigation or stale doc path. |
| `scripts/verify-claude-folder.sh` / `bin/hle-claude-folder` | Local `.claude` rules, commands, context, and agents are present for scaffold operation. | `.claude/`, `CLAUDE.md`, `CLAUDE.local.md` | Missing local operating guidance. |
| `scripts/verify-antipattern-registry.sh` / `bin/hle-antipattern-registry` | Required anti-pattern registry files are present and indexed. | `ai_docs/anti_patterns/` | False-pass or safety pattern registry drift. |
| `scripts/verify-semantic-predicates.sh` / `bin/hle-semantic-predicates` | Semantic predicate receipt and QUALITY_BAR expose HLE-SP-001..003; specs and anti-pattern docs include acceptance/detector terms. | `docs/quality/semantic-predicates.md`, `QUALITY_BAR.md`, `ai_specs/`, `ai_docs/anti_patterns/` | Semantic scaffold bars are missing or not mapped to checks. |
| `scripts/verify-module-map.sh` / `bin/hle-module-map` | M001-M004 module markers exist in both code-module map and plan. | `ai_docs/CODE_MODULE_MAP.md`, `plan.toml` | Module inventory drift. |
| `scripts/verify-layer-dag.sh` / `bin/hle-layer-dag` | Layer graph is present and acyclic enough for scaffold review. | `schematics/layer-dag.md`, `ai_docs/layers/` | Layer topology drift. |
| `scripts/verify-receipt-schema.sh` / `bin/hle-receipt-schema` | JSON schemas parse; scaffold receipts carry `^Verdict`, `^Manifest_sha256`, `^Framework_sha256`, and `^Counter_evidence_locator`; split hash anchors are 64-char lowercase SHA-256 values. | `schemas/receipt.schema.json`, `.deployment-work/receipts/`, `HARNESS_CONTRACT.md` | Receipt cannot serve as unambiguous evidence. |
| `scripts/verify-negative-controls.sh` / `bin/hle-negative-controls` | Negative fixtures stay negative and do not accidentally satisfy status/receipt predicates. | `tests/fixtures/negative/` | False-pass resistance has regressed. |
| `scripts/verify-runbook-schema.sh` / `bin/hle-runbook-schema` | Runbook schema surfaces remain present for awaiting-human semantics. | `runbooks/`, `schemas/status.schema.json`, `ai_specs/S10_RUNBOOK_SEMANTICS_AND_AWAITING_HUMAN_FSM.md` | Runbook scaffold contract drift. |
| `scripts/verify-receipt-graph.sh` / `bin/hle-receipt-graph` | At least one receipt exists and each receipt has the required split-anchor graph fields. | `.deployment-work/receipts/`, `schematics/receipt-graph.md` | Receipt graph is absent or unanchored. |
| `scripts/verify-test-taxonomy.sh` / `bin/hle-test-taxonomy` | Test taxonomy surfaces exist for scaffold/unit/integration coverage naming. | `tests/`, `QUALITY_BAR.md` | Test scope or naming drift. |
| `scripts/verify-bounded-logs.sh` / `bin/hle-bounded-logs` | Log/status artifacts remain bounded and scaffold-safe. | `.deployment-work/status/`, runbooks | Evidence surfaces risk unbounded output. |
| `scripts/verify-usepattern-registry.sh` / `bin/hle-usepattern-registry` | Required use-pattern registry docs are present. | `ai_docs/use_patterns/` | Positive scaffold patterns are missing or under-indexed. |
| `scripts/verify-skeleton-only.sh` / `bin/hle-skeleton-only` | Rust files remain under the scaffold LOC cap while `m0_runtime = false`; when M0 is authorized, live integrations and cron/daemon flags remain false. | `crates/`, `plan.toml` | Skeleton boundary drift or unauthorized live runtime scope. |
| `scripts/verify-m0-runtime.sh` / `bin/hle-m0-runtime` | M0 local runtime files, CLI markers, verifier markers, and SQLite schema surfaces exist when `m0_runtime = true`. | `plan.toml`, `crates/`, `migrations/`, `examples/` | M0 local implementation surface is missing or not verifier-gated. |
| `scripts/verify-framework-hash-freshness.sh` / `bin/hle-framework-hash-freshness` | Latest authorization receipt `^Framework_sha256` resolves against the current deployment-framework manifest and target file hash. | `.deployment-work/receipts/`, external read-only framework manifest | Framework provenance hash is stale or unresolved. |
| `scripts/verify-vault-parity.sh` / `bin/hle-vault-parity` | Dedicated vault sections match the expected scaffold topic layout. | dedicated vault root, `vault/CONVENTIONS.md` | Vault parity drift. |
| `scripts/verify-bin-wrapper-parity.sh` / `bin/hle-bin-wrapper-parity` | Every verifier script has the expected exact `bin/hle-*` wrapper and no orphan wrappers exist. | `scripts/`, `bin/` | Wrapper drift or missing CLI parity. |
| `scripts/verify-script-safety.sh` / `bin/hle-script-safety` | Scripts and local Claude surfaces avoid forbidden side-effect patterns such as service starts, network fetches, package installs, or cron/daemon operations. | `scripts/`, `bin/`, `.claude/`, `CLAUDE.md` | Scaffold-only boundary violation risk. |
| `scripts/verify-local-loop-helpers.sh` / `bin/hle-local-loop-helpers` | Local loop helper remains read-only, bounded, argument-guarded, and free of mutating Kanban/service patterns. | `scripts/kanban-hle-workflow-monitor.py`, S06, S12 | Local helper can hang, mutate state, or become daemon-like. |
| `cargo fmt --check` | Rust formatting is stable. | `Cargo.toml`, `crates/` | Rust skeleton formatting drift. |
| `cargo check --workspace --all-targets` | Rust workspace skeleton compiles without executing M0 runtime behavior. | `crates/`, `Cargo.toml` | Compile-safe skeleton drift. |
| `cargo test --workspace --all-targets` | Rust scaffold tests pass. | `crates/`, `tests/` | Rust skeleton behavior/contract drift. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Rust skeleton has zero clippy warnings. | `crates/` | Warning-free quality bar drift. |
| `python3 tests/unit/test_manifest.py` | Python unit checks for manifest/spec/layer/receipt presence pass. | `tests/unit/`, root manifests | Manifest regression. |
| `python3 tests/integration/test_scaffold.py` | Python integration checks for sync and negative controls pass. | `tests/integration/`, `scripts/` | Scaffold verifier chain regression. |

## Receipt hash anchors

New receipts use split hash anchors:

- `^Manifest_sha256`: SHA-256 of the scaffold manifest evidence, currently `SHA256SUMS.txt` for the authorization receipt.
- `^Framework_sha256`: SHA-256 of the source/framework provenance that authorized scaffold creation.
- `^Source_sha256`: legacy compatibility alias only; do not use it as the sole hash anchor in new receipts.

## Watcher ingestion contract

A Watcher/CI reader should treat `verdict == "PASS"` in the JSON report as a summary of the same step predicates in this map. The textual PASS line remains available for humans, but verifier scripts remain the sole PASS/FAIL authority.
