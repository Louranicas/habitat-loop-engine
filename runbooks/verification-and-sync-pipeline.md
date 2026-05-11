# Verification & Sync Pipeline Runbook

The Habitat Loop Engine quality gate orchestrates 22 verify scripts + 4 cargo lanes + 2 python lanes through `scripts/quality-gate.sh`. This runbook documents the full chain, family-grouped, with what each predicate guards and what failure means. The codebase needs to be 'one shotted': every step runs once, foreground, finite, with JSON-anchored evidence.

## Modes

| Mode | Trigger | Verify scripts run | Notes |
|---|---|---|---|
| `--scaffold` | `scripts/quality-gate.sh --scaffold --json` | 21 (all except `verify-m0-runtime`) | Canonical baseline; runs `verify-local-loop-helpers` and `verify-source-topology` |
| `--m0` | `scripts/quality-gate.sh --m0 --json` | 22 (adds `verify-m0-runtime`) | Asserts M0 runtime files, CLI markers, schema markers when `m0_runtime = true` in `plan.toml` |

Both modes run all 4 cargo lanes (`fmt --check`, `check`, `test`, `clippy -D warnings`) and both python lanes. Total: 27 steps in scaffold mode, 28 in M0 mode.

Both gates emit `hle.quality_gate.v2` JSON to stdout (stderr streams logs). PASS requires every step `exit_code == 0` AND `status == PASS` AND non-vacuity floors satisfied. Watcher/CI consumers must read `verdict` from JSON; the textual PASS line is informational only.

## Family groups

### Sync & topology

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-sync.sh` | `bin/hle-verify-sync` | Required root files exist; `plan.toml` casing; S01-S13 specs; 7 layer docs; M001-M004 markers in `plan.toml` and `ULTRAMAP.md` | Scaffold inventory drift or missing authority map |
| `verify-module-map.sh` | `bin/hle-module-map` | M001-M004 module markers exist in `ai_docs/CODE_MODULE_MAP.md` and `plan.toml` | Module inventory drift |
| `verify-layer-dag.sh` | `bin/hle-layer-dag` | Layer graph present and acyclic enough for scaffold review | Layer topology drift |
| `verify-source-topology.sh` | `bin/hle-source-topology` | Full-codebase planned-vs-built topology check; `--strict` enforces 50-module target when M0 implementation begins | Source topology drift |

### Documentation

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-doc-links.sh` | `bin/hle-doc-links` | Markdown links resolve inside scaffold tree | Broken audit navigation |
| `verify-vault-parity.sh` | `bin/hle-vault-parity` | Dedicated Obsidian vault sections match scaffold topic layout | Vault parity drift |

### Claude/operator surface

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-claude-folder.sh` | `bin/hle-claude-folder` | `.claude/` rules, commands, context, agents present for scaffold operation | Missing local operating guidance |
| `verify-bin-wrapper-parity.sh` | `bin/hle-bin-wrapper-parity` | 1:1 `scripts/verify-*.sh` ↔ `bin/hle-*` parity, no orphan wrappers | Wrapper drift |

### Pattern intelligence

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-antipattern-registry.sh` | `bin/hle-antipattern-registry` | `ai_docs/anti_patterns/` registry present and indexed | Anti-pattern registry drift |
| `verify-usepattern-registry.sh` | `bin/hle-usepattern-registry` | `ai_docs/use_patterns/` registry present | Use-pattern registry drift |
| `verify-semantic-predicates.sh` | `bin/hle-semantic-predicates` | HLE-SP-001..003 receipt + `QUALITY_BAR.md` predicates present; specs/anti-pattern docs include detector terms | Semantic scaffold bars missing or unmapped |

### Evidence & receipts

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-receipt-schema.sh` | `bin/hle-receipt-schema` | JSON schemas parse; receipts carry `^Verdict`, `^Manifest_sha256`, `^Framework_sha256`, `^Counter_evidence_locator`; split hash anchors are 64-char lowercase SHA-256 | Receipt cannot serve as evidence |
| `verify-receipt-graph.sh` | `bin/hle-receipt-graph` | At least one receipt exists; each carries the split-anchor graph fields | Receipt graph absent or unanchored |
| `verify-framework-hash-freshness.sh` | `bin/hle-framework-hash-freshness` | Latest authorization receipt's `^Framework_sha256` resolves against current deployment-framework manifest | Framework provenance stale or unresolved |

### Safety & negative controls

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-negative-controls.sh` | `bin/hle-negative-controls` | Negative fixtures stay negative — do not accidentally satisfy status/receipt predicates | False-pass resistance regressed |
| `verify-bounded-logs.sh` | `bin/hle-bounded-logs` | Log/status artifacts remain bounded and scaffold-safe | Evidence surfaces risk unbounded output |
| `verify-script-safety.sh` | `bin/hle-script-safety` | Scripts and `.claude/` surfaces avoid forbidden side effects (service starts, network fetches, package installs, cron/daemon ops) | Scaffold-only boundary violation risk |
| `verify-skeleton-only.sh` | `bin/hle-skeleton-only` | Rust files remain under scaffold LOC cap while `m0_runtime = false`; live integrations and cron/daemon flags remain false even when M0 is authorized | Skeleton boundary drift |
| `verify-test-taxonomy.sh` | `bin/hle-test-taxonomy` | Test taxonomy surfaces exist for scaffold/unit/integration coverage naming | Test scope or naming drift |

### Local M0

