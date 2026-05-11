# M019 RETRY_POLICY

Layer: L03
Crate: `crates/hle-executor`
Cluster: C03_BOUNDED_EXECUTION
Status: planned (compile-safe stub authored 2026-05-11; full implementation gated by future authorization).

## Planned acceptance

- module compiles and passes smoke tests in `cargo test -p hle-executor`;
- module spec authored at `ai_specs/modules/c03-bounded-execution/M019_RETRY_POLICY.md`;
- module listed in `plan.toml` and `ai_docs/CLUSTERED_MODULES.md`;
- live integrations and cron/daemon flags remain false.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> MASTER_INDEX.md -> ULTRAMAP.md -> ai_docs/CLUSTERED_MODULES.md -> ai_specs/modules/c03-bounded-execution/M019_RETRY_POLICY.md -> this module sheet -> crates/hle-executor/src/retry_policy.rs`.

- Previous authority: `ai_docs/layers/L03_WORKFLOW_EXECUTOR.md` and `ai_docs/CLUSTERED_MODULES.md`.
- Spec authority: `ai_specs/modules/c03-bounded-execution/M019_RETRY_POLICY.md`.
- Source authority: `crates/hle-executor/src/retry_policy.rs`.
- Reciprocal requirement: when this module changes, update its spec sheet, layer doc, ULTRAMAP.md, source comments/behavior, and the MASTER_INDEX cluster row before claiming deployment alignment.
