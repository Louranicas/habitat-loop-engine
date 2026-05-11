# M049 CLI_DAEMON_ONCE

Layer: L06
Crate: `crates/hle-cli`
Cluster: C08_CLI_SURFACE
Status: planned (compile-safe stub authored 2026-05-11; full implementation gated by future authorization).

## Planned acceptance

- module compiles and passes smoke tests in `cargo test -p hle-cli`;
- module spec authored at `ai_specs/modules/c08-cli-surface/M049_CLI_DAEMON_ONCE.md`;
- module listed in `plan.toml` and `ai_docs/CLUSTERED_MODULES.md`;
- live integrations and cron/daemon flags remain false.

## Bidirectional deployment chain links

This module is part of the full end-to-end stack deployment chain: `CLAUDE.local.md -> README.md -> QUICKSTART.md -> MASTER_INDEX.md -> ULTRAMAP.md -> ai_docs/CLUSTERED_MODULES.md -> ai_specs/modules/c08-cli-surface/M049_CLI_DAEMON_ONCE.md -> this module sheet -> crates/hle-cli/src/daemon_once.rs`.

- Previous authority: `ai_docs/layers/L06_CLI.md` and `ai_docs/CLUSTERED_MODULES.md`.
- Spec authority: `ai_specs/modules/c08-cli-surface/M049_CLI_DAEMON_ONCE.md`.
- Source authority: `crates/hle-cli/src/daemon_once.rs`.
- Reciprocal requirement: when this module changes, update its spec sheet, layer doc, ULTRAMAP.md, source comments/behavior, and the MASTER_INDEX cluster row before claiming deployment alignment.
