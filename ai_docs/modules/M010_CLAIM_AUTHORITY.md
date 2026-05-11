# M010 CLAIM_AUTHORITY

Layer: L01
Crate: `crates/hle-core`
Cluster: C02_AUTHORITY_STATE
Status: planned (compile-safe stub authored 2026-05-11; full implementation gated by future authorization).

## Planned acceptance

- module compiles and passes smoke tests in `cargo test -p hle-core`;
- module spec authored at `ai_specs/modules/c02-authority-state/M010_CLAIM_AUTHORITY.md`;
- module listed in `plan.toml` and `ai_docs/CLUSTERED_MODULES.md`;
- live integrations and cron/daemon flags remain false.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> MASTER_INDEX.md -> ULTRAMAP.md -> ai_docs/CLUSTERED_MODULES.md -> ai_specs/modules/c02-authority-state/M010_CLAIM_AUTHORITY.md -> this module sheet -> crates/hle-core/src/authority/claim_authority.rs`.

- Previous authority: `ai_docs/layers/L01_FOUNDATION.md` and `ai_docs/CLUSTERED_MODULES.md`.
- Spec authority: `ai_specs/modules/c02-authority-state/M010_CLAIM_AUTHORITY.md`.
- Source authority: `crates/hle-core/src/authority/claim_authority.rs`.
- Reciprocal requirement: when this module changes, update its spec sheet, layer doc, ULTRAMAP.md, source comments/behavior, and the MASTER_INDEX cluster row before claiming deployment alignment.
