# M038 RUNBOOK_INCIDENT_REPLAY

Layer: L07
Crate: `crates/hle-runbook`
Cluster: C06_RUNBOOK_SEMANTICS
Status: planned (compile-safe stub authored 2026-05-11; full implementation gated by future authorization).

## Planned acceptance

- module compiles and passes smoke tests in `cargo test -p hle-runbook`;
- module spec authored at `ai_specs/modules/c06-runbook-semantics/M038_RUNBOOK_INCIDENT_REPLAY.md`;
- module listed in `plan.toml` and `ai_docs/CLUSTERED_MODULES.md`;
- live integrations and cron/daemon flags remain false.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> MASTER_INDEX.md -> ULTRAMAP.md -> ai_docs/CLUSTERED_MODULES.md -> ai_specs/modules/c06-runbook-semantics/M038_RUNBOOK_INCIDENT_REPLAY.md -> this module sheet -> crates/hle-runbook/src/incident_replay.rs`.

- Previous authority: `ai_docs/layers/L07_RUNBOOK_SEMANTICS.md` and `ai_docs/CLUSTERED_MODULES.md`.
- Spec authority: `ai_specs/modules/c06-runbook-semantics/M038_RUNBOOK_INCIDENT_REPLAY.md`.
- Source authority: `crates/hle-runbook/src/incident_replay.rs`.
- Reciprocal requirement: when this module changes, update its spec sheet, layer doc, ULTRAMAP.md, source comments/behavior, and the MASTER_INDEX cluster row before claiming deployment alignment.