| Script | Wrapper | Predicate | Failure means |
|---|---|---|---|
| `verify-local-loop-helpers.sh` | `bin/hle-local-loop-helpers` | `scripts/kanban-hle-workflow-monitor.py` remains read-only, bounded, argument-guarded, free of mutating Kanban/service patterns | Local helper risks hang or daemon-like behavior |
| `verify-m0-runtime.sh` | `bin/hle-m0-runtime` | M0 runtime files, CLI markers, verifier markers, SQLite schema markers exist when `m0_runtime = true` | M0 surface missing or not gated |
| `verify-runbook-schema.sh` | `bin/hle-runbook-schema` | Runbook schema surfaces remain present for AwaitingHuman semantics | Runbook scaffold contract drift |

### Cargo lanes (run by `scripts/quality-gate.sh`)

| Step | Predicate | Failure means |
|---|---|---|
| `cargo fmt --check` | Rust formatting stable across workspace | Formatting drift |
| `cargo check --workspace --all-targets` | Workspace compiles without runtime execution | Compile-safe skeleton drift |
| `cargo test --workspace --all-targets` | Rust scaffold tests pass | Behavior/contract drift |
| `cargo clippy --workspace --all-targets -- -D warnings` | Zero clippy warnings (workspace lints: pedantic warn; unwrap/expect/panic/todo/dbg deny) | Quality bar drift |

### Python lanes (run by `scripts/quality-gate.sh`)

| Step | Predicate | Failure means |
|---|---|---|
| `python3 tests/unit/test_manifest.py` | Manifest, spec, layer, receipt presence | Manifest regression |
| `python3 tests/integration/test_scaffold.py` | Sync and negative controls integration | Scaffold verifier chain regression |

## Canonical sequence

The exact step order is the source of truth for predicate dependencies. Read it from any recent gate run:

```bash
jq '.steps[] | .command' .deployment-work/status/scaffold-status.json
jq '.steps[] | .command' .deployment-work/status/quality-gate-*.json | head -40
```

Do not reorder: dependencies between predicates assume this sequence. For example, `verify-sync` must precede `verify-module-map` (the latter assumes the modules table parses); `cargo check` must precede `cargo test`; `verify-bin-wrapper-parity` must precede `verify-script-safety` (safety scans walk only registered wrappers).

## Failure interpretation

Any single step failure aborts the gate and returns non-zero. The JSON report names the failing step plus its `exit_code` and `duration_ms`. To debug:

1. Read the JSON status: `jq '.steps[] | select(.status != "PASS")' <gate-json>`
2. Run the failing predicate alone: `bin/hle-<name>` or `scripts/verify-<name>.sh`
3. Identify the family group above and the "Failure means" column.
4. Fix the upstream authority surface (specs, plan.toml, ULTRAMAP, source) — do NOT patch the verifier to silence the signal.

## Receipt anchored fields (per `HARNESS_CONTRACT.md`)

New scaffold receipts use split hash anchors:

- `^Manifest_sha256` — SHA-256 of `SHA256SUMS.txt` at receipt time
- `^Framework_sha256` — SHA-256 of the deployment-framework provenance file at receipt time
- `^Source_sha256` — legacy compatibility alias only; do not use as sole hash anchor in new receipts

Required anchored fields per receipt:

- `^Verdict:` — verdict identifier (e.g., `SCAFFOLD_CREATED`, `SCAFFOLD_EXPANSION_AUTHORIZED`, `INFORMATIONAL`)
- `^Manifest_sha256:` and/or `^Framework_sha256:`
- `^Counter_evidence_locator:` — path(s) to falsification surfaces (e.g., `FALSE_100_TRAPS.md`, the gate JSON)

`verify-receipt-schema.sh` rejects receipts missing these fields or carrying values that are not 64-char lowercase SHA-256.

## Scope and boundaries

This runbook describes verifier chain behavior only. It does not authorize:

- live Habitat write integrations,
- cron, systemd, or background daemon installation,
- production deployment claims,
- bypassing the `begin M0` gate per `AUTHORIZATION_PHRASES.md`.

When the planned 46-module expansion lands (M005-M054 across clusters C01-C09), this runbook will gain new predicates per cluster — receipt SHA recompute (C01), type-state authority verification (C02), bounded-execution policy enforcement (C03), anti-pattern scanner events (C04), append-only ledger integrity (C05), runbook safety policy (C06), bridge contract parity (C07), CLI typed adapters (C08).

## When to consult this runbook

- Before changing any verifier script: read its predicate above to know what other surfaces depend on it.
- Before adding a new gate: confirm placement in the family groups + canonical order; add the matching `bin/hle-*` wrapper.
- After any failure: identify family + predicate; the "Failure means" column tells you which authority surface to fix first.
- Before declaring closure: confirm `^Manifest_sha256` resolves and `verdict == "PASS"` in JSON.

## Cross-references

- Predicate map: [`../docs/SCRIPT_SPEC_PREDICATE_MAP.md`](../docs/SCRIPT_SPEC_PREDICATE_MAP.md)
- Specs: [`../ai_specs/INDEX.md`](../ai_specs/INDEX.md) (S08 covers the Atuin/QI verification chain explicitly; S13 covers JSONL substrate integration contract)
- Anti-patterns: [`../ai_docs/anti_patterns/INDEX.md`](../ai_docs/anti_patterns/INDEX.md)
- Use-patterns: [`../ai_docs/use_patterns/INDEX.md`](../ai_docs/use_patterns/INDEX.md)
- Schemas: [`../schemas/`](../schemas/) (`receipt.schema.json`, `status.schema.json`, `plan.schema.json`)
- Quality bar: [`../QUALITY_BAR.md`](../QUALITY_BAR.md)
- Authorization phrases: `/home/louranicas/claude-code-workspace/loop-workflow-engine-project/shared-context/loop-workflow-engine/deployment-framework/AUTHORIZATION_PHRASES.md`
