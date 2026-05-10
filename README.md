# Habitat Loop Engine

Status: scaffold-only repository created 2026-05-09T23:52:44Z. M0 runtime logic is intentionally not implemented.

Mission: provide a Habitat-grade local workflow loop engine with executor/verifier separation, durable receipts, runbook-aware awaiting-human semantics, and substrate-ready evidence trails.

## Current boundary

This repository is a scaffold substrate only. It may contain compile-safe Rust crate skeletons, documentation, schemas, scripts, and tests that verify scaffold coherence. It must not perform M0 runtime loop execution until Luke authorizes `begin M0`.

## Quick commands

```bash
scripts/verify-sync.sh
scripts/verify-doc-links.sh
scripts/verify-claude-folder.sh
scripts/verify-antipattern-registry.sh
scripts/verify-module-map.sh
scripts/verify-layer-dag.sh
scripts/verify-receipt-schema.sh
scripts/verify-negative-controls.sh
scripts/verify-runbook-schema.sh
scripts/verify-receipt-graph.sh
scripts/verify-test-taxonomy.sh
scripts/verify-bounded-logs.sh
scripts/verify-script-safety.sh
scripts/quality-gate.sh --scaffold
scripts/quality-gate.sh --scaffold --json
```

See [Script / Spec / Predicate Map](docs/SCRIPT_SPEC_PREDICATE_MAP.md) for the scaffold predicate chain, split receipt hash anchors, and CI/Watcher JSON report contract.

## Architecture

- L01 Foundation
- L02 Persistence
- L03 Workflow Executor
- L04 Verification
- L05 Dispatch Bridges
- L06 CLI
- L07 Runbook Semantics

## Non-goals before M0 authorization

- no live Habitat writes;
- no cron/daemon creation;
- no production executor loop;
- no final deployment claim;
- no S13 execution PASS without future round-trip verifier evidence.

## Link map

- [Quickstart](QUICKSTART.md)
- [Architecture](ARCHITECTURE.md)
- [UltraMap](ULTRAMAP.md)
- [Plan](plan.toml)
- [Quality Bar](QUALITY_BAR.md)
- [Harness Contract](HARNESS_CONTRACT.md)
