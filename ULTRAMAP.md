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

| Module | Layer | Crate/Path | Tests | Specs |
|---|---|---|---|---|
| M001 | L01 | crates/substrate-types | tests/unit/test_manifest.py | S01,S02,S13 |
| M002 | L04 | crates/substrate-verify | tests/unit/test_manifest.py | S04,S08,S13 |
| M003 | L03 | crates/substrate-emit | tests/unit/test_manifest.py | S03,S05,S13 |
| M004 | L06 | crates/hle-cli | tests/integration/test_scaffold.py | S06 |

## Scripts

All scaffold scripts are listed in `scripts/` and must be reflected in `plan.toml`.

## DevOps V3 integration plan

Read-only integration only before explicit live-integration authorization. Future bridge must write receipts and pass verifier authority gates.
