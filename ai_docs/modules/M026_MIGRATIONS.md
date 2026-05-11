# M026 MIGRATIONS

Layer: L02
Crate: `crates/hle-storage`
Cluster: C05_PERSISTENCE_LEDGER
Status: planned (compile-safe stub authored 2026-05-11; full implementation gated by future authorization).

## Planned acceptance

- module compiles and passes smoke tests in `cargo test -p hle-storage`;
- module spec authored at `ai_specs/modules/c05-persistence-ledger/M026_MIGRATIONS.md`;
- module listed in `plan.toml` and `ai_docs/CLUSTERED_MODULES.md`;
- live integrations and cron/daemon flags remain false.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> MASTER_INDEX.md -> ULTRAMAP.md -> ai_docs/CLUSTERED_MODULES.md -> ai_specs/modules/c05-persistence-ledger/M026_MIGRATIONS.md -> this module sheet -> crates/hle-storage/src/migrations.rs`.

- Previous authority: `ai_docs/layers/L02_PERSISTENCE.md` and `ai_docs/CLUSTERED_MODULES.md`.
- Spec authority: `ai_specs/modules/c05-persistence-ledger/M026_MIGRATIONS.md`.
- Source authority: `crates/hle-storage/src/migrations.rs`.
- Reciprocal requirement: when this module changes, update its spec sheet, layer doc, ULTRAMAP.md, source comments/behavior, and the MASTER_INDEX cluster row before claiming deployment alignment.
