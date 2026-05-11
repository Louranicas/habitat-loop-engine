# C05 Persistence Ledger — Cluster Overview

> **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02 | **Modules:** 7 (M025–M031)
> **Error Codes:** 2400–2499 | **Crate:** `crates/hle-storage`
> **Synergy:** Schema-first ledger ties every run, tick, evidence artifact, verifier verdict,
> and blocker into an append-only, causally linked proof surface.

---

## Purpose

C05 is the durable backbone of the Habitat Loop Engine. Every state transition that passes
through C02 (Authority/State) and C03 (Bounded Execution) lands here as a verifier-legible
ledger row. No row is ever mutated after insert on the append-only surfaces (M029, M030).
The pool (M025) and migrations (M026) guarantee that all tables exist before any caller
attempts an insert, and that the SQLite database is configured for WAL + foreign-key
enforcement before the first connection is handed out.

---

## File Map

```
crates/hle-storage/
├── src/
│   ├── lib.rs              # crate root — re-exports all C05 modules
│   ├── pool.rs             # M025 — connection pool trait and rusqlite wrapper
│   ├── migrations.rs       # M026 — schema-first migration runner
│   ├── workflow_runs.rs    # M027 — workflow run table abstraction
│   ├── workflow_ticks.rs   # M028 — tick ledger (freshness + causality)
│   ├── evidence_store.rs   # M029 — bounded evidence path/blob store (append-only)
│   ├── verifier_results_store.rs  # M030 — verifier verdict ledger (append-only)
│   └── blockers_store.rs   # M031 — blocked/awaiting-human state persistence
migrations/
└── 0001_scaffold_schema.sql  # authoritative schema source — M026 reads this
```

---

## Dependency Graph (Internal, C05 only)

```
M025 pool
  └─► M026 migrations   (migrations call pool to execute SQL)
        └─► M027 workflow_runs    (inserts/queries workflow_runs table)
              └─► M028 workflow_ticks   (FK: ticks.run_id -> workflow_runs.id)
                    ├─► M029 evidence_store      (FK: evidence.run_id)
                    ├─► M030 verifier_results_store  (FK: results.run_id)
                    └─► M031 blockers_store      (FK: blockers.run_id)
```

M025 is the root — nothing in C05 opens a connection without going through the pool trait.
M026 runs before any caller can use the higher modules (enforced by initialization order).
M027–M031 depend on M025 for connections and on M026 for schema presence.

---

## Cross-Cluster Dependencies

| Direction | Partner | Detail |
|-----------|---------|--------|
| C05 → C01 | receipts_store (M003) | M030 stores `receipt_sha` FKed to evidence_store SHA; receipt graph is built over both |
| C05 → C02 | state_machine (M008) | M027 run rows reflect status driven by M008 state transitions |
| C04 → C05 | false_pass_auditor (M020) | M020 queries M030 to verify PASS claims have an independent verifier receipt |
| C07 → C05 | stcortex_anchor_bridge (M040) | read-only reads from M029/M030 for anchor emission; no writes from bridge |
| C08 → C05 | cli_status (M046) | reads M027 run rows and M031 blocker rows for status display |
| C06 → C05 | runbook_human_confirm (M031_ref) | M031 is the backing store for runbook awaiting-human state |

---

## SQLite Configuration Invariants (All Tables)

All tables are created by M026 migrations and must satisfy these invariants before
any other module touches them:

```sql
PRAGMA journal_mode = WAL;       -- set once on first connection open
PRAGMA foreign_keys = ON;        -- set per-connection, every connection
PRAGMA strict = ON;              -- STRICT table keyword where supported
```

The pool (M025) enforces `foreign_keys = ON` and `journal_mode = WAL` in the
connection lifecycle hook. No caller module may bypass the pool to open a raw connection.

---

## Append-Only Policy (M029, M030)

M029 (`evidence_store`) and M030 (`verifier_results_store`) are strictly append-only:
- No `UPDATE` statements are permitted.
- No `DELETE` statements are permitted except via TTL predicates.
- Corrections are represented as new rows with a `supersedes_id` reference.
- TTL deletion uses the predicate `created_unix < (strftime('%s','now') - retention_secs)`
  — **never a literal integer** (framework §17.5 hard rule).

---

## Error Code Allocation (2400–2499)

| Code | Variant | Module | Condition |
|------|---------|--------|-----------|
| 2400 | `PoolInit` | M025 | Database file cannot be opened or WAL pragma fails |
| 2401 | `ConnectionAcquire` | M025 | Pool exhausted; timeout exceeded |
| 2402 | `ConnectionLeak` | M025 | Guard dropped without explicit release |
| 2410 | `MigrationNotFound` | M026 | Expected SQL file absent in migrations/ |
| 2411 | `MigrationChecksum` | M026 | Migration SQL content changed after apply |
| 2412 | `MigrationOrder` | M026 | Migration IDs not monotonically increasing |
| 2420 | `RunInsert` | M027 | workflow_runs INSERT failed |
| 2421 | `RunNotFound` | M027 | run_id not found for update/complete |
| 2422 | `RunStatusInvalid` | M027 | Status string not in allowed CHECK set |
| 2430 | `TickInsert` | M028 | workflow_ticks INSERT failed |
| 2431 | `TickParentMissing` | M028 | parent_tick_id references non-existent tick |
| 2440 | `EvidenceSizeExceeded` | M029 | Blob exceeds `MAX_EVIDENCE_BYTES` |
| 2441 | `EvidenceKindUnknown` | M029 | Evidence kind string not in known set |
| 2442 | `EvidenceInsert` | M029 | INSERT into evidence_store failed |
| 2450 | `VerifierInsert` | M030 | INSERT into verifier_results_store failed |
| 2451 | `VerifierVerdictInvalid` | M030 | Verdict not PASS/FAIL/AWAITING_HUMAN |
| 2460 | `BlockerInsert` | M031 | INSERT into blockers_store failed |
| 2461 | `BlockerNotFound` | M031 | Blocker row to resolve is absent |
| 2499 | `Storage` | any | Catch-all for unexpected rusqlite errors |

---

## Quality Gate Requirements

```
cargo check --workspace --all-targets             zero errors
cargo clippy --workspace -- -D warnings           zero warnings
cargo clippy --workspace -- -D warnings -W clippy::pedantic   zero warnings
cargo test --workspace --all-targets              >= 50 tests per module
Zero-tolerance: unsafe / unwrap / expect / panic  enforced by workspace lints
```

All schema DDL lives in `migrations/` — no inline `CREATE TABLE` strings in Rust source.
Module tests use an in-memory SQLite database (`:memory:`) seeded by M026 migrations.

---

*C05 Persistence Ledger Cluster Overview v1.0 | habitat-loop-engine*
